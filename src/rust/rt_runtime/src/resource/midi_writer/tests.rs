use super::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

const Q: usize = 32;
#[derive(Clone, Copy)]
struct Original {
    frame: u64,
    order: u64,
    bytes: [u8; 64],
    len: usize,
}
fn original(frame: u64, order: u64, bytes: &[u8]) -> Original {
    let mut data = [0; 64];
    data[..bytes.len()].copy_from_slice(bytes);
    Original {
        frame,
        order,
        bytes: data,
        len: bytes.len(),
    }
}
struct Oracle {
    rows: Vec<Original>,
    initial: MidiState,
    frames: u64,
}
impl Oracle {
    fn new(rows: &[Original], initial: &MidiState, frames: u64) -> Self {
        let mut owned = Vec::with_capacity(96);
        owned.extend_from_slice(rows);
        Self {
            rows: owned,
            initial: *initial,
            frames,
        }
    }
    fn write(&mut self, position: u64, fixed: bool, before: &MidiState, input: &MidiBuffer) {
        self.rows
            .retain(|row| row.frame < position || row.frame >= position + Q as u64);
        if position == 0 {
            self.initial = *before;
        }
        for i in 0..input.len() {
            let (offset, bytes) = input.get(i).unwrap();
            self.rows
                .push(original(position + u64::from(offset), i as u64, bytes));
        }
        self.rows.sort_unstable_by_key(|row| (row.frame, row.order));
        if !fixed {
            self.frames = position + Q as u64;
        }
    }
}
fn resources() -> ResourceStore {
    ResourceStore::new(
        crate::Config {
            page_bytes: 64,
            resource_pages: 256,
            ..crate::Config::default()
        }
        .prepare()
        .unwrap(),
        Quantum::new(Q).unwrap(),
    )
    .unwrap()
}
fn config() -> Config {
    Config {
        events_per_tick: 8,
        bytes_per_tick: 128,
        packets: midi_packets::Config {
            pages: 128,
            bodies: 512,
            records: 2048,
            max_packet: 64,
        },
        indexes: store::Config {
            leases: 32,
            versions: 16,
            category_pages: 128,
            packet_nodes: 4096,
            scalar_nodes: 4096,
            hold_nodes: 1024,
            sostenuto_nodes: 1024,
        },
    }
}
fn admit(
    core: &mut Core,
    resources: &mut ResourceStore,
    media: Media,
    kind: Kind,
    position: u64,
) -> Writer {
    let mut admission = core
        .begin_writer(media, kind)
        .map_err(|(error, _)| error)
        .unwrap();
    let mut steps = 0;
    while !core
        .progress_writer(&mut resources.render_io(), &mut admission, 1)
        .unwrap()
    {
        steps += 1;
        assert!(steps < 100);
    }
    core.finish_admission(admission, position)
        .map_err(|(error, _)| error)
        .unwrap()
}
fn check(core: &mut Core, resources: &mut ResourceStore, media: &Media, oracle: &Oracle) {
    assert_eq!(media.frames, oracle.frames);
    assert_eq!(media.initial, oracle.initial);
    let totals = core.totals(&mut resources.render_io(), media).unwrap();
    assert_eq!(totals.events, oracle.rows.len());
    assert_eq!(
        totals.bytes,
        oracle.rows.iter().map(|row| row.len + 12).sum::<usize>()
    );
    let mut packet = None;
    for row in &oracle.rows {
        packet = core
            .first(&mut resources.render_io(), media, packet)
            .unwrap();
        let event = packet.unwrap();
        assert_eq!(
            (event.frame, event.order, event.bytes),
            (row.frame, row.order, row.len)
        );
        let mut bytes = [0; 64];
        assert_eq!(
            core.read_packet(&mut resources.render_io(), event, 0, &mut bytes),
            Ok(row.len)
        );
        assert_eq!(&bytes[..row.len], &row.bytes[..row.len]);
    }
    assert!(core
        .first(&mut resources.render_io(), media, packet)
        .unwrap()
        .is_none());
    for frame in [0, 1, 7, 8, Q as u64, oracle.frames] {
        let mut expected = oracle.initial;
        for row in oracle.rows.iter().filter(|row| row.frame < frame) {
            expected.apply_packet(&row.bytes[..row.len], 64).unwrap();
        }
        assert_eq!(
            core.state(&mut resources.render_io(), media, frame)
                .unwrap(),
            expected,
            "at {frame}"
        );
    }
}
fn cleanup(core: &mut Core, resources: &mut ResourceStore) {
    core.begin_cleanup(&mut resources.render_io()).unwrap();
    while core.maintain(&mut resources.render_io(), 1).unwrap() > 0 {}
    core.finish_cleanup(resources).unwrap();
    assert_eq!(resources.usage().free_pages, 256);
    assert_eq!(resources.usage().reserved_pages, 0);
}

#[shoop_test]
fn continuous_ticks_share_full_packets_preserve_snapshots_and_exit_without_suffix_repair() {
    let mut resources = resources();
    let mut core = Core::prepare(&mut resources, Quantum::new(Q).unwrap(), config()).unwrap();
    let mut staged = Staging::prepare(8).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    let mut source = MidiState::default();
    let mut initial = MidiState::default();
    initial.apply_packet(&[0xb0, 7, 42], 64).unwrap();
    let mut sysex = [37; 35];
    sysex[0] = 0xf0;
    sysex[34] = 0xf7;
    let rows = [
        original(0, 0, &[0x90, 60, 90]),
        original(10, 1, &[0xb0, 66, 127]),
        original(32, 2, &[0x80, 60, 0]),
        original(96, 3, &sysex),
    ];
    let mut loaded = core
        .empty(Frame(u64::from(u32::MAX) + 100), 128, &initial)
        .unwrap();
    let old_oracle = Oracle::new(&rows, &initial, 128);
    let mut a_oracle = Oracle::new(&rows, &initial, 128);
    let mut b_oracle = Oracle::new(&rows, &initial, 128);
    let mut growing_oracle = Oracle::new(&rows, &initial, 128);
    let mut middle_oracle = Oracle::new(&[], &initial, 128);
    assert_no_heap(|| {
        for row in rows {
            core.import_packet(
                &mut resources.render_io(),
                &mut loaded,
                row.frame,
                row.order,
                &row.bytes[..row.len],
            )
            .unwrap();
        }
        let old = core.retain(&loaded).unwrap();
        let second = core.retain(&loaded).unwrap();
        let prefix = core.retain(&loaded).unwrap();
        let mut a = admit(&mut core, &mut resources, loaded, Kind::Fixed, 0);
        let mut b = admit(&mut core, &mut resources, second, Kind::Fixed, 64);
        let mut growing = Some(admit(
            &mut core,
            &mut resources,
            prefix,
            Kind::Growing {
                maximum_frames: 512,
            },
            128,
        ));
        let mut continued = None;
        let mut middle = None;
        for tick in 0..48 {
            input.clear();
            if tick % 3 != 1 {
                input
                    .push(0, Q, &[0xb0, 64, if tick % 2 == 0 { 127 } else { 0 }])
                    .unwrap();
                input
                    .push(7, Q, &[0x90, 60 + tick as u8 % 4, 80 + tick as u8])
                    .unwrap();
                input.push(7, Q, &[0x80, 60 + tick as u8 % 4, 1]).unwrap();
                if tick % 4 == 0 {
                    input.push(31, Q, &sysex).unwrap();
                }
            }
            let frame = Frame((tick * Q) as u64);
            core.stage(&mut resources.render_io(), &mut staged, frame, &input)
                .unwrap();
            let bodies = core.packets.available_bodies();
            a_oracle.write(a.position, true, &source, &input);
            b_oracle.write(b.position, true, &source, &input);
            core.write_tick(&mut resources.render_io(), &mut a, &source, &staged)
                .unwrap();
            core.write_tick(&mut resources.render_io(), &mut b, &source, &staged)
                .unwrap();
            if let Some(writer) = &mut growing {
                growing_oracle.write(writer.position, false, &source, &input);
                core.write_tick(&mut resources.render_io(), writer, &source, &staged)
                    .unwrap();
            }
            assert_eq!(core.packets.available_bodies(), bodies);
            for i in 0..input.len() {
                source.apply_packet(input.get(i).unwrap().1, 64).unwrap();
            }
            core.clear_stage(&mut staged).unwrap();
            if tick == 11 {
                continued = Some(core.finish_writer(growing.take().unwrap()));
            }
            if tick == 7 {
                middle = Some(core.snapshot(&a).unwrap());
                middle_oracle.rows.extend_from_slice(&a_oracle.rows);
                middle_oracle.initial = a_oracle.initial;
            }
            if tick % 12 == 0 || tick == 47 {
                check(&mut core, &mut resources, &a.media, &a_oracle);
                check(&mut core, &mut resources, &b.media, &b_oracle);
                check(&mut core, &mut resources, &old, &old_oracle);
                if let Some(media) = &middle {
                    check(&mut core, &mut resources, media, &middle_oracle);
                }
                if let Some(media) = &continued {
                    check(&mut core, &mut resources, media, &growing_oracle);
                }
            }
            assert!(core.maintain(&mut resources.render_io(), 512).unwrap() <= 512);
        }
        assert_eq!(a.media.revision, Revision(48));
        assert_eq!(b.media.revision, Revision(48));
        let a = core.finish_writer(a);
        let b = core.finish_writer(b);
        check(&mut core, &mut resources, &a, &a_oracle);
        check(&mut core, &mut resources, &b, &b_oracle);
        core.release(a).unwrap();
        core.release(b).unwrap();
        core.release(old).unwrap();
        core.release(middle.take().unwrap()).unwrap();
        core.release(continued.take().unwrap()).unwrap();
        cleanup(&mut core, &mut resources);
    });
}

#[shoop_test]
fn refused_tick_keeps_the_previous_complete_cut_and_stop_needs_no_write_workspace() {
    let mut resources = resources();
    let mut config = config();
    config.indexes.scalar_nodes = 1;
    let mut core = Core::prepare(&mut resources, Quantum::new(Q).unwrap(), config).unwrap();
    let mut staged = Staging::prepare(8).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    let initial = MidiState::default();
    let mut media = core.empty(Frame(0), 64, &initial).unwrap();
    let oracle = Oracle::new(&[original(0, 0, &[0x90, 60, 90])], &initial, 64);
    assert_no_heap(|| {
        core.import_packet(
            &mut resources.render_io(),
            &mut media,
            0,
            0,
            &[0x90, 60, 90],
        )
        .unwrap();
        let old = core.retain(&media).unwrap();
        let mut writer = admit(&mut core, &mut resources, media, Kind::Fixed, 32);
        input.push(0, Q, &[0xb0, 121, 0]).unwrap();
        core.stage(&mut resources.render_io(), &mut staged, Frame(100), &input)
            .unwrap();
        assert_eq!(
            core.write_tick(&mut resources.render_io(), &mut writer, &initial, &staged),
            Err(Error::Capacity)
        );
        assert_eq!(writer.position, 32);
        assert_eq!(writer.media.revision, Revision(0));
        core.clear_stage(&mut staged).unwrap();
        let stopped = core.finish_writer(writer);
        check(&mut core, &mut resources, &stopped, &oracle);
        check(&mut core, &mut resources, &old, &oracle);
        core.release(stopped).unwrap();
        core.release(old).unwrap();
        cleanup(&mut core, &mut resources);
    });
}
