const assert = (condition, message) => { if (!condition) throw new Error(`graph payload check: ${message}`); };

export function checkGraphPayloads(wasm, Receiver) {
    let rejected = false;
    try { new Receiver(0n); } catch (error) { rejected = error === 6; }
    assert(rejected, "zero session constructor");
    const receiver = new Receiver(0xffffffffffffffffn);
    const memoryBytes = wasm.memory.buffer.byteLength;
    let sequence = 0x100000000n;
    function packet(kind, slot = 0, generation = 1, fill = () => {}) {
        const bytes = new Uint8Array(96), data = new DataView(bytes.buffer);
        bytes.set([76, 71, 80, 49, 1, 0, kind]);
        data.setBigUint64(8, 0xffffffffffffffffn, true);
        data.setBigUint64(16, sequence + 1n, true);
        data.setUint32(24, slot, true); data.setUint32(28, generation, true);
        fill(data);
        return bytes;
    }
    function send(bytes, expected = 0) {
        const before = receiver.last_sequence();
        new Uint8Array(wasm.memory.buffer, receiver.record_ptr(), 96).set(bytes);
        assert(receiver.ingest() === expected, `ingest expected ${expected}`);
        if (expected === 0) sequence = new DataView(bytes.buffer).getBigUint64(16, true);
        assert(receiver.last_sequence() === (expected === 0 ? sequence : before), "refusal changed sequence watermark");
    }
    const begin = packet(1, 0, 1, data => {
        data.setBigUint64(32, 0xffffffffffffffffn, true);
        for (const at of [40, 44, 48]) data.setUint32(at, 1, true);
        data.setUint8(52, 1);
    });
    send(begin);
    assert(receiver.phase(0, 1) === 1 && receiver.generation(0) === 1, "receiving identity");
    const edge = () => packet(3, 0, 1, data => {
        // Audio host port 0 -> node 0 generation 1, port 0.
        data.setUint32(56, 1, true);
    });
    send(edge(), 17); // Edges cannot precede node steps.
    for (const at of [7, 53, 95]) {
        const bad = packet(6); bad[at] = 1; send(bad, 4);
    }
    const step = () => packet(2, 0, 1, data => {
        data.setUint32(40, 1, true); data.setBigUint64(44, 1n, true);
    });
    const overwide = step(); new DataView(overwide.buffer).setBigUint64(44, 0x100000000n, true);
    send(overwide, 2); // Never truncate u64 wire indices to Wasm32 usize.
    send(step());
    const invalidKind = edge(); invalidKind[44] = 2; send(invalidKind, 13);
    const invalidHost = edge(); invalidHost[36] = 1; send(invalidHost, 6);
    const padding = edge(); padding[45] = 1; send(padding, 4);
    send(edge());
    const planning = () => packet(4, 0, 1, data => {
        data.setUint32(36, 1, true); data.setBigUint64(40, 1n, true); data.setBigUint64(48, 2n, true);
    });
    for (const at of [40, 48, 56, 64]) {
        const bytes = planning(); new DataView(bytes.buffer).setBigUint64(at, 0x100000000n, true); send(bytes, 2);
    }
    send(planning());
    send(planning(), 17);
    const binding = () => packet(5, 0, 1, data => { data.setUint32(40, 1, true); data.setUint32(48, 1, true); });
    const hugeSlot = binding(); new DataView(hugeSlot.buffer).setBigUint64(52, 0x100000000n, true); send(hugeSlot, 2);
    send(binding()); send(packet(6));
    assert(receiver.phase(0, 1) === 2 && receiver.take(0, 1) === 0 && receiver.phase(0, 1) === 3, "sealed/local loan phases");
    assert(receiver.loan_epoch(0) === 0xffffffffffffffffn, "full-width epoch lost");
    send(packet(7), 15);
    assert(receiver.recycle(0, 2) === 6 && receiver.loan_epoch(0) === 0xffffffffffffffffn, "foreign return destroyed owner");
    assert(receiver.recycle(0, 1) === 0 && receiver.generation(0) === 2 && receiver.phase(0, 2) === 0, "local owning return");
    send(packet(7), 6); // Old generation.
    const foreign = packet(1, 0, 2); foreign[8] ^= 128; send(foreign, 6);
    const duplicate = packet(1, 0, 2); new DataView(duplicate.buffer).setBigUint64(16, sequence, true); send(duplicate, 10);
    const checks = ["graph payload canonical records, full-width fields and Wasm32 overflow refusal"];
    for (let round = 0; round < 128; round++) {
        const slot = round % 2, generation = receiver.generation(slot);
        send(packet(1, slot, generation));
        send(packet(6, slot, generation));
        assert(receiver.take(slot, generation) === 0 && receiver.take(slot, generation) === 15, "duplicate local loan");
        assert(receiver.recycle(slot, generation) === 0 && receiver.recycle(slot, generation) === 6, "duplicate owning return");
        assert(receiver.generation(slot) === generation + 1, "generation reuse");
    }
    assert(receiver.phase(0, 1) === (0x80000000 | 6) >>> 0, "stale phase query");
    assert(wasm.memory.buffer.byteLength === memoryBytes, "prepared graph payload memory grew");
    receiver.free();
    checks.push("graph payload 128 bounded local owner exchanges with fixed memory");
    return checks;
}
