use shoop_app_api::NewEnginePools;
use shoop_rt_host_native::{DummyConfig, DummyControl, DummyMode, NativeClient, NativeConfig};
use shoop_rt_protocol::*;
use std::time::{Duration, Instant};
fn request(client: &mut NativeClient, dummy: &DummyControl, request: Request) {
    let correlation = client.request(request).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        while let Some(message) = client.poll() {
            if let EngineMessage::Result {
                correlation: id,
                outcome,
            } = message
            {
                if id == correlation {
                    assert_eq!(outcome, Outcome::Done);
                    return;
                }
            }
        }
        assert!(Instant::now() < deadline);
        dummy.request_frames(256);
        assert!(dummy.wait_rendered(Duration::from_secs(5)));
    }
}
fn main() {
    for megabytes in [16, 256] {
        let description = shoop_host::engine::engine_description(
            &NewEnginePools {
                tracks: 8,
                loops: 80,
                channels: 2,
                loop_memory_mb: megabytes,
            },
            256,
        );
        let page_bytes = description.page_bytes;
        let pages = description.resource_pages;
        let started = shoop_rt_host_native::start(&NativeConfig {
            driver: shoop_rt_host_native::DriverConfig::Dummy(DummyConfig {
                mode: DummyMode::Controlled,
                buffer_frames: 256,
                input: Some(|_, _| 0.25),
                ..DummyConfig::default()
            }),
            engine: description,
        })
        .unwrap();
        let host = started.host;
        let mut client = started.client;
        let dummy = host.dummy().unwrap().clone();
        client.connect().unwrap();
        while client.poll().is_some() {}
        let strip = StripId(SlotId::new(1, 1));
        let loop_id = LoopSlotId(SlotId::new(2, 1));
        request(
            &mut client,
            &dummy,
            Request::CreateStrip {
                strip,
                shape: StripShape {
                    audio_inputs: 1,
                    audio_outputs: 1,
                    midi_input: false,
                    midi_output: false,
                },
            },
        );
        request(
            &mut client,
            &dummy,
            Request::Connect {
                port: EnginePort::Strip(StripPort {
                    strip,
                    kind: PortKind::AudioIn,
                    index: 0,
                }),
                host: HostPortId(0),
                connected: true,
            },
        );
        request(&mut client, &dummy, Request::CreateLoop { loop_id, strip });
        request(
            &mut client,
            &dummy,
            Request::LoopTransition {
                loop_id,
                mode: LoopMode::Recording,
                delay: None,
            },
        );
        let first = host.status().next_frame;
        let mut seen_recording = false;
        let mut stopped_at = None;
        let mut last = None;
        for _ in 0..22_500 {
            dummy.request_frames(256);
            assert!(dummy.wait_rendered(Duration::from_secs(5)));
            while let Some(message) = client.poll() {
                if let EngineMessage::Telemetry(Telemetry::Loop {
                    loop_id: id,
                    mode,
                    length,
                    ..
                }) = message
                {
                    if id == loop_id {
                        seen_recording |= mode == LoopMode::Recording;
                        if seen_recording && mode == LoopMode::Stopped && stopped_at.is_none() {
                            stopped_at = Some(host.status().next_frame - first);
                        }
                        last = Some((mode, length));
                    }
                }
            }
        }
        assert!(seen_recording);
        println!("pool_mb={megabytes} pages={pages} page_bytes={page_bytes} rendered_seconds=120 stopped_after_frames={stopped_at:?} final={last:?}");
        if megabytes == 16 {
            assert!(stopped_at.is_some());
        } else {
            assert!(stopped_at.is_none());
        }
        drop(client);
        assert!(host.stop().stopped.is_ok());
    }
}
