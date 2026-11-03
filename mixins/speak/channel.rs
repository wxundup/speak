use std::collections::VecDeque;

use super::codec::Decoder;
use super::config::OUTPUT_BUFFER_SIZE;
use super::frame::FRAME_SIZE;
use super::packets::SoundPacket;

pub struct PacketBuffer {
    threshold: usize,
    buffer: Option<VecDeque<SoundPacket>>,
    last_sequence_number: i64,
    flushing: bool,
}

impl PacketBuffer {
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold: threshold as usize,
            buffer: (threshold > 0).then(VecDeque::new),
            last_sequence_number: -1,
            flushing: false,
        }
    }

    /// Whether reordering is on (`packetThreshold > 0`).
    pub fn reorders(&self) -> bool {
        self.buffer.is_some()
    }

    pub fn poll(&mut self, queue: &mut VecDeque<SoundPacket>) -> Option<SoundPacket> {
        if self.buffer.is_none() {
            return queue.pop_front();
        }
        let buffer = self.buffer.as_mut().unwrap();
        // The buffer's head first, when the rules deliver it.
        if self.flushing {
            if buffer.is_empty() {
                self.flushing = false;
            } else {
                return Self::take_first(buffer, &mut self.last_sequence_number);
            }
        } else if buffer.len() > self.threshold {
            return Self::take_first(buffer, &mut self.last_sequence_number);
        } else if let Some(head) = buffer.front() {
            if head.sequence_number == self.last_sequence_number + 1 || self.last_sequence_number < 0
            {
                return Self::take_first(buffer, &mut self.last_sequence_number);
            }
        }
        // The queue's next: in-order delivers, out-of-order buffers.
        let packet = queue.pop_front()?;
        if packet.sequence_number == self.last_sequence_number + 1 || self.last_sequence_number < 0
        {
            self.last_sequence_number = packet.sequence_number;
            Some(packet)
        } else {
            let stop = packet.data.is_empty();
            Self::insert_sorted(buffer, packet);
            if stop {
                self.flushing = true;
            }
            None
        }
    }

    fn take_first(buffer: &mut VecDeque<SoundPacket>, last: &mut i64) -> Option<SoundPacket> {
        let packet = buffer.pop_front()?;
        *last = packet.sequence_number;
        Some(packet)
    }

    fn insert_sorted(buffer: &mut VecDeque<SoundPacket>, packet: SoundPacket) {
        let at = buffer
            .iter()
            .position(|p| p.sequence_number > packet.sequence_number)
            .unwrap_or(buffer.len());
        buffer.insert(at, packet);
    }

    /// The buffer's depth (`getSize`).
    pub fn size(&self) -> usize {
        self.buffer.as_ref().map_or(0, |b| b.len())
    }

    /// Clears everything (`clear`): the buffer, the last sequence, the
    /// flushing flag.
    pub fn clear(&mut self) {
        if let Some(b) = self.buffer.as_mut() {
            b.clear();
        }
        self.last_sequence_number = -1;
        self.flushing = false;
    }
}

/// What the channel decided for one decoded frame (`writeToSpeaker`'s
/// branches). The volume stack is the speaker layer's; the frame carries
/// the decoded samples.
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    /// A group (static) frame: play non-positionally (`GroupSoundPacket`).
    Static { samples: Vec<f32> },
    /// A player or locational frame: play positionally.
    Positional { samples: Vec<f32> },
    /// The stop marker: the sender stopped (`new short[0]`): reset the
    /// sequence, clear the reorder buffer.
    Stop,
    /// Nothing this step.
    Idle,
}

pub struct Channel {
    player: u128,
    queue: VecDeque<SoundPacket>,
    buffer: PacketBuffer,
    decoder: Decoder,
    /// `lastSequenceNumber`: -1 until the first packet.
    last_sequence: i64,
    /// The last gap's size (`lostPackets`), for the debug overlay.
    lost: i64,
    /// How many ticks since the last packet, for the idle kill.
    idle_ticks: u64,
    /// The pre-buffer: silent frames queued before the first sound plays
    /// (`outputBufferSize`, default 5 = 100 ms). Counts down while
    /// filling.
    prebuffer_left: u32,
    /// The silence frames left to play from a compensation gap
    /// (`decoder.decode(data, n)`'s first n−1 PLC frames).
    compensation: u32,
    /// The gap's real frame, decoded at once and played after the
    /// silences.
    pending_real: Option<Vec<i16>>,
}

pub const IDLE_KILL_TICKS: u64 = 600;

impl Channel {
    pub fn new(player: u128, reorder_threshold: u32, prebuffer: u32) -> Self {
        Self {
            player,
            queue: VecDeque::new(),
            buffer: PacketBuffer::new(reorder_threshold),
            decoder: Decoder::new(),
            last_sequence: -1,
            lost: 0,
            idle_ticks: 0,
            prebuffer_left: prebuffer,
            compensation: 0,
            pending_real: None,
        }
    }

    pub fn player(&self) -> u128 {
        self.player
    }

    /// A packet arrived (`processSoundPacket` → the queue).
    pub fn enqueue(&mut self, packet: SoundPacket) {
        self.queue.push_back(packet);
    }

    /// The queue's depth (the debug overlay's `Queue` column).
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// The reorder buffer's depth (the debug overlay's `Reordering`).
    pub fn buffer_len(&self) -> usize {
        self.buffer.size()
    }

    /// How many packets were lost in the last gap (`lostPackets`), for the
    /// debug overlay.
    pub fn lost_last_gap(&self) -> i64 {
        self.lost
    }

    pub fn tick(&mut self) -> Frame {
        self.idle_ticks += 1;
        // A pending gap: the PLC silences first, then the real frame.
        if self.compensation > 0 {
            self.compensation -= 1;
            return self.prebuffered(Frame::Positional {
                samples: vec![0.0; FRAME_SIZE],
            });
        }
        if let Some(real) = self.pending_real.take() {
            return self.prebuffered(Frame::Positional {
                samples: super::frame::shorts_to_floats_normalized(&real),
            });
        }
        let Some(packet) = self.buffer.poll(&mut self.queue) else {
            return Frame::Idle;
        };
        self.idle_ticks = 0;
        // Old-sequence drop (`packet.getSequenceNumber() <=
        // lastSequenceNumber`), except for client-channel packets, whose
        // sequence is negative.
        if packet.sequence_number >= 0 && packet.sequence_number <= self.last_sequence {
            return Frame::Idle;
        }
        // An empty payload is the stop marker: reset the sequence and the
        // reorder buffer, not the queue.
        if packet.data.is_empty() {
            self.last_sequence = -1;
            self.buffer.clear();
            self.lost = 0;
            return Frame::Stop;
        }
        // The gap policy: `packetsToCompensate = seq - (last + 1)`; past
        // `output_buffer_size` the decoder resets and no compensation runs;
        // otherwise `packetsToCompensate + 1` frames decode — the first n−1
        // being the PLC stand-in (silence) and the last the real one.
        let gap = if self.last_sequence < 0 {
            0
        } else {
            packet.sequence_number - self.last_sequence - 1
        };
        self.last_sequence = packet.sequence_number;
        let Some(samples) = self.decoder.decode(&packet.data) else {
            return Frame::Idle;
        };
        if gap > i64::from(OUTPUT_BUFFER_SIZE) {
            self.decoder.reset();
            self.lost = gap;
        } else if gap > 0 {
            self.lost = gap;
            // The silences go out first, the real frame after them — all
            // n frames of the one decode.
            self.compensation = (gap - 1) as u32;
            self.pending_real = Some(samples);
            return self.prebuffered(Frame::Positional {
                samples: vec![0.0; FRAME_SIZE],
            });
        }
        let frame = match packet.sound_kind {
            super::packets::SOUND_GROUP => Frame::Static {
                samples: super::frame::shorts_to_floats_normalized(&samples),
            },
            _ => Frame::Positional {
                samples: super::frame::shorts_to_floats_normalized(&samples),
            },
        };
        self.prebuffered(frame)
    }

    /// The pre-buffer: the first `output_buffer_size` frames of a stream
    /// play as silence so the queue has depth before the sound starts
    /// (`ALSpeakerBase.play`'s pre-fill; here at the channel level).
    fn prebuffered(&mut self, frame: Frame) -> Frame {
        if self.prebuffer_left > 0 {
            self.prebuffer_left -= 1;
            return match frame {
                Frame::Static { .. } => Frame::Static {
                    samples: vec![0.0; FRAME_SIZE],
                },
                Frame::Positional { .. } => Frame::Positional {
                    samples: vec![0.0; FRAME_SIZE],
                },
                other => other,
            };
        }
        frame
    }

    /// Whether the channel is idle past the kill time (`canKill`).
    pub fn can_kill(&self) -> bool {
        self.idle_ticks > IDLE_KILL_TICKS
    }

    /// Closes the channel (`closeAndKill`): the queue empties.
    pub fn close(&mut self) {
        self.queue.clear();
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::packets;

    fn packet(seq: i64, data: &[u8]) -> SoundPacket {
        SoundPacket {
            sound_kind: packets::SOUND_PLAYER,
            sender: 1,
            channel_id: 1,
            sequence_number: seq,
            whisper: false,
            distance: 48.0,
            position: [0.0; 3],
            dimension: "o".into(),
            name: "Alex".into(),
            data: data.to_vec(),
        }
    }

    fn voice(seq: i64) -> SoundPacket {
        packet(seq, &[1; 960])
    }

    #[test]
    fn reordering_off_polls_the_queue_straight() {
        let mut c = Channel::new(1, 0, 0);
        c.enqueue(voice(0));
        c.enqueue(voice(1));
        assert!(matches!(c.tick(), Frame::Positional { .. }));
        assert!(matches!(c.tick(), Frame::Positional { .. }));
        assert!(matches!(c.tick(), Frame::Idle));
    }

    #[test]
    fn the_first_packet_delivers_at_once_and_the_next_in_order() {
        let mut c = Channel::new(1, 3, 0);
        c.enqueue(voice(5));
        assert!(matches!(c.tick(), Frame::Positional { .. }), "first ever");
        c.enqueue(voice(6));
        assert!(matches!(c.tick(), Frame::Positional { .. }));
    }

    #[test]
    fn an_in_order_packet_delivers_even_past_an_out_of_order_one() {
        let mut c = Channel::new(1, 3, 0);
        c.enqueue(voice(0));
        assert!(matches!(c.tick(), Frame::Positional { .. }));
        // 2 arrives out of order and buffers; the queue's next is 1, which
        // IS in order, so it delivers on the following tick (the poll
        // checks the queue packet before buffering it).
        c.enqueue(voice(2));
        c.enqueue(voice(1));
        assert!(matches!(c.tick(), Frame::Idle));
        assert_eq!(c.buffer_len(), 1, "only 2 is buffered");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "1");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "2");
        c.enqueue(voice(3));
        c.enqueue(voice(4));
        assert!(matches!(c.tick(), Frame::Positional { .. }), "3");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "4");
        assert!(matches!(c.tick(), Frame::Idle));
    }

    #[test]
    fn debug_flush_trace() {
        let mut c = Channel::new(1, 3, 0);
        c.enqueue(voice(0));
        println!("t1: {:?}", c.tick());
        for seq in [2i64, 3, 4, 5] {
            c.enqueue(voice(seq));
        }
        for i in 0..8 {
            let frame = c.tick();
            println!(
                "t{}: {:?} buffer={} queue={}",
                i + 2,
                frame,
                c.buffer_len(),
                c.queue_len()
            );
        }
    }

    #[test]
    fn the_buffer_flushes_past_its_threshold() {
        let mut c = Channel::new(1, 3, 0);
        c.enqueue(voice(0));
        c.tick();
        // Four out-of-order packets arrive: each buffers (nothing else is
        // in order), and on the tick after the fourth the buffer holds
        // 4 > threshold 3, so the head (2) flushes.
        for seq in [2i64, 3, 4, 5] {
            c.enqueue(voice(seq));
        }
        assert!(matches!(c.tick(), Frame::Idle), "2 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "3 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "4 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "5 buffered");
        assert!(
            matches!(c.tick(), Frame::Positional { ref samples, .. } if samples.iter().all(|&s| s == 0.0)),
            "the gap PLC frame"
        );
        assert!(matches!(c.tick(), Frame::Positional { .. }), "2, the real frame");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "3");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "4");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "5");
        assert!(matches!(c.tick(), Frame::Idle), "drained");
    }

    #[test]
    fn a_stop_packet_in_the_buffer_flushes_everything_in_order() {
        let mut c = Channel::new(1, 3, 0);
        c.enqueue(voice(0));
        assert!(matches!(c.tick(), Frame::Positional { .. }));
        c.enqueue(voice(3));
        c.enqueue(packet(2, &[]));
        c.enqueue(voice(1));
        // The flush drains the buffer's head first: the stop packet (2)
        // itself, which resets the sequence; then voice(1) delivers from
        // the queue as a first-ever; voice(3) is out of order again and
        // buffers (the stop clears the buffer, not the queue).
        assert!(matches!(c.tick(), Frame::Idle), "3 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "2 buffered, flushing set");
        assert!(matches!(c.tick(), Frame::Stop), "the buffered stop");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "1 after the reset");
        assert!(matches!(c.tick(), Frame::Idle), "3 is out of order again");
    }

    #[test]
    fn an_in_order_stop_resets_everything() {
        let mut c = Channel::new(1, 3, 0);
        c.enqueue(voice(0));
        c.tick();
        c.enqueue(packet(1, &[]));
        assert!(matches!(c.tick(), Frame::Stop));
        // The last sequence reset: the next first-ever packet delivers.
        c.enqueue(voice(100));
        assert!(matches!(c.tick(), Frame::Positional { .. }));
    }

    #[test]
    fn an_old_packet_is_dropped() {
        let mut c = Channel::new(1, 3, 0);
        c.enqueue(voice(0));
        c.tick();
        c.enqueue(voice(1));
        c.tick();
        c.enqueue(voice(0));
        assert!(matches!(c.tick(), Frame::Idle), "the old one is skipped");
    }

    #[test]
    fn a_small_gap_compensates_with_silence_then_the_real_frame() {
        let mut c = Channel::new(1, 3, 0);
        c.enqueue(voice(0));
        c.tick();
        // A gap reaches the channel only through a flush: 3 is out of
        // order, then 4, 5, 6 buffer behind it (the last pushes the
        // buffer past its threshold, flushing 3 out).
        for seq in [3i64, 4, 5, 6] {
            c.enqueue(voice(seq));
        }
        assert!(matches!(c.tick(), Frame::Idle), "3 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "4 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "5 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "6 buffered");
        // The flush delivers 3 with a gap of 2 (packets 1 and 2 lost):
        // two silence frames, then the real one.
        let first = c.tick();
        assert!(
            matches!(first, Frame::Positional { ref samples, .. } if samples.iter().all(|&s| s == 0.0)),
            "the first gap frame is silence"
        );
        let second = c.tick();
        assert!(
            matches!(second, Frame::Positional { ref samples, .. } if samples.iter().all(|&s| s == 0.0))
        );
        let third = c.tick();
        assert!(
            matches!(third, Frame::Positional { ref samples, .. } if samples.iter().any(|&s| s != 0.0)),
            "the third frame is the real one"
        );
        assert_eq!(c.lost_last_gap(), 2);
        // The rest follow in order from the buffer.
        assert!(matches!(c.tick(), Frame::Positional { .. }), "4");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "5");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "6");
    }

    #[test]
    fn a_huge_gap_resets_instead_of_compensating() {
        let mut c = Channel::new(1, 3, 0);
        c.enqueue(voice(0));
        c.tick();
        // A gap of 99 rides the same flush: 100 buffers, then 101, 102,
        // 103 push it out.
        for seq in [100i64, 101, 102, 103] {
            c.enqueue(voice(seq));
        }
        assert!(matches!(c.tick(), Frame::Idle), "100 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "101 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "102 buffered");
        assert!(matches!(c.tick(), Frame::Idle), "103 buffered");
        let frame = c.tick();
        assert!(
            matches!(frame, Frame::Positional { ref samples, .. } if samples.iter().any(|&s| s != 0.0)),
            "no compensation: the real frame at once"
        );
        assert_eq!(c.lost_last_gap(), 99);
        // The rest follow in order (only the decoder is cleared here).
        assert!(matches!(c.tick(), Frame::Positional { .. }), "101");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "102");
        assert!(matches!(c.tick(), Frame::Positional { .. }), "103");
    }


    #[test]
    fn the_prebuffer_silences_the_first_frames() {
        let mut c = Channel::new(1, 0, 5);
        for seq in 0..8i64 {
            c.enqueue(voice(seq));
        }
        let mut silences = 0;
        let mut reals = 0;
        for _ in 0..8 {
            match c.tick() {
                Frame::Positional { samples, .. } => {
                    if samples.iter().all(|&s| s == 0.0) {
                        silences += 1;
                    } else {
                        reals += 1;
                    }
                }
                _ => {}
            }
        }
        assert_eq!(silences, 5, "output_buffer_size silent frames first");
        assert_eq!(reals, 3);
    }

    #[test]
    fn an_idle_channel_kills_after_thirty_seconds() {
        let mut c = Channel::new(1, 3, 0);
        for _ in 0..600 {
            c.tick();
        }
        assert!(!c.can_kill());
        c.tick();
        assert!(c.can_kill());
        c.close();
        assert_eq!(c.queue_len(), 0);
    }

    #[test]
    fn group_frames_are_static() {
        let mut c = Channel::new(1, 0, 0);
        let mut p = voice(0);
        p.sound_kind = packets::SOUND_GROUP;
        c.enqueue(p);
        assert!(matches!(c.tick(), Frame::Static { .. }));
    }
}
