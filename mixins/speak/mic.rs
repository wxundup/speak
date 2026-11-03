use std::cell::Cell;

use super::activation::{ActivationConfig, MicProcessor};
use super::codec::Encoder;
use super::frame::FRAME_SIZE;

thread_local! {
    /// The frame sequence counter (`MicThread.sequenceNumber`): starts at
    /// 0, increments per sent frame, stop frames included.
    static SEQUENCE: Cell<u64> = const { Cell::new(0) };
}

/// What the tick decided, for the caller to act on.
#[derive(Debug, PartialEq)]
pub enum MicStep {
    /// Nothing to send this tick (the mic isn't running, the player is
    /// disabled, or the activation said silent).
    Idle,
    /// A voice frame to send, with its sequence number.
    Send { sequence: u64, whisper: bool, data: Vec<u8> },
    /// The stop packet: one empty frame after the last sent one
    /// (`MicPacket(new byte[0], false, seq++)`), then nothing.
    Stop { sequence: u64 },
}

pub struct Mic {
    processor: MicProcessor,
    encoder: Encoder,
    /// Whether a frame went out and the stop packet hasn't yet
    /// (`hasSentAudio` / `stopPacketSent`).
    has_sent_audio: bool,
    disabled: bool,
}

impl Mic {
    pub fn new(config: ActivationConfig) -> Self {
        Self {
            processor: MicProcessor::new(config),
            encoder: Encoder::default(),
            has_sent_audio: false,
            disabled: false,
        }
    }

    /// Replaces the config (`MicThread` recreates the processor when the
    /// activation type changes; the stop packet still flows if needed).
    pub fn set_config(&mut self, config: ActivationConfig) {
        self.processor.set_config(config);
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    pub fn is_talking(&self) -> bool {
        !self.disabled && self.processor.should_transmit_audio()
    }

    /// Whether the current transmission is a whisper.
    pub fn is_whispering(&self) -> bool {
        self.processor.is_whispering()
    }

    /// The speech probability this tick's gain chain computed.
    pub fn speech_probability(&self) -> f32 {
        self.processor.speech_probability()
    }

    pub fn tick(
        &mut self,
        frame: Option<&[i16]>,
        muted: bool,
        push_to_talk_down: bool,
        whisper_down: bool,
    ) -> MicStep {
        if self.disabled {
            self.processor.reset();
            if self.has_sent_audio {
                self.has_sent_audio = false;
                let sequence = SEQUENCE.with(|s| {
                    let seq = s.get();
                    s.set(seq + 1);
                    seq
                });
                return MicStep::Stop { sequence };
            }
            return MicStep::Idle;
        }
        let mut buffer = match frame {
            Some(frame) => frame.to_vec(),
            None => vec![0i16; FRAME_SIZE],
        };
        self.decide(&mut buffer, muted, push_to_talk_down, whisper_down)
    }

    fn decide(
        &mut self,
        frame: &mut [i16],
        muted: bool,
        push_to_talk_down: bool,
        whisper_down: bool,
    ) -> MicStep {
        self.processor
            .process(frame, muted, whisper_down, push_to_talk_down, false);
        if self.processor.should_transmit_audio() {
            let Some(data) = self.encoder.encode(frame) else {
                return MicStep::Idle;
            };
            self.has_sent_audio = true;
            let sequence = SEQUENCE.with(|s| {
                let seq = s.get();
                s.set(seq + 1);
                seq
            });
            return MicStep::Send {
                sequence,
                whisper: self.processor.is_whispering(),
                data,
            };
        }
        // Silent: the stop packet goes once on the transition.
        if self.has_sent_audio {
            self.has_sent_audio = false;
            self.encoder.reset();
            let sequence = SEQUENCE.with(|s| {
                let seq = s.get();
                s.set(seq + 1);
                seq
            });
            return MicStep::Stop { sequence };
        }
        MicStep::Idle
    }
}

/// Resets the sequence counter (a new world, a fresh session).
pub fn reset_sequence() {
    SEQUENCE.with(|s| s.set(0));
}

/// The current sequence number, for the tests.

static MIC: std::sync::Mutex<Option<Mic>> = std::sync::Mutex::new(None);

pub fn with_mic<R>(f: impl FnOnce(&mut Mic) -> R) -> R {
    let mut guard = MIC.lock().expect("the mic pipeline");
    guard.get_or_insert_with(|| Mic::new(ActivationConfig::default()));
    f(guard.as_mut().unwrap())
}

#[cfg(test)]
pub fn sequence() -> u64 {
    SEQUENCE.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::Activation;

    fn config(activation: Activation) -> ActivationConfig {
        ActivationConfig {
            activation,
            ..ActivationConfig::default()
        }
    }

    fn frame() -> Vec<i16> {
        vec![3000i16; FRAME_SIZE]
    }

    fn silence() -> Vec<i16> {
        vec![0i16; FRAME_SIZE]
    }

    #[test]
    fn a_loud_frame_sends_and_the_tail_sends_the_stop() {
        reset_sequence();
        let mut mic = Mic::new(config(Activation::Voice));
        // Loud: send.
        match mic.tick(Some(&frame()), false, false, false) {
            MicStep::Send { sequence, whisper, .. } => {
                assert_eq!(sequence, 0);
                assert!(!whisper);
            }
            other => panic!("expected a send, got {other:?}"),
        }
        // Silent: the hold-time (25 frames) keeps sending.
        for seq in 1..26u64 {
            match mic.tick(Some(&silence()), false, false, false) {
                MicStep::Send { sequence, .. } => assert_eq!(sequence, seq),
                other => panic!("expected a send at {seq}, got {other:?}"),
            }
        }
        // The 26th silent frame is past the tail: the stop packet.
        match mic.tick(Some(&silence()), false, false, false) {
            MicStep::Stop { sequence } => assert_eq!(sequence, 26),
            other => panic!("expected the stop, got {other:?}"),
        }
        // And then nothing.
        assert_eq!(mic.tick(Some(&silence()), false, false, false), MicStep::Idle);
    }

    #[test]
    fn the_stop_packet_goes_once_on_the_disabled_transition() {
        reset_sequence();
        let mut mic = Mic::new(config(Activation::PushToTalk));
        match mic.tick(Some(&frame()), false, true, false) {
            MicStep::Send { sequence, .. } => assert_eq!(sequence, 0),
            other => panic!("expected a send, got {other:?}"),
        }
        mic.set_disabled(true);
        match mic.tick(None, false, false, false) {
            MicStep::Stop { sequence } => assert_eq!(sequence, 1),
            other => panic!("expected the stop, got {other:?}"),
        }
        assert_eq!(mic.tick(None, false, false, false), MicStep::Idle);
        // And nothing sends while disabled, whatever the keys do.
        mic.set_disabled(false);
        // Re-enabled: the key decides again.
        match mic.tick(Some(&frame()), false, true, true) {
            MicStep::Send { sequence, .. } => assert_eq!(sequence, 2),
            other => panic!("expected a send, got {other:?}"),
        }
    }

    #[test]
    fn the_sequence_counts_every_sent_frame_including_the_stop() {
        reset_sequence();
        let mut mic = Mic::new(config(Activation::PushToTalk));
        let mut seen = Vec::new();
        for step in [
            mic.tick(Some(&frame()), false, true, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
        ] {
            match step {
                MicStep::Send { sequence, .. } | MicStep::Stop { sequence } => {
                    seen.push(sequence)
                }
                MicStep::Idle => {}
            }
        }
        // PTT's tail is 5 silent frames after the release: a send, 5 tail
        // sends, then the stop.
        assert_eq!(seen, vec![0, 1, 2, 3, 4, 5, 6], "continuous, stop included");
    }

    #[test]
    fn the_whisper_flag_rides_the_send() {
        reset_sequence();
        let mut mic = Mic::new(config(Activation::PushToTalk));
        match mic.tick(Some(&frame()), false, true, true) {
            MicStep::Send { whisper, .. } => assert!(whisper),
            other => panic!("expected a send, got {other:?}"),
        }
        match mic.tick(Some(&silence()), false, false, false) {
            MicStep::Send { whisper, .. } => assert!(!whisper, "the key went up"),
            other => panic!("expected a tail send, got {other:?}"),
        }
    }

    #[test]
    fn a_mic_muted_stops_sending() {
        reset_sequence();
        let mut mic = Mic::new(config(Activation::PushToTalk));
        mic.tick(Some(&frame()), false, true, false);
        // Muted: silent (the activation resets), so the stop flows.
        match mic.tick(Some(&frame()), true, true, true) {
            MicStep::Stop { .. } => {}
            other => panic!("expected the stop on mute, got {other:?}"),
        }
        assert_eq!(mic.tick(Some(&frame()), true, true, true), MicStep::Idle);
    }

    #[test]
    fn a_missing_capture_still_counts_the_tail() {
        reset_sequence();
        let mut mic = Mic::new(config(Activation::PushToTalk));
        mic.tick(Some(&frame()), false, true, false);
        // No capture: the silence stand-in advances the tail. The PTT tail
        // is 5 silent frames after the release, then the stop.
        let mut tail = 0;
        for _ in 0..9 {
            match mic.tick(None, false, false, false) {
                MicStep::Send { .. } => tail += 1,
                MicStep::Stop { .. } => break,
                MicStep::Idle => {}
            }
        }
        assert_eq!(tail, 5, "the PTT tail is 5 frames after the release");
    }

    #[test]
    fn the_sequence_carries_across_the_stop_and_a_new_send() {
        reset_sequence();
        let mut mic = Mic::new(config(Activation::PushToTalk));
        let mut numbers = Vec::new();
        // A burst, its stop, and another burst: the counter never resets.
        // 1 send + 5 tail sends + the stop, then the next burst starts at 7.
        for step in [
            mic.tick(Some(&frame()), false, true, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&silence()), false, false, false),
            mic.tick(Some(&frame()), false, true, false),
        ] {
            match step {
                MicStep::Send { sequence, .. } | MicStep::Stop { sequence } => {
                    numbers.push(sequence)
                }
                MicStep::Idle => {}
            }
        }
        // 1 send + 5 tail + 1 stop (sequences 0..6), then the next burst
        // starts at 7.
        assert_eq!(numbers, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }
}
