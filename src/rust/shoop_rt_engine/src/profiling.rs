use core::fmt::{self, Write};
use rt_runtime::profiling::{Status, CHUNK_SAMPLES, PHASE};
use shoop_rt_protocol::{EngineMessage, Lane, Name};
use shoop_rt_transport::carrier::Outbox;

use crate::{endpoint::ControlEndpoint, pool::Pool};

struct Label {
    bytes: [u8; 64],
    len: usize,
}
impl Write for Label {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        if end > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

impl Pool {
    pub(crate) fn profiling_names(&self) -> alloc::boxed::Box<[Name]> {
        let mut names = alloc::vec![Name::new("Node").unwrap(); self.nodes()];
        let mut set = |node: rt_runtime::NodeId, args: fmt::Arguments<'_>| {
            let mut label = Label {
                bytes: [0; 64],
                len: 0,
            };
            let _ = label.write_fmt(args);
            names[node.slot() as usize] =
                Name::new(core::str::from_utf8(&label.bytes[..label.len]).unwrap_or("Node"))
                    .expect("bounded node label");
        };
        set(self.reader(), format_args!("Media export reader"));
        for slot in 0..self.strips() {
            set(
                self.strip_input(slot),
                format_args!("Strip {} input", slot + 1),
            );
            set(
                self.strip_output(slot),
                format_args!("Strip {} output", slot + 1),
            );
        }
        for slot in 0..self.loops() {
            set(
                self.playback(slot),
                format_args!("Loop {} playback", slot + 1),
            );
            set(
                self.capture(slot),
                format_args!("Loop {} capture", slot + 1),
            );
            set(self.fader(slot), format_args!("Loop {} fader", slot + 1));
        }
        for slot in 0..self.buses() {
            set(self.bus(slot), format_args!("Bus {} output", slot + 1));
        }
        names.into_boxed_slice()
    }
}

impl ControlEndpoint {
    pub(crate) fn forward_profiling(&mut self, out: &mut Outbox) {
        if !self.connected {
            return;
        }
        let status = self
            .profiling
            .as_ref()
            .map_or(Status::default(), |reader| reader.control().status());
        if self.profile_status != Some(status) {
            if out.push(EngineMessage::ProfilingStatus(status)).is_err() {
                return;
            }
            self.profile_status = Some(status);
        }
        if self.profile_generation != status.generation {
            self.profile_named.fill(false);
            self.profile_generation = status.generation;
        }
        let Some(reader) = &mut self.profiling else {
            return;
        };
        for _ in 0..128 {
            if out.available(Lane::Profiling) < CHUNK_SAMPLES + 1 {
                break;
            }
            let Some(packet) = reader.next_packet() else {
                break;
            };
            for sample in &packet.samples[..usize::from(packet.len)] {
                if sample.key & PHASE == 0 {
                    let key = sample.key & !rt_runtime::profiling::LAYOUT;
                    if let Some(named) = self.profile_named.get_mut(key as usize) {
                        if !*named {
                            let _ = out.push(EngineMessage::ProfilingNode {
                                generation: packet.generation,
                                key,
                                name: self.profile_names[key as usize],
                            });
                            *named = true;
                        }
                    }
                }
            }
            let _ = out.push(EngineMessage::Profiling(packet));
        }
    }
}
