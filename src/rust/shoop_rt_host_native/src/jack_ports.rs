//! Prepared callback bindings; owners retire back to the control thread.
use jack::{AudioIn, AudioOut, MidiIn, MidiOut, Port, ProcessScope};
use rtrb::{Consumer, Producer, RingBuffer};
use shoop_port_graph::{DataType, Direction, Error, ErrorKind, PortSpec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Slot {
    pub kind: usize,
    pub index: usize,
}
pub(crate) enum Owned {
    AudioIn(Port<AudioIn>),
    AudioOut(Port<AudioOut>),
    MidiIn(Port<MidiIn>),
    MidiOut(Port<MidiOut>),
}
impl Owned {
    pub fn register(client: &jack::Client, spec: &PortSpec) -> Result<Self, Error> {
        let failure = |e| Error::new(ErrorKind::Backend, format!("{e}"));
        Ok(match (spec.data_type, spec.direction) {
            (DataType::Audio, Direction::Input) => Self::AudioIn(
                client
                    .register_port(&spec.name, AudioIn::default())
                    .map_err(failure)?,
            ),
            (DataType::Audio, Direction::Output) => Self::AudioOut(
                client
                    .register_port(&spec.name, AudioOut::default())
                    .map_err(failure)?,
            ),
            (DataType::Midi, Direction::Input) => Self::MidiIn(
                client
                    .register_port(&spec.name, MidiIn::default())
                    .map_err(failure)?,
            ),
            (DataType::Midi, Direction::Output) => Self::MidiOut(
                client
                    .register_port(&spec.name, MidiOut::default())
                    .map_err(failure)?,
            ),
        })
    }
    pub fn unowned(&self) -> Port<jack::Unowned> {
        match self {
            Self::AudioIn(p) => p.clone_unowned(),
            Self::AudioOut(p) => p.clone_unowned(),
            Self::MidiIn(p) => p.clone_unowned(),
            Self::MidiOut(p) => p.clone_unowned(),
        }
    }
    fn clear(&mut self, scope: &ProcessScope, reset: &mut u8) {
        match self {
            Self::AudioOut(port) => {
                port.as_mut_slice(scope).fill(0.);
                *reset = 32;
            }
            Self::MidiOut(port) => {
                let mut writer = port.writer(scope);
                while *reset < 32 {
                    if writer
                        .write(&jack::RawMidi {
                            time: 0,
                            bytes: &[0xb0 | (*reset / 2), [123, 120][(*reset % 2) as usize], 0],
                        })
                        .is_err()
                    {
                        break;
                    }
                    *reset += 1;
                }
            }
            _ => *reset = 32,
        }
    }
}
struct Update {
    ticket: u64,
    slot: Slot,
    port: Option<Owned>,
}
pub(crate) struct Ack {
    pub ticket: u64,
    pub retired: Option<Owned>,
}
pub(crate) struct Callback {
    pub inputs: Box<[Option<Port<AudioIn>>]>,
    pub outputs: Box<[Option<Port<AudioOut>>]>,
    pub midi_inputs: Box<[Option<Port<MidiIn>>]>,
    pub midi_outputs: Box<[Option<Port<MidiOut>>]>,
    updates: Consumer<Update>,
    acknowledgements: Producer<Ack>,
    retired: Option<(u64, Option<Owned>, u8, u8)>,
    acknowledgement: Option<Ack>,
}
pub(crate) struct Control {
    updates: Producer<Update>,
    acknowledgements: Consumer<Ack>,
    used: [Vec<bool>; 4],
    names: [Vec<String>; 4],
    next: u64,
}
fn slots<T>(capacity: usize) -> Box<[Option<Port<T>>]> {
    (0..capacity).map(|_| None).collect()
}
pub(crate) fn prepare(names: [Vec<String>; 4]) -> (Callback, Control) {
    let (send, receive) = RingBuffer::new(2);
    let (ack, completed) = RingBuffer::new(2);
    let used = core::array::from_fn(|kind| vec![false; names[kind].len()]);
    let callback = Callback {
        inputs: slots(names[0].len()),
        outputs: slots(names[1].len()),
        midi_inputs: slots(names[2].len()),
        midi_outputs: slots(names[3].len()),
        updates: receive,
        acknowledgements: ack,
        retired: None,
        acknowledgement: None,
    };
    (
        callback,
        Control {
            updates: send,
            acknowledgements: completed,
            used,
            names,
            next: 1,
        },
    )
}
impl Control {
    pub fn allocate(&mut self, spec: &PortSpec) -> Result<Slot, Error> {
        let kind = match (spec.data_type, spec.direction) {
            (DataType::Audio, Direction::Input) => 0,
            (DataType::Audio, Direction::Output) => 1,
            (DataType::Midi, Direction::Input) => 2,
            (DataType::Midi, Direction::Output) => 3,
        };
        let index = self.used[kind]
            .iter()
            .position(|used| !used)
            .ok_or_else(|| {
                Error::new(ErrorKind::Capacity, "prepared host I/O capacity exhausted")
            })?;
        self.used[kind][index] = true;
        Ok(Slot { kind, index })
    }
    pub fn release(&mut self, slot: Slot) {
        self.used[slot.kind][slot.index] = false;
    }
    pub fn name(&self, slot: Slot) -> &str {
        &self.names[slot.kind][slot.index]
    }
    pub fn stage(&mut self, slot: Slot, port: Option<Owned>) -> Result<u64, Error> {
        let ticket = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::Capacity, "I/O handoff identity exhausted"))?;
        self.updates
            .push(Update { ticket, slot, port })
            .map_err(|_| Error::new(ErrorKind::Busy, "I/O handoff queue is full"))?;
        Ok(ticket)
    }
    pub fn completed(&mut self) -> Option<Ack> {
        self.acknowledgements.pop().ok()
    }
}
impl Callback {
    pub fn begin(&mut self, scope: &ProcessScope) {
        if let Some((ticket, port, remaining, reset)) = &mut self.retired {
            if let Some(port) = port {
                port.clear(scope, reset);
            }
            if *reset == 32 {
                *remaining -= 1;
            }
            if *remaining == 0 {
                let ticket = *ticket;
                let (_, port, _, _) = self.retired.take().unwrap();
                self.acknowledgement = Some(Ack {
                    ticket,
                    retired: port,
                });
            }
        }
        if self.retired.is_some() || self.acknowledgement.is_some() {
            return;
        }
        let Ok(update) = self.updates.pop() else {
            return;
        };
        let index = update.slot.index;
        if let Some(port) = update.port {
            let retired = match port {
                Owned::AudioIn(p) => self.inputs[index].replace(p).map(Owned::AudioIn),
                Owned::AudioOut(p) => self.outputs[index].replace(p).map(Owned::AudioOut),
                Owned::MidiIn(p) => self.midi_inputs[index].replace(p).map(Owned::MidiIn),
                Owned::MidiOut(p) => self.midi_outputs[index].replace(p).map(Owned::MidiOut),
            };
            self.acknowledgement = Some(Ack {
                ticket: update.ticket,
                retired,
            });
        } else {
            let mut retired = match update.slot.kind {
                0 => self.inputs[index].take().map(Owned::AudioIn),
                1 => self.outputs[index].take().map(Owned::AudioOut),
                2 => self.midi_inputs[index].take().map(Owned::MidiIn),
                _ => self.midi_outputs[index].take().map(Owned::MidiOut),
            };
            let mut reset = 32;
            if let Some(port) = &mut retired {
                reset = 0;
                port.clear(scope, &mut reset);
            }
            self.retired = Some((update.ticket, retired, 3, reset));
        }
    }
    pub fn finish(&mut self) {
        if let Some(ack) = self.acknowledgement.take() {
            if let Err(rtrb::PushError::Full(ack)) = self.acknowledgements.push(ack) {
                self.acknowledgement = Some(ack);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
    use std::time::{Duration, Instant};
    struct Guarded(Callback);
    impl jack::ProcessHandler for Guarded {
        fn process(&mut self, _: &jack::Client, scope: &ProcessScope) -> jack::Control {
            assert_no_heap(|| {
                self.0.begin(scope);
                self.0.finish();
            });
            jack::Control::Continue
        }
    }
    #[shoop_test(no_wasm = "real JACK callback ownership handoff")]
    fn publication_handoff_and_retirement_do_not_allocate_or_destroy_owners_on_callback() {
        let (client, _) =
            match jack::Client::new("handoff-heap-guard", jack::ClientOptions::NO_START_SERVER) {
                Ok(client) => client,
                Err(_) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => return,
                Err(error) => panic!("required JACK server unavailable: {error}"),
            };
        let (callback, mut control) =
            prepare(core::array::from_fn(|kind| vec![format!("slot_{kind}")]));
        let active = client.activate_async((), Guarded(callback)).unwrap();
        for (data_type, direction) in [
            (DataType::Audio, Direction::Input),
            (DataType::Audio, Direction::Output),
            (DataType::Midi, Direction::Input),
            (DataType::Midi, Direction::Output),
        ] {
            let spec = PortSpec {
                name: format!("{data_type:?}_{direction:?}"),
                data_type,
                direction,
            };
            let slot = control.allocate(&spec).unwrap();
            let owned = Owned::register(active.as_client(), &spec).unwrap();
            let await_ack = |control: &mut Control, ticket| {
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if let Some(ack) = control.completed() {
                        assert_eq!(ack.ticket, ticket);
                        break ack;
                    }
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
            };
            // Stage outside the callback's guarded scope, reclaim only after acknowledgement.
            let ticket = control.stage(slot, Some(owned)).unwrap();
            assert!(await_ack(&mut control, ticket).retired.is_none());
            let ticket = control.stage(slot, None).unwrap();
            let retired = await_ack(&mut control, ticket).retired.unwrap();
            active
                .as_client()
                .unregister_port(retired.unowned())
                .unwrap();
            drop(retired);
            control.release(slot);
        }
    }
}
