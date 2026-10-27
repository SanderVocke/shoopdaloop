const assert = (condition, message) => { if (!condition) throw new Error(`binding check: ${message}`); };
export function checkBindings(wasm, Client, Bridge) {
    const checks = [];
    let rejected = false;
    try { new Client(0n); } catch (error) { rejected = error === 9; }
    assert(rejected, "zero session constructor");
    rejected = false;
    try { new Bridge(1n, 3); } catch (error) { rejected = error === 5; }
    assert(rejected, "unknown fault mode constructor");
    checks.push("constructor refusal");
    const credit = new Client(0xffffffffffffffffn);
    assert(credit.prepare(3, 0n, 0n, 8) === 4 && credit.outstanding() === 0, "invalid flags consumed credit");
    for (let index = 0; index < 4; index++) {
        const frame = 0xffffffffffffffffn - BigInt(index);
        assert(credit.prepare(3, frame, 0n, 0) === 0, "normal credit");
        const wire = new DataView(wasm.memory.buffer, credit.request_ptr(), 64);
        assert(wire.getBigUint64(8, true) === 0xffffffffffffffffn && wire.getBigUint64(24, true) === frame, "lost full-width session/frame");
        assert(wire.getBigUint64(16, true) === BigInt(index + 1) && wire.getUint32(32, true) === index, "sequence/slot");
    }
    assert(credit.prepare(3, 0n, 0n, 0) === 10, "normal pressure");
    assert(credit.prepare(3, 0n, 0n, 4) === 0 && credit.prepare(3, 0n, 0n, 4) === 10, "independent recovery credit");
    new Uint8Array(wasm.memory.buffer, credit.response_ptr(), 96).fill(0);
    assert(credit.receive() === 2 && credit.outstanding() === 5, "malformed response released credits");
    credit.free(); checks.push("full-width codec and bounded independent credits");

    function submit(client, bridge, kind, at) {
        assert(client.prepare(kind, at, 0n, 0) === 0, "prepare");
        new Uint8Array(wasm.memory.buffer, bridge.request_ptr(), 64).set(new Uint8Array(wasm.memory.buffer, client.request_ptr(), 64));
        assert(bridge.ingest() === 0, "ingest");
    }
    function drain(client, bridge) {
        let count = 0;
        for (;;) {
            const status = bridge.poll_response();
            if (status === 0) break;
            assert(status === 1, "poll error");
            const packet = new Uint8Array(wasm.memory.buffer, bridge.response_ptr(), 96);
            const target = new Uint8Array(wasm.memory.buffer, client.response_ptr(), 96);
            target.set(packet);
            if (count === 0) {
                target[8] ^= 128;
                assert(client.receive() === 9, "foreign response accepted");
                target.set(packet);
            }
            assert(client.receive() === 0, "valid receipt");
            assert(client.receive() === 11, "duplicate receipt accepted");
            count++;
        }
        assert(client.outstanding() === 0, "unreleased credits");
        return count;
    }
    function recovered(bridge) {
        assert(bridge.stop() === 0, "stopped cleanup");
        assert([bridge.recovered_pages(), bridge.recovered_versions(), bridge.recovered_snapshots(), bridge.media_references()].join() === "256,16,16,0", "pool recovery");
    }
    const client = new Client(91n), bridge = new Bridge(91n, 0);
    const memoryBytes = wasm.memory.buffer.byteLength;
    for (const [kind, at] of [[1, 0n], [2, 4n], [3, 12n]]) submit(client, bridge, kind, at);
    assert(bridge.render(0) === 0 && bridge.frame() === 0n && bridge.callbacks() === 0n, "zero callback advanced");
    assert(bridge.render(128) === 0, "actual engine render");
    const expected = [1, 2, 3, 4, .5, 1, 1.5, 2, .5, 1, 1.5, 2];
    for (let channel = 0; channel < 2; channel++) {
        const audio = new Float32Array(wasm.memory.buffer, bridge.output_ptr(channel), 128);
        for (let frame = 0; frame < 128; frame++) assert(audio[frame] === (expected[frame] ?? 0) * (channel ? 10 : 1), "stereo scalar output");
    }
    assert(bridge.journal_len() === 134 * 15 && bridge.stop() === 15, "journal size/retained results");
    assert(drain(client, bridge) === 6, "missing admission/execution receipts");
    checks.push("zero callback, stereo rendering and owning receipt validation");
    for (let round = 0; round < 10; round++) {
        const at = bridge.frame();
        submit(client, bridge, 2, at); submit(client, bridge, 3, at + 12n);
        assert(bridge.render(128) === 0 && drain(client, bridge) === 4, "repeated playback");
        const audio = new Float32Array(wasm.memory.buffer, bridge.output_ptr(0), 128);
        for (let frame = 0; frame < 128; frame++) assert(audio[frame] === (frame < 12 ? (frame % 4 + 1) * .5 : 0), "diagnostic pressure changed samples");
    }
    assert(bridge.journal_overflow() > 0 && bridge.heap_allocations() === 0 && bridge.heap_reallocations() === 0 && bridge.heap_deallocations() === 0, "journal/heap witnesses");
    assert(wasm.memory.buffer.byteLength === memoryBytes, "prepared memory grew");
    recovered(bridge); bridge.free(); client.free();
    checks.push("fixed memory and diagnostic exhaustion without changing music");
    checks.push("successful stop and complete resource recovery");
    for (const mode of [1, 2]) {
        const client = new Client(93n), bridge = new Bridge(93n, mode);
        submit(client, bridge, 1, 0n);
        assert(bridge.render(128) === (mode === 1 ? 10 : 0xffff0001) && bridge.fault_witness() === mode, "wrong failure origin");
        if (mode === 2) assert(bridge.heap_allocations() > 0 && bridge.heap_reallocations() > 0 && bridge.heap_deallocations() > 0, "ineffective heap instrumentation");
        assert(bridge.render(128) === 10, "faulted renderer resumed");
        recovered(bridge); bridge.free(); client.free();
        checks.push(mode === 1 ? "actual processor failure witness" : "actual allocation/reallocation/deallocation witnesses");
    }
    return { passed: true, checks };
}
