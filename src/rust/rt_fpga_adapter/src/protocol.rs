//! Packed transition-port values. No musical state is computed here.
use rt_runtime::{Error, Frame, Revision, Sequence};
use shoop_loops::{self as loops, Event, EventKind, LoopId, Mode, RequestId, State, Transition};
pub fn put<const N: usize>(words: &mut [u32; N], offset: usize, width: usize, value: u64) {
    assert!(width <= 64 && offset + width <= N * 32);
    for i in 0..width {
        let bit = offset + i;
        words[bit / 32] =
            (words[bit / 32] & !(1 << (bit % 32))) | ((((value >> i) & 1) as u32) << (bit % 32));
    }
}
pub fn get<const N: usize>(words: &[u32; N], offset: usize, width: usize) -> u64 {
    assert!(width <= 64 && offset + width <= N * 32);
    (0..width).fold(0, |v, i| {
        v | (u64::from((words[(offset + i) / 32] >> ((offset + i) % 32)) & 1) << i)
    })
}
pub fn error(code: u64) -> Result<loops::Error, Error> {
    Ok(match code {
        1 => loops::Error::InvalidConfig,
        2 => loops::Error::Capacity,
        3 => loops::Error::InvalidId,
        4 => loops::Error::DuplicateId,
        5 => loops::Error::InvalidState,
        6 => loops::Error::InvalidFrame,
        7 => loops::Error::MissingSync,
        8 => loops::Error::Exhausted,
        9 => loops::Error::EmptyContent,
        10 => loops::Error::InvalidProgram,
        _ => return Err(Error::InvalidState),
    })
}
pub fn state(words: &[u32; 26], slot: u32) -> Result<State, Error> {
    if get(words, 0, 1) == 0 {
        return Err(Error::InvalidId);
    }
    let id = LoopId::from_parts(slot, get(words, 1, 32) as u32).map_err(|_| Error::InvalidId)?;
    let mut s = State::from_observation(id, Revision(get(words, 448, 64)));
    s.mode = match get(words, 33, 2) {
        0 => Mode::Stopped,
        1 => Mode::Playing,
        2 => Mode::Recording,
        _ => return Err(Error::InvalidState),
    };
    s.content_length = get(words, 64, 64);
    s.position = get(words, 128, 64);
    s.capture_frames = get(words, 192, 64);
    s.cycles = get(words, 256, 64);
    s.content_revision = Revision(get(words, 320, 64));
    s.capture_revision = Revision(get(words, 384, 64));
    s.repeat_sync = get(words, 35, 1) != 0;
    s.last_pulse = if get(words, 36, 1) != 0 {
        Some(Frame(get(words, 512, 64)))
    } else {
        None
    };
    Ok(s)
}
pub fn transition(command: loops::Request) -> Result<[u32; 20], Error> {
    let code = match command.transition {
        Transition::Stop => 0,
        Transition::Play {
            position: 0,
            retrigger: false,
        } => 1,
        Transition::Record {
            monitor_previous: false,
        } => 2,
        Transition::Replace {
            length: 0,
            preserve_position: false,
            ..
        } => 3,
        _ => return Err(Error::InvalidParameter),
    };
    let mut w = [0; 20];
    if let loops::Timing::AfterTicks { source, skip } = command.timing {
        put(&mut w, 33, 1, 1);
        put(
            &mut w,
            256,
            64,
            u64::from(source.slot()) | (u64::from(source.generation()) << 32),
        );
        put(&mut w, 320, 64, skip);
    }
    if let Transition::Replace { revision, .. } = command.transition {
        put(&mut w, 44, 1, 1);
        put(&mut w, 384, 64, revision.0);
    }
    put(&mut w, 34, 2, code);
    put(
        &mut w,
        64,
        64,
        u64::from(command.target.slot()) | (u64::from(command.target.generation()) << 32),
    );
    put(&mut w, 128, 64, command.sequence.0);
    put(&mut w, 192, 64, command.at.0);
    Ok(w)
}
pub fn completion(words: &[u32; 26], slot: u32) -> Result<Option<loops::Completion>, Error> {
    if get(words, 0, 1) == 0 {
        return Ok(None);
    }
    let frame = Frame(get(words, 576, 64));
    let outcome = match get(words, 37, 3) {
        0 => return Ok(None),
        1 => loops::Outcome::Applied { frame },
        2 => loops::Outcome::Superseded { frame },
        3 => loops::Outcome::Cancelled { frame },
        4 => loops::Outcome::Failed {
            frame,
            reason: error(get(words, 40, 4))?,
        },
        _ => return Err(Error::InvalidState),
    };
    Ok(Some(loops::Completion {
        id: RequestId::from_parts(slot, get(words, 1, 32) as u32).map_err(|_| Error::InvalidId)?,
        sequence: Sequence(get(words, 128, 64)),
        requested: Frame(get(words, 192, 64)),
        effective: Frame(get(words, 448, 64)),
        outcome,
    }))
}
pub fn event(words: &[u32; 26]) -> Result<Event, Error> {
    let kind = match get(words, 254, 2) {
        0 => EventKind::Transition(match get(words, 192, 2) {
            0 => Transition::Stop,
            1 => Transition::Play {
                position: get(words, 128, 64),
                retrigger: get(words, 194, 1) != 0,
            },
            2 => Transition::Record {
                monitor_previous: false,
            },
            3 if get(words, 195, 1) != 0 => Transition::Replace {
                length: 0,
                revision: Revision(get(words, 128, 64)),
                preserve_position: false,
            },
            _ => return Err(Error::InvalidState),
        }),
        1 => EventKind::Wrap,
        2 => EventKind::Pulse,
        _ => return Err(Error::InvalidState),
    };
    Ok(Event {
        frame: Frame(get(words, 0, 64)),
        target: LoopId::from_parts(get(words, 64, 32) as u32, get(words, 96, 32) as u32)
            .map_err(|_| Error::InvalidId)?,
        kind,
    })
}
