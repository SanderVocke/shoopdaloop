const assert = (condition, message) => { if (!condition) throw new Error(`graph command check: ${message}`); };
export function checkGraphCommands(wasm, GraphClient, Client, Bridge) {
    const checks = [];
    let rejected = false;
    try { new GraphClient(0n); } catch (error) { rejected = error === 9; }
    assert(rejected, "zero session");
    const wire = new GraphClient(0xffffffffffffffffn);
    assert(wire.prepare_payload(0xffffffff, 0, 1, 1n, 1n) === 7, "payload index");
    assert(wire.prepare_payload(0, 0, 1, 0xffffffffffffffffn, 0xffffffffffffffffn) === 0, "full-width payload");
    let data = new DataView(wasm.memory.buffer, wire.payload_ptr(), 96);
    assert([8, 16, 32].every(at => data.getBigUint64(at, true) === 0xffffffffffffffffn), "payload high bits");
    assert(wire.stage(0, 0, 0, 1n, false) === 6 && wire.outstanding() === 0, "invalid bank consumed credit");
    assert(wire.activate(0xffffffff, 0xffffffff, 0xffffffff, 0xffffffffffffffffn, 0xffffffffffffffffn, 0xffffffffffffffffn, false) === 0, "full-width command");
    data = new DataView(wasm.memory.buffer, wire.request_ptr(), 96);
    assert([8, 40, 48, 56].every(at => data.getBigUint64(at, true) === 0xffffffffffffffffn), "command high bits");
    assert(wire.stage(0, 1, 0, 1n, false) === 0 && wire.stage(1, 1, 1, 1n, false) === 10 && wire.outstanding() === 2, "bounded graph credits");
    new Uint8Array(wasm.memory.buffer, wire.response_ptr(), 128).fill(0);
    assert(wire.receive() === 2 && wire.outstanding() === 2, "malformed reply consumed credit");
    wire.free(); checks.push("graph command public bindings preserve full-width fields and bounded credits");
    const client = new Client(0xffffffffffffffffn), graph = new GraphClient(0xffffffffffffffffn), bridge = Bridge.new_graphs(0xffffffffffffffffn, 0);
    const bytes = wasm.memory.buffer.byteLength;
    const copy = (source, destination, size) => new Uint8Array(wasm.memory.buffer, destination, size).set(new Uint8Array(wasm.memory.buffer, source, size));
    const responses = [];
    function command() { copy(graph.request_ptr(), bridge.graph_request_ptr(), 96); assert(bridge.ingest_graph() === 0, "graph ingress"); }
    function drain(save = true) {
        const records = [];
        for (let count = 0; count < 4; count++) {
            const result = bridge.poll_graph_response();
            if (result === 0) break;
            assert(result === 1, "graph polling");
            const record = Array.from(new Uint8Array(wasm.memory.buffer, bridge.graph_response_ptr(), 128));
            copy(bridge.graph_response_ptr(), graph.response_ptr(), 128);
            const target = new Uint8Array(wasm.memory.buffer, graph.response_ptr(), 128);
            target[8] ^= 128;
            assert(graph.receive() === 9, "foreign graph reply");
            target.set(record);
            assert(graph.receive() === 0 && graph.receive() === 11, "graph receipt or duplicate");
            records.push(new DataView(Uint8Array.from(record).buffer));
            if (save) responses.push(record);
        }
        return records;
    }
    let sequence = 0n;
    function upload(generation, epoch) {
        for (let index = 0; index < graph.payload_count(); index++) {
            assert(graph.prepare_payload(index, 0, generation, epoch, ++sequence) === 0, "payload prepare");
            copy(graph.payload_ptr(), bridge.graph_payload_ptr(), 96);
            assert(bridge.ingest_graph_payload() === 0, "payload materialize");
        }
    }
    function render(frames) {
        assert(bridge.render(frames) === 0, "graph render");
        assert([bridge.heap_allocations(), bridge.heap_reallocations(), bridge.heap_deallocations()].every(n => n === 0), "graph heap witness");
        assert(wasm.memory.buffer.byteLength === bytes, "graph binding memory growth");
    }
    for (const [kind, at] of [[1,0n],[2,4n],[3,12n]]) {
        assert(client.prepare(kind, at, 0n, 0) === 0, "primitive prepare");
        copy(client.request_ptr(), bridge.request_ptr(), 64); assert(bridge.ingest() === 0, "primitive ingress");
    }
    upload(1, 1n); assert(graph.stage(0, 1, 0, 1n, false) === 0, "stage"); command();
    render(0); assert(bridge.frame() === 0n && bridge.callbacks() === 0n, "zero callback");
    render(1);
    const audio = [0,1].map(channel => [new Float32Array(wasm.memory.buffer, bridge.output_ptr(channel), 128)[0]]);
    const staged = drain(); assert(staged.length === 1 && staged[0].getUint32(28, true) === 1, "Ready");
    const id = [76,80,84].map(at => staged[0].getUint32(at, true));
    assert(graph.activate(...id, 4n, 1n, 0n, false) === 0, "activate"); command(); render(12);
    for (let channel = 0; channel < 2; channel++) audio[channel].push(...new Float32Array(wasm.memory.buffer, bridge.output_ptr(channel), 12));
    const active = drain(); assert(active.length === 2 && active[1].getBigUint64(88, true) === 2n, "publication epoch");
    let primitive = 0;
    for (let count = 0; count < 10; count++) {
        const result = bridge.poll_response(); if (result === 0) break;
        assert(result === 1, "primitive poll"); copy(bridge.response_ptr(), client.response_ptr(), 96);
        assert(client.receive() === 0, "primitive receive"); primitive++;
    }
    assert(primitive === 6 && client.outstanding() === 0 && graph.outstanding() === 0 && bridge.outstanding() === 0, "terminal credits");
    assert(bridge.stop() === 15, "Ready/Taken graph silently disposed as successful stop");
    assert(graph.release(...id, false) === 0, "release"); command(); render(1);
    const returned = drain();
    assert(returned.length === 1 && returned[0].getUint32(108, true) === 2 && [112,116,120].map(at => returned[0].getUint32(at, true)).join() === "3,12,2", "retired initial graph capacities");
    const left = [1,2,3,4,1,2,3,4,1,2,3,4,0];
    assert(audio.every((samples, channel) => samples.every((value, at) => value === left[at] * (channel ? 10 : 1))), "full graph-route samples");
    checks.push("graph command bindings render exact route cut, receipts and owning retirement");
    for (let round = 0; round < 128; round++) {
        upload(round + 2, 2n);
        assert(graph.stage(0, round + 2, 0, 2n, false) === 0, "reuse stage"); command(); render(1);
        const ready = drain(false); assert(ready.length === 1 && ready[0].getUint32(28, true) === 1, "reuse Ready");
        const nextId = [76,80,84].map(at => ready[0].getUint32(at, true));
        assert(graph.release(...nextId, false) === 0, "reuse release"); command(); render(1);
        const recycled = drain(false);
        assert(recycled.length === 1 && recycled[0].getUint32(28, true) === 2 && recycled[0].getUint32(108, true) === round + 3, "reuse owning return");
    }
    assert([graph.outstanding(), graph.retained_graphs(), bridge.outstanding(), bridge.retained_graphs()].every(n => n === 0), "outstanding graph owners/credits");
    assert(bridge.stop() === 0 && bridge.render(1) === 10, "stopped graph binding");
    const report = { audio, graphResponses: responses,
        midiBytes: Array.from(new Uint8Array(wasm.memory.buffer, bridge.journal_ptr(), bridge.journal_len())),
        semanticBytes: Array.from(new Uint8Array(wasm.memory.buffer, bridge.semantic_ptr(), bridge.semantic_len())),
        initialBytes: bytes, finalBytes: wasm.memory.buffer.byteLength, rounds: 128, generation: 130,
        recovered: [bridge.recovered_pages(), bridge.recovered_versions(), bridge.recovered_snapshots(), bridge.media_references()] };
    assert(report.recovered.join() === "256,16,16,0" && report.initialBytes === report.finalBytes && bridge.journal_overflow() === 0, "graph binding recovery");
    graph.free(); client.free(); bridge.free();
    checks.push("graph command bindings recycle 128 candidates without memory growth or callback heap work");
    return { checks, report };
}
