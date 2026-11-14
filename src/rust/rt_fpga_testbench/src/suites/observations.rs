//! Accepted cut history and ownership through the real public command path.
use crate::{Result, bits::Bits, model::Design, sim::Sim};
use rt_fpga_adapter::{
    audio_actions::{Identity, Progress, Record, Service},
    audio_image::Transfer,
    observations::{Reader, Reference, Scope},
    protocol, resident,
};
use rt_runtime::{Epoch, Frame};
use shoop_loops::{
    LoopId,
    audio::Smoothing,
    runtime::{Reply, Selection},
};
fn exchange(s: &mut Sim<'_>, t: &Transfer) -> (u8, [u32; 26]) {
    s.model.set_all(&[
        ("request_valid", 1),
        ("request_opcode", t.opcode.into()),
        ("request_state", u64::from(t.state)),
        ("request_address", t.address.into()),
    ]);
    s.model.set_bits("request_data", &Bits::from(t.words));
    s.until(1000, "observation command ready", |m| {
        m.get("request_ready") != 0
    });
    s.idle(1);
    s.model.set("request_valid", 0);
    s.until(20000, "observation command reply", |m| {
        m.get("response_valid") != 0
    });
    let result = (
        s.model.get("response_error") as u8,
        s.model.bits("response_data").array(),
    );
    s.idle(1);
    result
}
fn service(s: &mut Sim<'_>, owner: &mut Service, mut t: Transfer) -> Progress {
    loop {
        let (error, row) = exchange(s, &t);
        match owner
            .finish(error, row, s.model.get("current_frame"))
            .unwrap()
        {
            Progress::Transfer(next) => t = next,
            result => return result,
        }
    }
}
fn query(target: u64, selection: Option<u64>) -> Transfer {
    let target = LoopId::from_parts(target as u32, (target >> 32) as u32).unwrap();
    match selection {
        None => resident::state(target),
        Some(selection) => resident::export(
            target,
            match selection {
                0 => Selection::Playback,
                1 => Selection::Latest,
                _ => Selection::Recording,
            },
        ),
    }
}

fn cut(s: &mut Sim<'_>, mode: u64) {
    s.model.set_all(&[("cut_start", 1), ("cut_mode", mode)]);
    s.idle(1);
    s.model.set("cut_start", 0);
    s.until(20000, "bounded query cut", |m| {
        m.get("cut_done") != 0 || m.get("fault") != 0
    });
    assert_eq!(s.model.get("fault"), 0);
    s.idle(1);
}
fn schedule(s: &mut Sim<'_>, owner: &mut Service, sequence: &mut u64, t: Transfer) -> Identity {
    *sequence += 1;
    let t = owner
        .schedule(*sequence, s.model.get("current_frame"), t)
        .unwrap();
    let id = match service(s, owner, t) {
        Progress::Admitted { identity, .. } => identity,
        _ => panic!("observation admission refused"),
    };
    cut(s, 0);
    cut(s, 1);
    cut(s, 2);
    id
}
fn record(s: &mut Sim<'_>, owner: &mut Service, id: Identity, release: bool) -> Record {
    let t = owner.inspect(id, release).unwrap();
    match service(s, owner, t) {
        Progress::Record { record, .. } => record,
        _ => panic!("observation record refused"),
    }
}
fn failure(
    s: &mut Sim<'_>,
    owner: &mut Service,
    sequence: &mut u64,
    t: Transfer,
    tag: u8,
    reason: u8,
) {
    let id = schedule(s, owner, sequence, t);
    let row = record(s, owner, id, true);
    assert_eq!(resident::tag(&row.payload), tag);
    assert_eq!(
        protocol::get(&row.payload, 0, 8),
        u64::from(reason),
        "public row {row:?}"
    );
}
fn observe(
    s: &mut Sim<'_>,
    frame: u64,
    mode: u64,
    content: u64,
    capture: u64,
    frames: u64,
    length: u64,
    graph: u64,
) {
    let mut state = Bits::zero(832);
    state
        .set(0, 1, 1)
        .set(1, 32, 1)
        .set(33, 2, mode)
        .set(64, 64, length)
        .set(192, 64, frames)
        .set(320, 64, content)
        .set(384, 64, capture)
        .set(448, 64, content.max(capture));
    s.model.set_bits("observe_state", &state);
    s.model.set_all(&[
        ("observe_valid", 1),
        ("current_frame", frame),
        ("graph_epoch", graph),
    ]);
    s.until(1000, "ledger observation ready", |m| {
        m.get("observe_ready") != 0
    });
    s.idle(1);
    s.model.set("observe_valid", 0);
    s.until(2000, "ledger observation done", |m| {
        m.get("observe_done") != 0
    });
    assert_eq!(s.model.get("observe_error"), 0);
    s.idle(1);
}
fn read(s: &mut Sim<'_>, reference: Reference) -> [u64; 30] {
    let mut reader = Reader::new(reference);
    while let Some(t) = reader.request() {
        let (error, words) = exchange(s, &t);
        reader.accept(error, words).unwrap_or_else(|failure| {
            panic!(
                "reader {reference:?}, request {:?}, error {error}, result {words:?}: {failure:?}",
                t.words
            )
        });
    }
    reader.finish().unwrap()
}
fn scalar(s: &mut Sim<'_>, gain: f32, balance: f32, frames: u32, linear: bool) {
    let mut data = Bits::zero(97);
    data.set(0, 32, gain.to_bits().into())
        .set(32, 32, balance.to_bits().into())
        .set(64, 32, frames.into())
        .set(96, 1, u64::from(linear));
    s.model.set_bits("scalar_data", &data);
}
fn inspect(s: &mut Sim<'_>, slot: u32, generation: u32) -> u64 {
    s.model.set_all(&[
        ("inspect_valid", 1),
        ("inspect_slot", slot.into()),
        ("inspect_generation", generation.into()),
    ]);
    s.until(1000, "borrowed content ready", |m| {
        m.get("inspect_ready") != 0
    });
    s.idle(1);
    s.model.set("inspect_valid", 0);
    s.until(1000, "borrowed content result", |m| {
        m.get("inspect_done") != 0
    });
    let error = s.model.get("inspect_error");
    s.idle(1);
    error
}
pub fn historical_queries() -> Result<()> {
    let root = crate::root();
    let compiled = Design::new(
        "observations_probe",
        vec![
            root.join("fpga/loop_pipeline/tests/observations_probe.sv"),
            root.join("fpga/audio_io/rtl/audio_actions.sv"),
            root.join("fpga/audio_io/rtl/audio_native.sv"),
            root.join("fpga/audio_io/rtl/audio_observations.sv"),
            root.join("fpga/loop_pipeline/rtl/native_requests.sv"),
            root.join("fpga/loop_pipeline/rtl/media_cuts.sv"),
            root.join("fpga/loop_pipeline/rtl/cut_records.sv"),
            root.join("fpga/state_processor/rtl/sp_ram.sv"),
        ],
    )
    .build(&root.join("work/marlin-suite/public-observations"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    super::media_cuts::attach_views(&mut s);
    s.model
        .set_all(&[("reset", 1), ("epoch", 7), ("topology_epoch", 5)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(12);
    scalar(&mut s, 1.25, -0.25, 0, true);
    let origin = Frame(1_u64 << 32);
    let scope = Scope::new(2, origin)?;
    let target = LoopId::from_parts(0, 1).unwrap();
    let loop_id = 1_u64 << 32;
    let mut owner = Service::new(8, 32)?;
    let mut sequence = 0;
    failure(
        &mut s,
        &mut owner,
        &mut sequence,
        query(loop_id + 1, None),
        resident::RUNTIME_ERROR,
        6,
    );
    observe(&mut s, 0, 0, 0, 0, 0, 0, 3);
    failure(
        &mut s,
        &mut owner,
        &mut sequence,
        query(loop_id + 1, None),
        resident::TIMELINE_ERROR,
        3,
    );
    failure(
        &mut s,
        &mut owner,
        &mut sequence,
        query(loop_id, Some(2)),
        resident::RUNTIME_ERROR,
        2,
    );
    failure(
        &mut s,
        &mut owner,
        &mut sequence,
        query(loop_id, Some(1)),
        resident::RUNTIME_ERROR,
        6,
    );
    let empty = schedule(&mut s, &mut owner, &mut sequence, query(loop_id, Some(0)));
    let row = record(&mut s, &mut owner, empty, false);
    let reference = Reference::decode(&row, 7)?;
    let Reply::Export {
        id: empty_export,
        media: empty_media,
        ..
    } = scope.decode(
        reference,
        &read(&mut s, reference),
        target,
        Some(Selection::Playback),
    )?
    else {
        panic!("empty export missing")
    };
    assert_eq!(empty_media.view.description.frames, 0);
    assert!(empty_media.view.audio.is_none() && empty_media.view.windows.is_none());
    record(&mut s, &mut owner, empty, true);
    failure(
        &mut s,
        &mut owner,
        &mut sequence,
        scope.request_cursor(empty_export, false),
        resident::RUNTIME_ERROR,
        7,
    );
    let id = schedule(
        &mut s,
        &mut owner,
        &mut sequence,
        scope.release(empty_export),
    );
    assert_eq!(
        resident::tag(&record(&mut s, &mut owner, id, true).payload),
        resident::DONE
    );
    observe(&mut s, 0, 2, 0, 1, 0, 0, 3);
    observe(&mut s, 32, 2, 0, 1, 32, 0, 3);
    observe(&mut s, 64, 1, 1, 1, 64, 64, 3);
    let mut ids = vec![];
    for i in 0..8 {
        ids.push(schedule(
            &mut s,
            &mut owner,
            &mut sequence,
            query(loop_id, if i < 4 { None } else { Some(1) }),
        ));
    }
    assert_eq!(owner.schedule(999, 64, query(loop_id, None)).err(), Some(2));
    scalar(&mut s, 2., 0.75, 32, true);
    s.model.set("topology_epoch", 99);
    for q in 0..1050u64 {
        let capture = 2 + q / 2;
        let playing = q % 2 == 1;
        observe(
            &mut s,
            96 + q * 32,
            if playing { 1 } else { 2 },
            if playing { capture } else { capture - 1 },
            capture,
            if playing { 32 } else { 0 },
            if q == 0 { 64 } else { 32 },
            7 + q / 64,
        );
        cut(&mut s, 0);
        cut(&mut s, 1);
        cut(&mut s, 2);
        assert_eq!(s.model.get("request_valid"), 0);
    }
    let mut exports = vec![];
    let mut saved = vec![];
    for (i, &id) in ids.iter().enumerate() {
        let row = record(&mut s, &mut owner, id, false);
        let reference = Reference::decode(&row, 7)?;
        let raw = read(&mut s, reference);
        let reply = scope.decode(
            reference,
            &raw,
            target,
            if i < 4 { None } else { Some(Selection::Latest) },
        )?;
        let cut = match reply {
            Reply::State(cut) => cut,
            Reply::Export { id, cut, media } => {
                exports.push(id);
                assert_eq!(media.view.description.frames, 64);
                assert_eq!(media.view.description.revision.0, 1);
                let source = media.view.audio.unwrap();
                assert_eq!(source.snapshot.cut_frame, Frame(origin.0 + 64));
                assert_eq!(source.snapshot.capture.unwrap().start_frame, origin);
                assert_eq!(source.snapshot.description.bytes, 512);
                assert_eq!(source.snapshot.revision.0, 3);
                cut
            }
            _ => panic!("historical query changed kind"),
        };
        assert_eq!(cut.frame, Frame(origin.0 + 64));
        assert_eq!((cut.graph_epoch, cut.topology_epoch), (Epoch(3), Epoch(5)));
        assert_eq!((cut.gain, cut.balance), (1.25, -0.25));
        assert_eq!(cut.smoothing, Smoothing::LinearRestart { frames: 0 });
        assert_eq!(cut.raw_audio_frames, 64);
        saved.push((reference, raw));
    }
    let (reference, raw) = saved[0];
    let mut corrupted = raw;
    corrupted[29] |= 2;
    assert!(matches!(
        scope.decode(reference, &corrupted, target, None),
        Err(rt_runtime::Error::Disconnected)
    ));
    let mut reader = Reader::new(reference);
    let (error, words) = exchange(&mut s, &reader.request().unwrap());
    assert_eq!(error, 0);
    let mut wrong = words;
    wrong[4] ^= 1;
    assert_eq!(
        reader.accept(0, wrong),
        Err(rt_runtime::Error::Disconnected)
    );
    assert_eq!(reader.accept(0, words), Ok(false));
    assert_eq!(reader.finish(), Err(rt_runtime::Error::Incomplete));
    for &id in &ids {
        record(&mut s, &mut owner, id, true);
    }
    let foreign = Scope::new(2, Frame(0))?;
    failure(
        &mut s,
        &mut owner,
        &mut sequence,
        foreign.release(exports[0]),
        resident::RUNTIME_ERROR,
        1,
    );
    failure(
        &mut s,
        &mut owner,
        &mut sequence,
        foreign.request_cursor(exports[0], true),
        resident::RUNTIME_ERROR,
        2,
    );

    let old = saved[0].0;
    let t = Reader::new(old).request().unwrap();
    assert_eq!(
        exchange(&mut s, &t).0,
        3,
        "acknowledged record remained readable"
    );
    failure(
        &mut s,
        &mut owner,
        &mut sequence,
        query(loop_id, Some(1)),
        resident::RUNTIME_ERROR,
        3,
    );
    let id = schedule(&mut s, &mut owner, &mut sequence, scope.release(exports[0]));
    assert_eq!(
        resident::tag(&record(&mut s, &mut owner, id, true).payload),
        resident::DONE
    );
    let id = schedule(
        &mut s,
        &mut owner,
        &mut sequence,
        scope.request_cursor(exports[1], false),
    );
    let row = record(&mut s, &mut owner, id, true);
    let cursor = scope.cursor(&row.payload)?;
    let snapshot = scope.cursor_snapshot(cursor)?;
    assert_eq!(inspect(&mut s, snapshot.slot, snapshot.generation), 0);
    for &export in &exports[1..] {
        let id = schedule(&mut s, &mut owner, &mut sequence, scope.release(export));
        assert_eq!(
            resident::tag(&record(&mut s, &mut owner, id, true).payload),
            resident::DONE
        );
    }
    assert_eq!(
        inspect(&mut s, snapshot.slot, snapshot.generation),
        3,
        "borrowed cursor pinned retired content"
    );
    failure(
        &mut s,
        &mut owner,
        &mut sequence,
        scope.request_cursor(exports[1], false),
        resident::RUNTIME_ERROR,
        1,
    );
    let id = schedule(
        &mut s,
        &mut owner,
        &mut sequence,
        scope.retire(empty_media.media),
    );
    assert_eq!(
        resident::tag(&record(&mut s, &mut owner, id, true).payload),
        resident::DONE
    );
    let id = schedule(&mut s, &mut owner, &mut sequence, query(loop_id, None));
    let row = record(&mut s, &mut owner, id, false);
    let new = Reference::decode(&row, 7)?;
    let Reply::State(cut) = scope.decode(new, &read(&mut s, new), target, None)? else {
        panic!("new query missing")
    };
    assert_eq!(
        (cut.gain, cut.balance, cut.topology_epoch),
        (2., 0.75, Epoch(99))
    );
    assert_eq!(cut.smoothing, Smoothing::LinearRestart { frames: 32 });
    assert_eq!(
        exchange(&mut s, &Reader::new(old).request().unwrap()).0,
        3,
        "old generation aliased reused record"
    );
    let mut bad_word = Reader::new(new).request().unwrap();
    bad_word.words[2] = 255;
    assert_eq!(exchange(&mut s, &bad_word).0, 3);
    s.model.set_all(&[("reset", 1), ("epoch", 8)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(12);
    observe(&mut s, 0, 0, 0, 0, 0, 0, 3);
    let mut owner = Service::new(8, 32)?;
    let id = schedule(&mut s, &mut owner, &mut sequence, query(loop_id, None));
    let row = record(&mut s, &mut owner, id, false);
    assert_eq!(
        Reference::decode(&row, 7),
        Err(rt_runtime::Error::StaleEpoch)
    );
    let current = Reference::decode(&row, 8)?;
    read(&mut s, current);
    assert_eq!(
        exchange(&mut s, &Reader::new(new).request().unwrap()).0,
        3,
        "old session aliased fresh record"
    );
    assert_eq!(s.model.get("fault"), 0);
    println!(
        "public observations: all8 retained credits, 1,050 advancing cuts, historical scalar/epoch/media data, checked record ACK before row reuse, export lease and borrowed cursor lifetimes, stale generation/session and word bounds"
    );
    Ok(())
}
