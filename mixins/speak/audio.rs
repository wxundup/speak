// Speak. — real microphone capture and speaker playback for proximity voice.
//
// This file is PURE Rust: no `crate::game`, so it compiles on its own. The
// bundle adds it to the game as `game::official_speak_audio` (the module
// `official_speak_audio` in `crates/horizon-client-render/src/lib.rs`), where
// `cpal` 0.16 already sits behind the `audio` feature; it compiles with and
// without that feature, and never links audio dependencies when it is off.
//
// What runs here:
//   - `set_capture_enabled(true)` opens the default input device on the
//     calling thread and resamples whatever it gives (any rate, any channel
//     count, F32/I16/U16) to mono 48 kHz PCM16. `false` drops the stream and
//     clears pending samples: the mic is open only while push-to-talk asks.
//   - `take_frames` hands out complete 20 ms frames (960 samples) and keeps
//     the remainder for the next call.
//   - `enqueue_playback` converts received mono 48 kHz voice to the output
//     device's rate and channels (the stream opens lazily on the first
//     received voice) and scales it by the listener's volume.
//   - `stop` drops both streams and all pending audio.
//
// `HORIZON_NO_AUDIO=1` disables all of it: opening capture or playback then
// returns an error instead of pretending to work. No default device is the
// same: an explicit error, never manufactured silence.
//
// cpal streams are not `Send`, so each stream lives in a thread-local owner
// on the thread that opened it. The callbacks share bounded queues through
// `Arc<Mutex<..>>`; no lock is held while a device is opened or a stream
// built. Linear interpolation keeps this adapter small.

/// The sample rate every voice frame travels at, mono.
pub const SAMPLE_RATE: u32 = 48_000;
/// One 20 ms frame of mono PCM16.
pub const FRAME_SAMPLES: usize = 960;
/// At most one second of captured audio queued before the oldest is dropped.
#[cfg(feature = "audio")]
const CAPTURE_MAX: usize = 48_000;

// ---------------------------------------------------------------------------
// Pure helpers. No device, no cpal: the tests drive these with synthetic PCM.
// ---------------------------------------------------------------------------

/// Average `channels` interleaved f32 samples down to mono.
pub fn to_mono(input: &[f32], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    input
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Linear-interpolation resampler that streams across calls.
///
/// `pos` is the fractional input-sample position of the next output sample,
/// measured from this chunk's start (`-1.0..` means "interpolate from
/// `prev`, the previous chunk's last sample"). It is carried across calls,
/// so chunks stitch without gaps or duplicated time. `prev` must be updated
/// by the caller to this chunk's last sample. `from == to` copies.
pub fn resample(input: &[f32], prev: f32, pos: &mut f64, from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let step = from as f64 / to as f64;
    let len = input.len() as f64;
    let mut p = pos.max(-1.0);
    let mut out = Vec::new();
    // Both interpolation endpoints must lie inside this chunk.
    while p + 1.0 < len {
        let i = p.floor();
        let frac = (p - i) as f32;
        // A negative index means "between the previous chunk's last sample
        // (`prev`) and this chunk's first".
        let (a, b) = if i < 0.0 {
            (prev, input[0])
        } else {
            (input[i as usize], input[i as usize + 1])
        };
        out.push(a + (b - a) * frac);
        p += step;
    }
    *pos = p - len;
    out
}

/// f32 in -1.0..=1.0 to PCM16, saturating outside it.
pub fn f32_to_i16(s: f32) -> i16 {
    (s * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i16
}

/// PCM16 to f32 in -1.0..=1.0.
pub fn i16_to_f32(s: i16) -> f32 {
    s as f32 / 32_767.0
}

/// Cut complete `FRAME_SAMPLES` frames off the front of `buf`; the partial
/// remainder stays.
pub fn frames_from(buf: &mut std::collections::VecDeque<i16>) -> Vec<Vec<i16>> {
    let mut frames = Vec::new();
    while buf.len() >= FRAME_SAMPLES {
        frames.push(buf.drain(..FRAME_SAMPLES).collect());
    }
    frames
}

// ---------------------------------------------------------------------------
// Without the `audio` feature: the API stays, no device is ever touched.
// ---------------------------------------------------------------------------

#[cfg(not(feature = "audio"))]
pub fn set_capture_enabled(_enabled: bool) -> Result<(), String> {
    Err("speak audio: built without the audio feature".into())
}

#[cfg(not(feature = "audio"))]
pub fn take_frames() -> Vec<Vec<i16>> {
    Vec::new()
}

#[cfg(not(feature = "audio"))]
pub fn enqueue_playback(_samples: &[i16], _volume: f32) -> Result<(), String> {
    Err("speak audio: built without the audio feature".into())
}

#[cfg(not(feature = "audio"))]
pub fn stop() {}

// ---------------------------------------------------------------------------
// With the `audio` feature: real capture and playback through cpal.
// ---------------------------------------------------------------------------

#[cfg(feature = "audio")]
mod live {
    use super::{SAMPLE_RATE, f32_to_i16, frames_from, i16_to_f32, resample};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use cpal::SampleFormat as F;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    /// Captured mono 48 kHz PCM16, plus the resampler's carried position.
    pub struct CaptureState {
        pub buf: VecDeque<i16>,
        prev: f32,
        pos: f64,
    }

    impl CaptureState {
        pub fn new() -> Self {
            Self {
                buf: VecDeque::new(),
                prev: 0.0,
                pos: 0.0,
            }
        }

        /// One callback's worth of mono samples at `rate`, resampled to
        /// `SAMPLE_RATE` and queued as PCM16. Also the pure path the tests
        /// drive without a device.
        pub fn push_mono(&mut self, mono: &[f32], rate: u32) {
            if mono.is_empty() {
                return;
            }
            let out = resample(mono, self.prev, &mut self.pos, rate, SAMPLE_RATE);
            self.prev = mono.last().copied().unwrap_or(self.prev);
            for s in out {
                self.buf.push_back(f32_to_i16(s));
            }
            while self.buf.len() > super::CAPTURE_MAX {
                self.buf.pop_front();
            }
        }
    }

    /// Received voice converted for the output device: its rate, its channel
    /// count, volume applied.
    pub struct PlaybackState {
        pub queue: VecDeque<f32>,
        prev: f32,
        pos: f64,
        rate: u32,
        channels: usize,
    }

    impl PlaybackState {
        pub fn new(rate: u32, channels: usize) -> Self {
            Self {
                queue: VecDeque::new(),
                prev: 0.0,
                pos: 0.0,
                rate,
                channels,
            }
        }

        /// Queue mono `SAMPLE_RATE` voice at `volume` for the device. Also the
        /// pure path the tests drive without a device.
        pub fn push_voice(&mut self, mono: &[f32], volume: f32) {
            if mono.is_empty() {
                return;
            }
            let scaled: Vec<f32> = mono.iter().map(|&s| s * volume).collect();
            let out = resample(&scaled, self.prev, &mut self.pos, SAMPLE_RATE, self.rate);
            self.prev = scaled.last().copied().unwrap_or(self.prev);
            for &s in &out {
                for _ in 0..self.channels {
                    self.queue.push_back(s);
                }
            }
            while self.queue.len() > self.rate as usize * self.channels * 2 {
                self.queue.pop_front();
            }
        }
    }

    /// Everything the calling thread owns: the streams (cpal streams are not
    /// `Send`, so they stay thread-local) and the queues their callbacks
    /// share with the API below.
    #[derive(Default)]
    struct Audio {
        capture: Option<cpal::Stream>,
        capture_shared: Option<Arc<Mutex<CaptureState>>>,
        output: Option<cpal::Stream>,
        output_failed: bool,
        playback_shared: Option<Arc<Mutex<PlaybackState>>>,
    }

    thread_local! {
        static AUDIO: RefCell<Audio> = RefCell::default();
    }

    /// `HORIZON_NO_AUDIO=1`: audio stays off, and asking for it is an error
    /// rather than a fake success.
    fn env_disabled() -> bool {
        std::env::var_os("HORIZON_NO_AUDIO").is_some()
    }

    /// A capture/playback mutex whose callback panicked: keep going with the
    /// state it left rather than dropping audio entirely.
    fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Build the input stream for one sample type.
    fn build_input<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        shared: Arc<Mutex<CaptureState>>,
        rate: u32,
        channels: usize,
    ) -> Result<cpal::Stream, String>
    where
        T: cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        device
            .build_input_stream(
                &config.config(),
                move |data: &[T], _| {
                    let mono: Vec<f32> = to_mono_typed(data, channels);
                    if mono.is_empty() {
                        return;
                    }
                    lock(&shared).push_mono(&mono, rate);
                },
                // shortcut: stream loss recovers on PTT release (input) or world change (output).
                |_e| {},
                None,
            )
            .map_err(|e| format!("speak audio: input stream: {e}"))
    }

    /// Average interleaved samples of any type down to mono f32.
    fn to_mono_typed<T>(data: &[T], channels: usize) -> Vec<f32>
    where
        T: cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        let channels = channels.max(1);
        data.chunks(channels)
            .map(|frame| {
                let sum: f32 = frame
                    .iter()
                    .map(|&s| <f32 as cpal::Sample>::from_sample(s))
                    .sum();
                sum / frame.len() as f32
            })
            .collect()
    }

    /// Build the output stream for one sample type.
    fn build_output<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        shared: Arc<Mutex<PlaybackState>>,
    ) -> Result<cpal::Stream, String>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        device
            .build_output_stream(
                &config.config(),
                move |data: &mut [T], _| {
                    let mut state = lock(&shared);
                    for out in data.iter_mut() {
                        *out = <T as cpal::Sample>::from_sample(
                            state.queue.pop_front().unwrap_or(0.0).clamp(-1.0, 1.0),
                        );
                    }
                },
                // shortcut: stream loss recovers on PTT release (input) or world change (output).
                |_e| {},
                None,
            )
            .map_err(|e| format!("speak audio: output stream: {e}"))
    }

    /// The output stream on the default device, in its default
    /// configuration: F32, I16 and U16 are handled, anything else is an
    /// explicit error.
    fn open_output(audio: &mut Audio) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("speak audio: no default output device")?;
        let config = device
            .default_output_config()
            .map_err(|e| format!("speak audio: output config: {e}"))?;
        let rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let shared = Arc::new(Mutex::new(PlaybackState::new(rate, channels)));
        let stream = match config.sample_format() {
            F::F32 => build_output::<f32>(&device, &config, shared.clone()),
            F::I16 => build_output::<i16>(&device, &config, shared.clone()),
            F::U16 => build_output::<u16>(&device, &config, shared.clone()),
            other => Err(format!(
                "speak audio: unsupported output sample format {other}"
            )),
        }?;
        stream
            .play()
            .map_err(|e| format!("speak audio: start output: {e}"))?;
        audio.output = Some(stream);
        audio.playback_shared = Some(shared);
        Ok(())
    }

    /// Open the default input device and start capturing to the shared queue.
    /// The stream is dropped (and the mic released) by `set_capture_enabled`
    /// or `stop` on this same thread.
    pub fn set_capture_enabled(enabled: bool) -> Result<(), String> {
        if !enabled {
            AUDIO.with(|audio| {
                let mut audio = audio.borrow_mut();
                audio.capture = None; // drops the stream: the mic is released
                audio.capture_shared = None; // ...and the pending frames with it
            });
            return Ok(());
        }
        if env_disabled() {
            return Err("speak audio: disabled by HORIZON_NO_AUDIO".into());
        }
        AUDIO.with(|audio| {
            let mut audio = audio.borrow_mut();
            if audio.capture.is_some() {
                return Ok(());
            }
            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .ok_or("speak audio: no default input device")?;
            let config = device
                .default_input_config()
                .map_err(|e| format!("speak audio: input config: {e}"))?;
            let rate = config.sample_rate().0;
            let channels = config.channels() as usize;
            let shared = Arc::new(Mutex::new(CaptureState::new()));
            let stream = match config.sample_format() {
                F::F32 => build_input::<f32>(&device, &config, shared.clone(), rate, channels),
                F::I16 => build_input::<i16>(&device, &config, shared.clone(), rate, channels),
                F::U16 => build_input::<u16>(&device, &config, shared.clone(), rate, channels),
                other => Err(format!(
                    "speak audio: unsupported input sample format {other}"
                )),
            }?;
            stream
                .play()
                .map_err(|e| format!("speak audio: start input: {e}"))?;
            audio.capture = Some(stream);
            audio.capture_shared = Some(shared);
            Ok(())
        })
    }

    /// Complete captured frames, in order. Empty without capture.
    pub fn take_frames() -> Vec<Vec<i16>> {
        AUDIO.with(|audio| {
            let audio = audio.borrow();
            let Some(shared) = &audio.capture_shared else {
                return Vec::new();
            };
            frames_from(&mut lock(shared).buf)
        })
    }

    /// Queue received mono 48 kHz voice for the speakers, scaled by
    /// `volume`. The output opens lazily here, on the first received voice.
    pub fn enqueue_playback(samples: &[i16], volume: f32) -> Result<(), String> {
        if env_disabled() {
            return Err("speak audio: disabled by HORIZON_NO_AUDIO".into());
        }
        if samples.is_empty() {
            return Ok(());
        }
        AUDIO.with(|audio| {
            let mut audio = audio.borrow_mut();
            if audio.output_failed {
                return Err("speak audio: output unavailable until voice restarts".into());
            }
            if audio.output.is_none()
                && let Err(error) = open_output(&mut audio)
            {
                audio.output_failed = true;
                return Err(error);
            }
            let Some(shared) = &audio.playback_shared else {
                return Ok(());
            };
            let mono: Vec<f32> = samples.iter().map(|&s| i16_to_f32(s)).collect();
            lock(shared).push_voice(&mono, volume);
            Ok(())
        })
    }

    /// Drop both streams and every pending sample.
    pub fn stop() {
        AUDIO.with(|audio| {
            let mut audio = audio.borrow_mut();
            audio.capture = None;
            audio.capture_shared = None;
            audio.output = None;
            audio.output_failed = false;
            audio.playback_shared = None;
        });
    }
}

#[cfg(feature = "audio")]
pub use live::{enqueue_playback, set_capture_enabled, stop, take_frames};

// ---------------------------------------------------------------------------
// Tests: synthetic PCM only, no device is ever opened or requested.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                (2.0 * std::f32::consts::PI * freq * t).sin() * amp
            })
            .collect()
    }

    #[test]
    fn to_mono_averages_channels() {
        let stereo = [1.0, 3.0, 5.0, 7.0, -1.0, 1.0];
        assert_eq!(to_mono(&stereo, 2), vec![2.0, 6.0, 0.0]);
        let mono = [0.25, -0.5];
        assert_eq!(to_mono(&mono, 1), mono);
        assert_eq!(to_mono(&mono, 0), mono); // 0 channels means mono already
    }

    #[test]
    fn resample_identity_copies() {
        let input = sine(440.0, FRAME_SAMPLES, 0.5);
        let mut pos = 0.0;
        assert_eq!(
            resample(&input, 0.0, &mut pos, SAMPLE_RATE, SAMPLE_RATE),
            input
        );
    }

    #[test]
    fn resample_upsample_doubles() {
        let input: Vec<f32> = (0..960).map(|i| i as f32).collect();
        let mut pos = 0.0;
        let out = resample(&input, 0.0, &mut pos, 24_000, 48_000);
        // step 0.5: out[4] sits on input index 2.
        assert_eq!(out.len(), 1918);
        assert!((out[2] - 1.0).abs() < 1e-3);
        assert!((out[4] - 2.0).abs() < 1e-3);
    }

    #[test]
    fn resample_downsample_halves() {
        let input: Vec<f32> = (0..960).map(|i| i as f32).collect();
        let mut pos = 0.0;
        let out = resample(&input, 0.0, &mut pos, 48_000, 24_000);
        assert_eq!(out.len(), 480);
        assert!((out[2] - 4.0).abs() < 1e-3);
        assert!((out[10] - 20.0).abs() < 1e-3);
    }

    #[test]
    fn resample_stitches_across_chunks() {
        // 24 kHz -> 48 kHz fed in two 10 ms chunks: the stitch carries the
        // position, so the total matches one 20 ms chunk's worth.
        let input: Vec<f32> = (0..960).map(|i| i as f32).collect();
        let mut pos = 0.0;
        let mut prev = 0.0;
        let a = resample(&input[..480], prev, &mut pos, 24_000, 48_000);
        prev = input[479];
        let b = resample(&input[480..], prev, &mut pos, 24_000, 48_000);
        assert_eq!(a.len() + b.len(), 1918);
        // Chunk b's first output sits exactly on `prev`, then between it and
        // the chunk's first sample: the carried position stitches cleanly.
        assert!((b[0] - 479.0).abs() < 1e-3);
        assert!((b[1] - 479.5).abs() < 1e-3);
    }

    #[test]
    fn pcm16_conversion_rounds_and_saturates() {
        assert_eq!(f32_to_i16(0.5), 16_384);
        assert_eq!(f32_to_i16(2.0), 32_767);
        assert_eq!(f32_to_i16(-2.0), -32_768);
        assert!((i16_to_f32(16_384) - 0.5).abs() < 1e-4);
        assert_eq!(f32_to_i16(i16_to_f32(-12_345)), -12_345);
    }

    #[test]
    fn frames_cut_complete_20ms() {
        let mut buf: std::collections::VecDeque<i16> =
            (0..(2 * FRAME_SAMPLES + 5) as i16).collect();
        let frames = frames_from(&mut buf);
        assert_eq!(frames.len(), 2);
        for frame in &frames {
            assert_eq!(frame.len(), FRAME_SAMPLES);
        }
        assert_eq!(frames[0][0], 0);
        assert_eq!(buf.len(), 5);
    }

    #[cfg(feature = "audio")]
    mod live_tests {
        use super::super::FRAME_SAMPLES;
        use super::super::live::*;

        #[test]
        fn capture_produces_nonzero_frames_without_a_device() {
            // The pure path the real callback runs: 20 ms of 440 Hz at full
            // scale becomes one nonzero frame.
            let mut state = CaptureState::new();
            let mono: Vec<f32> = (0..FRAME_SAMPLES)
                .map(|i| {
                    let t = i as f32 / 48_000.0;
                    (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                })
                .collect();
            state.push_mono(&mono, 48_000);
            let mut frames = std::collections::VecDeque::new();
            frames.extend(state.buf.iter().copied());
            let frames = super::super::frames_from(&mut frames);
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].len(), super::super::FRAME_SAMPLES);
            let peak = frames[0].iter().map(|&s| s.abs()).max().unwrap();
            assert!(
                peak > 8_000,
                "peak {peak} is silence, not a captured 440 Hz"
            );
        }

        #[test]
        fn capture_from_44100_resamples_to_48k() {
            // Three 10 ms callbacks at 44.1 kHz hold 30.03 ms of audio: at
            // least one complete frame comes out, and the resampler did not
            // stall or drop time.
            let mut state = CaptureState::new();
            for start in [0usize, 441, 882] {
                let mono: Vec<f32> = (0..441)
                    .map(|i| {
                        let t = (start + i) as f32 / 44_100.0;
                        (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                    })
                    .collect();
                state.push_mono(&mono, 44_100);
            }
            let mut buf = std::collections::VecDeque::new();
            buf.extend(state.buf.iter().copied());
            let frames = super::super::frames_from(&mut buf);
            assert_eq!(frames.len(), 1);
            let peak = frames[0].iter().map(|&s| s.abs()).max().unwrap();
            assert!(
                peak > 8_000,
                "peak {peak} is silence, not a resampled 440 Hz"
            );
        }

        #[test]
        fn playback_duplicates_channels_and_applies_volume() {
            let mut state = PlaybackState::new(48_000, 2);
            let mono = vec![0.5, -0.25, 0.125];
            state.push_voice(&mono, 0.5);
            assert_eq!(state.queue.len(), 6);
            // Stereo: both channels carry the same scaled sample.
            assert!((state.queue[0] - 0.25).abs() < 1e-6);
            assert!((state.queue[1] - 0.25).abs() < 1e-6);
            assert!((state.queue[2] + 0.125).abs() < 1e-6);
        }

        #[test]
        fn playback_resamples_to_device_rate() {
            // 48 kHz mono queued on a 24 kHz stereo device: 960 input samples
            // become about 480 per channel.
            let mut state = PlaybackState::new(24_000, 2);
            let mono = vec![1.0; super::super::FRAME_SAMPLES];
            state.push_voice(&mono, 1.0);
            let per_channel = state.queue.len() / 2;
            assert!(
                (480..=482).contains(&per_channel),
                "per channel {per_channel}"
            );
        }

        #[test]
        fn queues_are_bounded() {
            let mut capture = CaptureState::new();
            let big = vec![0.1f32; 4 * super::super::CAPTURE_MAX];
            capture.push_mono(&big, 48_000);
            assert!(capture.buf.len() <= super::super::CAPTURE_MAX);

            let mut playback = PlaybackState::new(48_000, 1);
            let big = vec![0.1f32; 384_000];
            playback.push_voice(&big, 1.0);
            assert!(playback.queue.len() <= 96_000);
        }
    }
}
