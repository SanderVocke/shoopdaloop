#![cfg(not(target_arch = "wasm32"))]

use rt_buffered::{AsyncBatch, Backend, BatchTag};
use rt_runtime::{buffer::MidiBuffer, Error, Frame};
use shoop_remote_fx::{
    describe, serve, shutdown, Action, Chain, ChainControls, Client, Config, Dsp, Kind, Lifecycle,
    MediaLimits, Status,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
use std::{
    net::TcpListener,
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::{Duration, Instant},
};

#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

struct Gain;

impl Dsp for Gain {
    fn process(
        &mut self,
        input: AsyncBatch<'_>,
        output: &mut [f32],
        midi: &mut [MidiBuffer],
    ) -> Result<(), Error> {
        for (wet, dry) in output.iter_mut().zip(input.audio) {
            *wet = dry * 2.;
        }
        for index in 0..input.midi[0].len() {
            let (offset, bytes) = input.midi[0].get(index).unwrap();
            midi[0].push(offset, input.frames, bytes)?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct Controls {
    status: Status,
    project: String,
}

impl ChainControls for Controls {
    fn status(&self) -> Status {
        Status {
            lifecycle: Lifecycle::Running,
            ..self.status
        }
    }
    fn active(&mut self, active: bool) -> Result<(), String> {
        self.status.active = active;
        Ok(())
    }
    fn editor(&mut self, visible: bool) -> Result<(), String> {
        self.status.editor_visible = Some(visible);
        Ok(())
    }
    fn save(&mut self) -> Result<String, String> {
        Ok(self.project.clone())
    }
    fn restore(&mut self, state: String) -> Result<(), String> {
        self.project = state;
        Ok(())
    }
}

#[shoop_test]
fn negotiates_udp_processes_pipelined_batches_and_uses_reliable_state() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let thread = thread::spawn(move || {
        serve(listener, 2, Arc::new(AtomicBool::new(false)), |_, _| {
            Ok(Chain {
                dsp: Box::new(Gain),
                controls: Box::new(Controls::default()),
            })
        })
        .unwrap();
    });
    let description = describe(address).unwrap();
    assert_eq!(description.slots, 2);
    let ports = MediaLimits {
        channels: 2,
        midi: true,
        midi_events: 4,
        midi_bytes: 16,
    };
    let config = Config {
        sample_rate: 8_000,
        quantum: 32,
        batch_quanta: 1,
        slack_quanta: 16,
        input: ports,
        output: ports,
        datagram_bytes: 1200,
        copies: 2,
    };
    let config = Config {
        slack_quanta: 12,
        ..config
    };
    let (mut endpoint, client) = Client::prepare(address, Kind::Rack, config).unwrap();
    let second = Client::prepare(address, Kind::Rack, config).unwrap();
    let refused = Client::prepare(address, Kind::Rack, config).err().unwrap();
    assert!(refused.to_string().contains("prepared capacity"));
    drop(second);
    let observed = client.observation();
    let command = |action| {
        client
            .request(observed.owner, observed.generation, action)
            .unwrap()
            .wait()
            .unwrap()
    };
    command(Action::Restore("test project".into()));
    assert_eq!(command(Action::Save).as_deref(), Some("test project"));
    command(Action::Editor(true));
    assert_eq!(client.observation().status.editor_visible, Some(true));

    let transport = endpoint.async_transport().unwrap();
    let mut buffer = MidiBuffer::new(4, 16).unwrap();
    buffer.push(7, 32, &[0x90, 42, 100]).unwrap();
    let midi = [buffer];
    for sequence in 7..10 {
        assert_no_heap(|| {
            transport
                .try_submit(AsyncBatch {
                    tag: BatchTag {
                        generation: 7,
                        sequence,
                        source: Frame(sequence * 32),
                    },
                    frames: 32,
                    audio: &[0.25; 64],
                    midi: &midi,
                    parameters: &[],
                })
                .unwrap()
        });
    }
    let mut next = 7;
    let end = Instant::now() + Duration::from_secs(1);
    while next < 10 && Instant::now() < end {
        assert_no_heap(|| {
            if let Some(done) = transport.try_completion() {
                assert_eq!(done.tag.sequence, next);
                let output = done.output.unwrap();
                assert_eq!(output.audio, [0.5; 64]);
                assert_eq!(output.midi[0].get(0), Some((7, &[0x90, 42, 100][..])));
                next += 1;
            }
        });
        thread::sleep(Duration::from_micros(100));
    }
    assert_eq!(next, 10);
    drop(endpoint);
    drop(client);
    shutdown(address, description.instance).unwrap();
    thread.join().unwrap();
}
