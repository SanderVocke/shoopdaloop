use rt_fpga_compiler::shoop_graph::*;

pub fn id(slot: u32) -> Id {
    Id {
        slot,
        generation: 1,
    }
}
pub fn port(g: &mut Graph, owner: Owner, role: PortRole, channel: u32) -> PortId {
    let id = PortId {
        owner,
        signal: SignalType::Audio,
        role,
        index: channel + 100,
    };
    g.ports.push(PortView {
        id,
        channel,
        name: "".into(),
    });
    id
}
pub fn edge(g: &mut Graph, source: PortId, destination: PortId) {
    g.connections.push(Connection {
        source,
        destination,
    });
}
pub fn session(channels: u32) -> Graph {
    let mut g = Graph::default();
    let layout = ChannelLayout::Direct {
        audio: channels,
        midi: false,
    };
    for slot in [10, 11] {
        g.strips.push(StripView {
            id: id(slot),
            name: "strip".into(),
            layout,
            controls: StripControls::default(),
            default_dry_playback: false,
            processor: None,
        });
    }
    for (slot, strip, gain) in [(100, 10, 0.5), (101, 10, 0.25), (102, 11, 1.0)] {
        g.loops.push(LoopView {
            id: id(slot),
            strip: Some(id(strip)),
            layout,
            gain,
            balance: 0.,
            boundary_smoothing_frames: 0,
            sync_source: None,
            repeat_sync: false,
            composite: None,
        });
    }
    g.strips[1].controls.input_monitoring = true;
    for slot in [20, 21] {
        g.buses.push(BusView {
            id: id(slot),
            name: "bus".into(),
            channels,
            master: slot == 21,
            gain_db: 0.,
            balance: 0.,
            muted: false,
            processor: None,
        });
    }
    for ch in 0..channels {
        let host_in = port(&mut g, Owner::Host, PortRole::Input, ch);
        let host_out = port(&mut g, Owner::Host, PortRole::Output, ch);
        let a_in = port(&mut g, Owner::Strip(id(10)), PortRole::Input, ch);
        let a_out = port(&mut g, Owner::Strip(id(10)), PortRole::Output, ch);
        let b_in = port(&mut g, Owner::Strip(id(11)), PortRole::Input, ch);
        let b_out = port(&mut g, Owner::Strip(id(11)), PortRole::Output, ch);
        let aux_in = port(&mut g, Owner::Bus(id(20)), PortRole::Input, ch);
        let aux_out = port(&mut g, Owner::Bus(id(20)), PortRole::Output, ch);
        let master_in = port(&mut g, Owner::Bus(id(21)), PortRole::Input, ch);
        let master_out = port(&mut g, Owner::Bus(id(21)), PortRole::Output, ch);
        for (source, destination) in [
            (host_in, a_in),
            (host_in, b_in),
            (a_out, aux_in),
            (aux_out, master_in),
            (b_out, master_in),
            (master_out, host_out),
        ] {
            edge(&mut g, source, destination);
        }
    }
    g
}
