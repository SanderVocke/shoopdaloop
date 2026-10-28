//! Fixed semantic observation storage; ordinary callback cuts are omitted.
use shoop_loop_engine::{
    runtime::BoundaryObserver, Decision, Event, EventKind, Frame, Mode, Transition,
};

pub const HEADER: usize = 40;
pub const STRIDE: usize = 160;
pub const RECORDS: usize = 128;
pub const BYTES: usize = HEADER + STRIDE * RECORDS;
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

pub struct Journal {
    bytes: Box<[u8]>,
    count: usize,
    dropped_boundaries: u64,
    dropped_records: u64,
    resolver_high_water: u64,
    resolver_previous: u64,
}
impl Journal {
    pub fn new(records: usize) -> Self {
        assert!(u32::try_from(records).is_ok());
        let size = records
            .checked_mul(STRIDE)
            .and_then(|size| size.checked_add(HEADER))
            .expect("semantic storage size");
        let mut bytes = vec![0; size].into_boxed_slice();
        std::hint::black_box(&mut *bytes).fill(0);
        bytes[..4].copy_from_slice(b"LSJ1");
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&(STRIDE as u16).to_le_bytes());
        Self {
            bytes,
            count: 0,
            dropped_boundaries: 0,
            dropped_records: 0,
            resolver_high_water: 0,
            resolver_previous: 0,
        }
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..HEADER + self.count * STRIDE]
    }
    fn append(&mut self, record: &[u8; STRIDE]) {
        let at = HEADER + self.count * STRIDE;
        self.bytes[at..at + STRIDE].copy_from_slice(record);
        self.count += 1;
    }
    fn header(&mut self) {
        put32(&mut self.bytes, 8, self.count as u32);
        put64(&mut self.bytes, 16, self.dropped_boundaries);
        put64(&mut self.bytes, 24, self.dropped_records);
        put64(&mut self.bytes, 32, self.resolver_high_water);
    }
}
impl BoundaryObserver for Journal {
    fn boundary(&mut self, frame: Frame, decisions: &[Decision], events: &[Event], dropped: u64) {
        let changed_loss = dropped != self.resolver_previous;
        self.resolver_previous = dropped;
        self.resolver_high_water = self.resolver_high_water.max(dropped);
        if events.is_empty()
            && !changed_loss
            && !decisions
                .iter()
                .any(|decision| decision.playback_discontinuity)
        {
            self.header();
            return;
        }
        let needed = 1usize
            .saturating_add(decisions.len())
            .saturating_add(events.len());
        let remaining = (self.bytes.len() - HEADER) / STRIDE - self.count;
        if needed > remaining {
            self.dropped_boundaries = self.dropped_boundaries.saturating_add(1);
            self.dropped_records = self
                .dropped_records
                .saturating_add(u64::try_from(needed).unwrap_or(u64::MAX));
            self.header();
            return;
        }
        let mut boundary = [0; STRIDE];
        boundary[0] = 1;
        put64(&mut boundary, 8, frame.0);
        put32(&mut boundary, 16, decisions.len() as u32);
        put32(&mut boundary, 20, events.len() as u32);
        put64(&mut boundary, 24, dropped);
        self.append(&boundary);
        for decision in decisions {
            self.append(&encode_decision(frame, decision));
        }
        for event in events {
            self.append(&encode_event(event));
        }
        self.header();
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
fn encode_decision(frame: Frame, decision: &Decision) -> [u8; STRIDE] {
    let state = decision.state;
    let mut bytes = [0; STRIDE];
    bytes[0] = 2;
    bytes[1] = match state.mode {
        Mode::Stopped => 0,
        Mode::Playing => 1,
        Mode::Recording => 2,
    };
    bytes[2] = u8::from(decision.playback_discontinuity)
        | (u8::from(state.capture_incomplete) << 1)
        | (u8::from(state.monitor_previous) << 2)
        | (u8::from(state.last_pulse.is_some()) << 3)
        | (u8::from(state.composite.is_some()) << 4)
        | (u8::from(
            state
                .composite
                .is_some_and(|value| value.entered_at.is_some()),
        ) << 5)
        | (u8::from(decision.origin.is_some()) << 6);
    put64(&mut bytes, 8, frame.0);
    put32(&mut bytes, 16, state.id.slot());
    put32(&mut bytes, 20, state.id.generation());
    for (at, value) in [
        (24, state.content_length),
        (32, state.content_revision.0),
        (40, state.position),
        (48, state.cycles),
        (56, state.capture_frames),
        (64, state.capture_revision.0),
        (72, state.last_pulse.map_or(0, |value| value.0)),
    ] {
        put64(&mut bytes, at, value);
    }
    if let Some(composite) = state.composite {
        put64(&mut bytes, 80, composite.iteration);
        put64(&mut bytes, 88, composite.passes);
        put64(
            &mut bytes,
            96,
            composite.entered_at.map_or(0, |value| value.0),
        );
    }
    bytes
}
fn encode_event(event: &Event) -> [u8; STRIDE] {
    let (code, flag, a, b) = match event.kind {
        EventKind::Transition(transition) => match transition {
            Transition::Record { monitor_previous } => (1, monitor_previous, 0, 0),
            Transition::Play {
                position,
                retrigger,
            } => (2, retrigger, position, 0),
            Transition::Stop => (3, false, 0, 0),
            Transition::Seek {
                position,
                retrigger,
            } => (4, retrigger, position, 0),
            Transition::Replace {
                length,
                revision,
                preserve_position,
            } => (5, preserve_position, length, revision.0),
            Transition::PlayIteration {
                iteration,
                retrigger,
            } => (6, retrigger, iteration, 0),
            Transition::SeekIteration {
                iteration,
                retrigger,
            } => (7, retrigger, iteration, 0),
            Transition::Program => (8, false, 0, 0),
        },
        EventKind::Wrap => (9, false, 0, 0),
        EventKind::Pulse => (10, false, 0, 0),
        EventKind::CaptureExhausted => (11, false, 0, 0),
        EventKind::Iteration { iteration, passes } => (12, false, iteration, passes),
        EventKind::ProgramEnd => (13, false, 0, 0),
        EventKind::Refused(error) => (14, false, u64::from(crate::wire::loop_error(error)), 0),
    };
    let mut bytes = [0; STRIDE];
    bytes[0] = 3;
    bytes[1] = code;
    bytes[2] = u8::from(flag);
    put64(&mut bytes, 8, event.frame.0);
    put32(&mut bytes, 16, event.target.slot());
    put32(&mut bytes, 20, event.target.generation());
    put64(&mut bytes, 24, a);
    put64(&mut bytes, 32, b);
    bytes
}

/// Independent table for the fixture's musical boundaries, not callback cuts.
pub fn expected_primitive() -> Vec<u8> {
    let mut bytes = vec![0; HEADER];
    bytes[..8].copy_from_slice(&[b'L', b'S', b'J', b'1', 1, 0, 160, 0]);
    bytes[8..12].copy_from_slice(&15u32.to_le_bytes());
    for (frame, mode, flags, length, revision, cycles, capture, pulse, event_code) in [
        (0u64, 2, 72, 0u64, 0u64, 0u64, 0u64, 0u64, 1),
        (4, 1, 73, 4, 1, 0, 4, 4, 2),
        (8, 1, 9, 4, 1, 1, 4, 8, 9),
        (12, 0, 72, 4, 1, 1, 4, 8, 3),
    ] {
        let mut boundary = [0; 160];
        boundary[0] = 1;
        boundary[8..16].copy_from_slice(&frame.to_le_bytes());
        boundary[16..20].copy_from_slice(&1u32.to_le_bytes());
        boundary[20..24].copy_from_slice(&(if frame == 12 { 1u32 } else { 2 }).to_le_bytes());
        bytes.extend(boundary);
        let mut decision = [0; 160];
        decision[0] = 2;
        decision[1] = mode;
        decision[2] = flags;
        decision[8..16].copy_from_slice(&frame.to_le_bytes());
        decision[20..24].copy_from_slice(&1u32.to_le_bytes());
        for (at, value) in [
            (24, length),
            (32, revision),
            (48, cycles),
            (56, capture),
            (64, 1u64),
            (72, pulse),
        ] {
            decision[at..at + 8].copy_from_slice(&value.to_le_bytes());
        }
        bytes.extend(decision);
        let codes = [event_code, 10];
        for code in &codes[..if frame == 12 { 1 } else { 2 }] {
            let mut event = [0; 160];
            event[0] = 3;
            event[1] = *code;
            event[8..16].copy_from_slice(&frame.to_le_bytes());
            event[20..24].copy_from_slice(&1u32.to_le_bytes());
            bytes.extend(event);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_event_variants_preserve_full_width_fields_and_canonical_padding() {
        use shoop_loop_engine::{LoopId, Revision};
        let variants = [
            (
                EventKind::Transition(Transition::Record {
                    monitor_previous: true,
                }),
                1,
                1,
                0,
                0,
            ),
            (
                EventKind::Transition(Transition::Play {
                    position: u64::MAX,
                    retrigger: true,
                }),
                2,
                1,
                u64::MAX,
                0,
            ),
            (EventKind::Transition(Transition::Stop), 3, 0, 0, 0),
            (
                EventKind::Transition(Transition::Seek {
                    position: u64::MAX,
                    retrigger: false,
                }),
                4,
                0,
                u64::MAX,
                0,
            ),
            (
                EventKind::Transition(Transition::Replace {
                    length: u64::MAX,
                    revision: Revision(u64::MAX - 1),
                    preserve_position: true,
                }),
                5,
                1,
                u64::MAX,
                u64::MAX - 1,
            ),
            (
                EventKind::Transition(Transition::PlayIteration {
                    iteration: u64::MAX,
                    retrigger: true,
                }),
                6,
                1,
                u64::MAX,
                0,
            ),
            (
                EventKind::Transition(Transition::SeekIteration {
                    iteration: u64::MAX,
                    retrigger: false,
                }),
                7,
                0,
                u64::MAX,
                0,
            ),
            (EventKind::Transition(Transition::Program), 8, 0, 0, 0),
            (EventKind::Wrap, 9, 0, 0, 0),
            (EventKind::Pulse, 10, 0, 0, 0),
            (EventKind::CaptureExhausted, 11, 0, 0, 0),
            (
                EventKind::Iteration {
                    iteration: u64::MAX,
                    passes: u64::MAX - 1,
                },
                12,
                0,
                u64::MAX,
                u64::MAX - 1,
            ),
            (EventKind::ProgramEnd, 13, 0, 0, 0),
            (
                EventKind::Refused(shoop_loop_engine::Error::EmptyContent),
                14,
                0,
                268,
                0,
            ),
        ];
        for (kind, code, flag, a, b) in variants {
            let event = Event {
                frame: Frame(u64::MAX),
                target: LoopId::from_parts(u32::MAX - 1, u32::MAX).unwrap(),
                kind,
            };
            let mut journal = Journal::new(2);
            crate::guarded(|| journal.boundary(event.frame, &[], &[event], 0));
            let mut expected = [0; 160];
            expected[0] = 3;
            expected[1] = code;
            expected[2] = flag;
            expected[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
            expected[16..20].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
            expected[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
            expected[24..32].copy_from_slice(&a.to_le_bytes());
            expected[32..40].copy_from_slice(&b.to_le_bytes());
            assert_eq!(&journal.bytes()[HEADER + STRIDE..], &expected);
        }
    }
    #[test]
    fn decision_projection_preserves_large_values_and_absent_optional_fields() {
        use shoop_loop_engine::{
            compile, CompositeState, Limits, LoopId, LoopKind, LoopSpec, RequestId, Resolver,
            Revision, TopologyDescription,
        };
        let id = LoopId::from_parts(0, u32::MAX).unwrap();
        let resolver = Resolver::prepare(
            compile(
                Limits {
                    loop_slots: 1,
                    ..Limits::default()
                },
                &TopologyDescription {
                    loops: vec![LoopSpec {
                        id,
                        kind: LoopKind::Primitive,
                        length: 0,
                        sync_source: None,
                    }],
                    ..TopologyDescription::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        let mut state = resolver.state(id).unwrap();
        state.mode = Mode::Recording;
        state.content_length = u64::MAX;
        state.content_revision = Revision(u64::MAX - 1);
        state.position = u64::MAX - 2;
        state.cycles = u64::MAX - 3;
        state.capture_frames = u64::MAX - 4;
        state.capture_revision = Revision(u64::MAX - 5);
        state.capture_incomplete = true;
        state.monitor_previous = true;
        state.last_pulse = Some(Frame(u64::MAX - 6));
        state.composite = Some(CompositeState {
            iteration: u64::MAX - 7,
            passes: u64::MAX - 8,
            entered_at: Some(Frame(u64::MAX - 9)),
        });
        let mut decision = Decision {
            state,
            playback_discontinuity: true,
            origin: Some(RequestId::from_parts(0, 1).unwrap()),
        };
        let mut journal = Journal::new(4);
        crate::guarded(|| journal.boundary(Frame(u64::MAX - 1), &[decision], &[], 0));
        let mut expected = [0; STRIDE];
        expected[0] = 2;
        expected[1] = 2;
        expected[2] = 127;
        expected[8..16].copy_from_slice(&(u64::MAX - 1).to_le_bytes());
        expected[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
        for (index, at) in (24..=96).step_by(8).enumerate() {
            expected[at..at + 8].copy_from_slice(&(u64::MAX - index as u64).to_le_bytes());
        }
        assert_eq!(&journal.bytes()[HEADER + STRIDE..], &expected);
        decision.state.last_pulse = None;
        decision.state.composite = None;
        decision.state.capture_incomplete = false;
        decision.state.monitor_previous = false;
        decision.origin = None;
        crate::guarded(|| journal.boundary(Frame(u64::MAX), &[decision], &[], 0));
        let encoded = &journal.bytes()[HEADER + 3 * STRIDE..];
        assert_eq!(encoded[2], 1);
        assert_eq!(&encoded[72..104], &[0; 32]);
        assert_eq!(&encoded[8..16], &u64::MAX.to_le_bytes());
    }

    #[test]
    fn loss_is_sticky_saturating_and_empty_callback_cuts_do_not_consume_records() {
        let mut journal = Journal::new(0);
        journal.dropped_boundaries = u64::MAX;
        journal.dropped_records = u64::MAX;
        crate::guarded(|| {
            journal.boundary(Frame(0), &[], &[], 0);
            journal.boundary(Frame(1), &[], &[], u64::MAX);
            journal.boundary(Frame(2), &[], &[], 0);
        });
        assert_eq!(journal.count, 0);
        assert_eq!(journal.dropped_boundaries, u64::MAX);
        assert_eq!(journal.dropped_records, u64::MAX);
        assert_eq!(journal.resolver_high_water, u64::MAX);
    }
}
