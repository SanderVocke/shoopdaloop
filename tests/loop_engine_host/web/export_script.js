const assert = (ok, message) => { if (!ok) throw new Error(`export script: ${message}`); };
const view = bytes => new DataView(Uint8Array.from(bytes).buffer);
export function acceptExport(io, packet) {
    const target = new Uint8Array(io.memory.buffer, io.exportClient.response_ptr(), 1024);
    const before = io.exportClient.outstanding();
    target.set(packet); target[16] ^= 128;
    assert(io.exportClient.receive() === 9 && io.exportClient.outstanding() === before, "foreign result consumed credit");
    target.set(packet);
    assert(io.exportClient.receive() === 0 && io.exportClient.receive() === 11, "valid/duplicate result");
    io.exportResponses.push(Array.from(packet));
}
export async function exerciseExports(io) {
    for (const bank of [0, 1]) {
        const epoch = bank * 128;
        await io.music(epoch);
        await io.command(1, bank, BigInt(epoch + 8));
        await io.step(); await io.drain();
        assert(io.exportClient.outstanding() === 0 && io.exportClient.retained() === bank + 1, "retained coherent export");
        await io.probeStop();
        const packet = io.exportResponses.at(-1), data = view(packet);
        assert(data.getUint32(12, true) === 2 && data.getBigUint64(104, true) === BigInt(epoch + 8), "settled export cut");
        assert(data.getBigUint64(136, true) === BigInt(bank + 1) && data.getBigUint64(144, true) === 4n && data.getUint32(152, true) === 2, "content metadata");
        assert(data.getBigUint64(176, true) === BigInt(epoch + 4) && data.getBigUint64(184, true) === BigInt(epoch + 4), "original snapshot cuts");
        assert(data.getBigUint64(192, true) === BigInt(epoch) && data.getBigUint64(200, true) === BigInt(epoch + 4) && data.getBigUint64(208, true) === BigInt(epoch) && data.getBigUint64(216, true) === BigInt(epoch + 4) && packet[157] === 0, "coherent complete capture stamps");
    }
    for (const bank of [0, 1]) {
        for (const kind of [2, 4, 3, 5]) {
            await io.command(kind, bank, kind === 4 || kind === 5 ? 0n : io.frame);
            await io.step(); await io.drain();
            const packet = io.exportResponses.at(-1), data = view(packet);
            assert(data.getUint32(12, true) === 2, "cursor/read domain outcome");
            if (kind === 4) {
                const expected = new Uint8Array(32), floats = new DataView(expected.buffer);
                for (let i = 0; i < 4; i++) for (let channel = 0; channel < 2; channel++) floats.setFloat32((i * 2 + channel) * 4, (i + 1) * (bank ? 10 : 1) * (channel ? 10 : 1), true);
                assert(data.getUint32(100, true) === 32 && packet.slice(256, 288).join() === expected.join() && (data.getUint32(8, true) & 4), "immutable audio/read EOF");
            } else if (kind === 5) {
                const expected = new Uint8Array(bank ? 0 : 30), midi = new DataView(expected.buffer);
                if (!bank) for (const [i, at, bytes] of [[0,0,[0x90,60,100]], [1,2,[0x80,60,45]]]) {
                    midi.setBigUint64(i * 15, BigInt(at), true); midi.setUint32(i * 15 + 8, 3, true); expected.set(bytes, i * 15 + 12);
                }
                assert(data.getUint32(100, true) === expected.length && packet.slice(256, 256 + expected.length).join() === expected.join() && (data.getUint32(8, true) & 4), "immutable MIDI/read EOF");
            }
        }
        for (let channel = 0; channel < 16; channel++) {
            await io.command(6, bank, io.frame, channel);
            await io.step(); await io.drain();
            const packet = io.exportResponses.at(-1), data = view(packet), expected = new Uint8Array(656);
            expected[0] = channel; expected.fill(255, 2, 6); new DataView(expected.buffer).setUint16(10, 8192, true);
            for (let key = 0; key < 128; key++) { expected[16 + key * 4 + 2] = 255; expected[528 + key] = [64,66,69].includes(key) ? 0 : 255; }
            assert(data.getUint32(12, true) === 2 && data.getUint32(100, true) === 656 && packet.slice(256, 912).join() === expected.join(), "complete initial projection on every channel");
        }
    }
    for (const bank of [0, 1]) {
        await io.command(7, bank, io.frame); await io.step(); await io.drain();
        assert(io.exportClient.generation(bank) === 2 && io.exportClient.retained() === 1 - bank, "export release/generation");
    }
    assert(io.exportClient.outstanding() === 0 && io.exportClient.retained() === 0, "all export credits/owners returned");
}
export async function exportBindings(wasm, Client, Bridge, ExportClient) {
    const client = new Client(0xffffffffffffffffn), exportClient = new ExportClient(0xffffffffffffffffn), engine = Bridge.new_exports(0xffffffffffffffffn);
    const io = { memory: wasm.memory, exportClient, frame: 0n, exportResponses: [], exportRequests: [], responses: [], samples: [], steps: 0,
        async music(epoch) { for (const [kind, at] of [[1,epoch],[2,epoch+4],[3,epoch+12]]) {
            assert(client.prepare(kind, BigInt(at), 0n, 0) === 0, "binding transition preparation");
            new Uint8Array(wasm.memory.buffer, engine.request_ptr(), 64).set(new Uint8Array(wasm.memory.buffer, client.request_ptr(), 64)); assert(engine.ingest() === 0, "binding transition ingress");
        } },
        async command(kind, bank, at, channel = 0) {
            assert(exportClient.prepare(kind, bank, 1, at, 0n, channel, 0, false) === 0, "binding export preparation");
            const packet = new Uint8Array(wasm.memory.buffer, exportClient.request_ptr(), 96), target = new Uint8Array(wasm.memory.buffer, engine.export_request_ptr(), 96);
            if (!this.exportRequests.length) { target.set(packet); target[95] = 1; assert(engine.ingest_export() === 4, "binding malformed ingress"); target.set(packet); target[8] ^= 128; assert(engine.ingest_export() === 9, "binding foreign ingress"); }
            target.set(packet); assert(engine.ingest_export() === 0, "binding export ingress"); this.exportRequests.push(Array.from(packet));
        },
        async step() {
            assert(engine.render(128) === 0, "binding render"); this.frame = engine.frame(); this.steps++;
            if (this.steps <= 2) { const left = new Float32Array(wasm.memory.buffer, engine.output_ptr(0), 128), right = new Float32Array(wasm.memory.buffer, engine.output_ptr(1), 128); for (let i = 0; i < 128; i++) this.samples.push([left[i], right[i]]); }
        },
        async drain() {
            for (let i = 0; i < 10; i++) { const code = engine.poll_response(); if (!code) break; assert(code === 1, "binding primitive poll"); const bytes = new Uint8Array(wasm.memory.buffer, engine.response_ptr(), 96); new Uint8Array(wasm.memory.buffer, client.response_ptr(), 96).set(bytes); assert(client.receive() === 0, "binding primitive receipt"); this.responses.push(Array.from(bytes)); }
            for (let i = 0; i < 2; i++) { const code = engine.poll_export_response(); if (!code) break; assert(code === 1, "binding export poll"); acceptExport(this, new Uint8Array(wasm.memory.buffer, engine.export_response_ptr(), 1024)); }
            assert(engine.poll_export_response() === 0, "binding response bound");
        },
        async probeStop() { assert(engine.stop() === 15, "retained export allowed stop"); },
    };
    const storage = [engine.export_resource_bytes(), engine.export_read_bytes(),96,1024,2,1];
    const bytes = wasm.memory.buffer.byteLength;
    await exerciseExports(io);
    assert(engine.frame() === 5632n && engine.callbacks() === 44n && engine.outstanding() === 0 && engine.retained_exports() === 0 && client.outstanding() === 0, "binding final clock/ownership");
    const midiBytes = Array.from(new Uint8Array(wasm.memory.buffer, engine.journal_ptr(), engine.journal_len()));
    assert(engine.stop() === 0 && wasm.memory.buffer.byteLength === bytes && [engine.heap_allocations(), engine.heap_reallocations(), engine.heap_deallocations()].join() === "0,0,0", "binding fixed memory/heap/stop");
    const result = { samples: io.samples, exportRequests: io.exportRequests, exportResponses: io.exportResponses, responses: io.responses, midiBytes,
        semanticBytes: Array.from(new Uint8Array(wasm.memory.buffer, engine.semantic_ptr(), engine.semantic_len())), recovered: [engine.recovered_pages(), engine.recovered_versions(), engine.recovered_snapshots(), engine.media_references()], counts: [client.outstanding(), exportClient.outstanding(), exportClient.retained(), engine.retained_exports()], storage, frames: Number(engine.frame()), callbacks: Number(engine.callbacks()) };
    engine.free(); exportClient.free(); client.free(); return result;
}
