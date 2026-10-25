use shoop_loop_engine::{Event, EventKind, Frame, LoopId, Mode, TopologyDescription, Transition};

pub type Sample = (Mode, u64, u64, u64, u64);
pub struct Expected {
    pub events: Vec<Event>,
    pub samples: Vec<Vec<Sample>>,
    pub cursors: Vec<(u64, u64, Frame)>,
}

#[derive(Clone, Copy)]
struct Child {
    mode: Mode,
    length: u64,
    head: u64,
    turns: u64,
    capture: u64,
}

/// Dense interpretation of raw sections, followed by one-sample clock/media
/// simulation. No production compiler, resolver, or lookup method is used.
pub fn simulate(input: &TopologyDescription, frames: u64) -> Expected {
    assert_eq!(input.programs.len(), 1);
    assert!(input.triggers.is_empty());
    let program = &input.programs[0];
    let count = input.loops.len();
    let mut children = vec![
        Child {
            mode: Mode::Stopped,
            length: 0,
            head: 0,
            turns: 0,
            capture: 0
        };
        count
    ];
    for node in &input.loops {
        children[node.id.slot() as usize].length = node.length;
    }
    let period = children[0].length;
    let explicit = program
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .any(|entry| entry.mode.is_some());
    let mut table: Vec<Vec<Option<Mode>>> = Vec::new();
    let mut base = 0;
    for section in &program.sections {
        let mut section_end = base;
        for entry in &section.entries {
            let child_length = children[entry.target.slot() as usize].length;
            let duration = entry.duration.unwrap_or_else(|| {
                ((child_length + program.nominal_unit_frames - 1) / program.nominal_unit_frames)
                    .max(1)
            }) as usize;
            let start = base + entry.delay as usize;
            let end = start + duration * entry.repetitions as usize;
            if table.len() < end {
                table.resize(end, vec![None; count]);
            }
            for row in &mut table[start..end] {
                row[entry.target.slot() as usize] = Some(entry.mode.unwrap_or(Mode::Playing));
            }
            section_end = section_end.max(end);
        }
        base = section_end;
    }
    assert!(!table.is_empty());
    let mut result = Expected {
        events: vec![],
        samples: vec![],
        cursors: vec![],
    };
    let mut iteration = 0;
    let mut passes = 0;
    let mut entered = Frame(0);
    let mut running = true;
    for frame in 0..frames {
        let at = Frame(frame);
        let mut emit = |target: usize, kind| {
            result.events.push(Event {
                frame: at,
                target: LoopId::from_parts(target as u32, 1).unwrap(),
                kind,
            })
        };
        let tick = frame % period == 0;
        let previous = iteration;
        let mut ended = false;
        if frame == 0 {
            emit(
                0,
                EventKind::Transition(Transition::Play {
                    position: 0,
                    retrigger: false,
                }),
            );
            emit(0, EventKind::Pulse);
            emit(
                1,
                EventKind::Transition(Transition::Play {
                    position: 0,
                    retrigger: false,
                }),
            );
        } else if tick {
            emit(0, EventKind::Wrap);
            emit(0, EventKind::Pulse);
        }
        if tick {
            emit(1, EventKind::Pulse);
            if running {
                if frame != 0 {
                    iteration += 1;
                    if iteration == table.len() {
                        iteration = 0;
                        passes += 1;
                        if explicit {
                            running = false;
                            ended = true;
                        }
                    }
                }
                entered = at;
                if ended {
                    emit(1, EventKind::ProgramEnd);
                } else {
                    emit(
                        1,
                        EventKind::Iteration {
                            iteration: iteration as u64,
                            passes,
                        },
                    );
                }
            }
        }
        let mut row = vec![
            (Mode::Playing, frame % period, frame / period, 0, period),
            (
                if running {
                    Mode::Playing
                } else {
                    Mode::Stopped
                },
                0,
                0,
                0,
                table.len() as u64 * program.nominal_unit_frames,
            ),
        ];
        for (target, child) in children.iter_mut().enumerate().skip(2) {
            let current = table[iteration][target];
            let old = table[previous][target];
            let action = ended || (running && tick && (frame == 0 || old != current));
            let mut pulse = false;
            if action {
                let desired = if ended {
                    Mode::Stopped
                } else {
                    current.unwrap_or(Mode::Stopped)
                };
                match desired {
                    Mode::Stopped => {
                        if child.mode != Mode::Stopped {
                            emit(target, EventKind::Transition(Transition::Stop));
                            if child.mode == Mode::Recording {
                                child.length = child.capture;
                            }
                            child.mode = Mode::Stopped;
                            child.head = 0;
                        }
                    }
                    Mode::Recording => {
                        child.mode = Mode::Recording;
                        child.capture = 0;
                        emit(
                            target,
                            EventKind::Transition(Transition::Record {
                                monitor_previous: false,
                            }),
                        );
                        pulse = true;
                    }
                    Mode::Playing => {
                        let length = if child.mode == Mode::Recording {
                            child.capture
                        } else {
                            child.length
                        };
                        if length == 0 {
                            if child.mode != Mode::Stopped {
                                emit(target, EventKind::Transition(Transition::Stop));
                            }
                            child.mode = Mode::Stopped;
                            child.length = length;
                            child.head = 0;
                        } else {
                            child.mode = Mode::Playing;
                            child.length = length;
                            child.head = 0;
                            child.turns = 0;
                            emit(
                                target,
                                EventKind::Transition(Transition::Play {
                                    position: 0,
                                    retrigger: true,
                                }),
                            );
                            pulse = true;
                        }
                    }
                }
            }
            if child.mode == Mode::Playing && child.head == child.length {
                child.head = 0;
                child.turns += 1;
                emit(target, EventKind::Wrap);
                pulse = true;
            }
            if pulse {
                emit(target, EventKind::Pulse);
            }
            row.push((
                child.mode,
                if child.mode == Mode::Playing {
                    child.head
                } else {
                    0
                },
                child.turns,
                child.capture,
                child.length,
            ));
            if child.mode == Mode::Playing {
                child.head += 1;
            }
            if child.mode == Mode::Recording {
                child.capture += 1;
            }
        }
        result.samples.push(row);
        result.cursors.push((iteration as u64, passes, entered));
    }
    result
}
