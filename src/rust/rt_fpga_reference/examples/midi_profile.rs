use rt_fpga_reference::{
    midi::{Emission, EmissionKind, Engine, Event, Interpreter, Message, MidiState, Take, Work},
    records::Identity,
};
use std::{hint::black_box, time::Instant};

fn main() {
    const EVENTS: u32 = 8192;
    const DENSITY: usize = 256;
    const REPEATS: usize = 101;
    let events: [Event; DENSITY] = core::array::from_fn(|i| Event {
        offset: (i / 2) as u32,
        message: Message::managed(&[
            if i % 2 == 0 { 0x90 } else { 0x80 },
            (i % 128) as u8,
            1 + (i % 127) as u8,
        ])
        .unwrap(),
    });
    let mut take = Take::prepare(EVENTS).unwrap();
    let mut interpreter = Interpreter::default();
    let initial = MidiState::default();
    let mut times = Vec::with_capacity(REPEATS);
    for _ in 0..REPEATS {
        let start = Instant::now();
        take.begin(&initial).unwrap();
        for _ in 0..EVENTS as usize / DENSITY {
            take.append_window(128, &events).unwrap();
        }
        take.finish().unwrap();
        let mut cursor = take.start_playback().unwrap();
        while let Some(record) = cursor.peek(&take).unwrap() {
            interpreter.apply(black_box(record.message)).unwrap();
            cursor.acknowledge(&take).unwrap();
        }
        take.end_playback(cursor).unwrap();
        times.push(start.elapsed().as_nanos());
        interpreter = Interpreter::default();
    }
    times.sort_unstable();
    let engine = Engine::prepare(
        Identity {
            slot: 0,
            generation: 1,
        },
        EVENTS,
        DENSITY as u32,
    )
    .unwrap();
    println!("events={EVENTS} events_per_128_frames={DENSITY} repeats={REPEATS} median_capture_read_state_ns={} min_ns={} max_ns={} prepared_engine_bytes={} state_bytes={}",
        times[REPEATS/2],times[0],times[REPEATS-1],engine.prepared_bytes(),size_of::<MidiState>());
    for bytes in [
        &[0x90, 60, 80][..],
        &[0xb0, 64, 127],
        &[0xb0, 64, 0],
        &[0xff],
    ] {
        let before = interpreter.steps();
        interpreter.apply(Message::managed(bytes).unwrap()).unwrap();
        println!(
            "message={bytes:?} update_steps={}",
            interpreter.steps() - before
        );
    }
    for kind in [
        EmissionKind::Cleanup,
        EmissionKind::Restore,
        EmissionKind::Recovery,
    ] {
        let mut emission = Emission::new(kind);
        let mut work = Work::default();
        while !emission.complete() {
            emission
                .step(interpreter.state().unwrap(), &mut work, &mut |_| Ok(()))
                .unwrap();
        }
        println!(
            "emission={kind:?} visited_fields={} accepted_messages={}",
            work.emission_steps, work.accepted
        );
    }
}
