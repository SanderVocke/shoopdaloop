use shoop_loop_engine::{
    Error, Event, EventKind, Frame, LoopId, Mode, Outcome, Request, Revision, Timing, Transition,
};

#[derive(Clone, Copy, Eq, PartialEq)]
struct Machine {
    mode: Mode,
    length: u64,
    head: u64,
    turns: u64,
    captured: u64,
    revision: u64,
    take: u64,
    watermark: u64,
    monitor: bool,
}

pub struct Expected {
    pub events: Vec<Event>,
    pub samples: Vec<(Mode, u64, u64)>,
    pub outcomes: Vec<Option<Outcome>>,
    pub final_state: (Mode, u64, u64, u64, u64, Revision, Revision),
}

/// An allocating single-sample machine. It has no production resolver,
/// component compiler, decision buffer, or interval-selection calls.
pub fn primitive(length: u64, requests: &[Request], frames: u64) -> Expected {
    assert!(requests.iter().all(|request| request.timing == Timing::At));
    let target = LoopId::from_parts(0, 1).unwrap();
    let mut machine = Machine {
        mode: Mode::Stopped,
        length,
        head: 0,
        turns: 0,
        captured: 0,
        revision: 0,
        take: 0,
        watermark: 0,
        monitor: false,
    };
    let mut result = Expected {
        events: vec![],
        samples: vec![],
        outcomes: vec![None; requests.len()],
        final_state: (Mode::Stopped, 0, 0, 0, 0, Revision(0), Revision(0)),
    };
    for frame in 0..frames {
        let at = Frame(frame);
        let eligible: Vec<_> = requests
            .iter()
            .enumerate()
            .filter(|(index, request)| request.at == at && result.outcomes[*index].is_none())
            .collect();
        let winner = eligible
            .iter()
            .max_by_key(|(_, request)| request.sequence)
            .map(|(index, _)| *index);
        for (index, _) in eligible {
            if Some(index) != winner {
                result.outcomes[index] = Some(Outcome::Superseded { frame: at });
            }
        }
        let mut pulse = false;
        if let Some(index) = winner {
            let operation = requests[index].transition;
            let mut candidate = machine;
            let mut failure = None;
            match operation {
                Transition::PlayIteration { .. } | Transition::SeekIteration { .. } => {
                    failure = Some(Error::InvalidState)
                }
                Transition::Stop => {
                    if candidate.mode == Mode::Recording {
                        candidate.length = candidate.captured;
                        candidate.revision = candidate.take;
                    }
                    candidate.mode = Mode::Stopped;
                    candidate.head = 0;
                    candidate.monitor = false;
                }
                Transition::Record { monitor_previous } => {
                    if let Some(revision) = candidate.watermark.checked_add(1) {
                        candidate.watermark = revision;
                        candidate.take = revision;
                        candidate.captured = 0;
                        candidate.monitor = monitor_previous;
                        candidate.mode = Mode::Recording;
                        pulse = true;
                    } else {
                        failure = Some(Error::Exhausted);
                    }
                }
                Transition::Play {
                    position,
                    retrigger,
                } => {
                    if candidate.mode != Mode::Playing || retrigger {
                        let length = if candidate.mode == Mode::Recording {
                            candidate.captured
                        } else {
                            candidate.length
                        };
                        if length == 0 {
                            failure = Some(Error::EmptyContent);
                        } else if position >= length {
                            failure = Some(Error::InvalidFrame);
                        } else {
                            if candidate.mode == Mode::Recording {
                                candidate.revision = candidate.take;
                            }
                            candidate.length = length;
                            candidate.mode = Mode::Playing;
                            candidate.head = position;
                            candidate.turns = 0;
                            candidate.monitor = false;
                            pulse = true;
                        }
                    }
                }
                Transition::Seek {
                    position,
                    retrigger,
                } => {
                    if candidate.mode != Mode::Playing {
                        failure = Some(Error::InvalidState);
                    } else if position >= candidate.length {
                        failure = Some(Error::InvalidFrame);
                    } else {
                        candidate.head = position;
                        if retrigger {
                            candidate.turns = 0;
                            pulse = true;
                        }
                    }
                }
                Transition::Replace {
                    length,
                    revision,
                    preserve_position,
                } => {
                    if candidate.mode == Mode::Recording {
                        failure = Some(Error::InvalidState);
                    } else if preserve_position && candidate.head != 0 && candidate.head >= length {
                        failure = Some(Error::InvalidFrame);
                    } else {
                        candidate.length = length;
                        candidate.revision = revision.0;
                        candidate.watermark = candidate.watermark.max(revision.0);
                        if !preserve_position {
                            candidate.head = 0;
                        }
                        if length == 0 {
                            candidate.mode = Mode::Stopped;
                        }
                    }
                }
            }
            if let Some(reason) = failure {
                result.outcomes[index] = Some(Outcome::Failed { frame: at, reason });
                pulse = false;
            } else {
                result.outcomes[index] = Some(Outcome::Applied { frame: at });
                if candidate != machine {
                    result.events.push(Event {
                        frame: at,
                        target,
                        kind: EventKind::Transition(operation),
                    });
                }
                machine = candidate;
                if operation == Transition::Stop {
                    for (other, request) in requests.iter().enumerate() {
                        if result.outcomes[other].is_none()
                            && request.sequence < requests[index].sequence
                        {
                            result.outcomes[other] = Some(Outcome::Cancelled { frame: at });
                        }
                    }
                }
            }
        }
        let plays = machine.length > 0
            && (machine.mode == Mode::Playing
                || (machine.mode == Mode::Recording && machine.monitor));
        if plays && machine.head == machine.length {
            machine.head = 0;
            machine.turns += 1;
            result.events.push(Event {
                frame: at,
                target,
                kind: EventKind::Wrap,
            });
            pulse = true;
        }
        if pulse {
            result.events.push(Event {
                frame: at,
                target,
                kind: EventKind::Pulse,
            });
        }
        result.samples.push((
            machine.mode,
            if plays { machine.head } else { 0 },
            machine.captured,
        ));
        if plays {
            machine.head += 1;
        }
        if machine.mode == Mode::Recording {
            machine.captured += 1;
        }
    }
    result.final_state = (
        machine.mode,
        machine.length,
        machine.head,
        machine.turns,
        machine.captured,
        Revision(machine.revision),
        Revision(machine.take),
    );
    result
}
