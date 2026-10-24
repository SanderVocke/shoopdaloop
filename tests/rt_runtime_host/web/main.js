import { initSync, Controller } from "./pkg/fixture.js";

const Q = 128, RATE = 48000, LIMIT = 4096, HEADER = 48;
const OP = { begin: 1, steps: 2, edges: 3, graph: 4, cancelPlan: 5, parameter: 6, midi: 7, execution: 8, status: 9,
    create: 10, upload: 11, write: 12, finish: 13, commit: 14, cancelUpload: 15, snapshot: 16, cursor: 17, read: 18,
    closeCursor: 19, release: 20, currentness: 21, info: 22, capture: 23, stopCapture: 24, removeResource: 25, removeNode: 26, reset: 27 };
const assert = (condition, message = "fixture assertion") => { if (!condition) throw new Error(message); };
const view = bytes => new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
const u32 = (bytes, at = 0) => view(bytes).getUint32(at, true);
const u64 = (bytes, at = 0) => Number(view(bytes).getBigUint64(at, true));
function body(length, fill = () => {}) { const bytes = new Uint8Array(length); fill(view(bytes), bytes); return bytes; }
const handle = bytes => [u32(bytes), u32(bytes, 4)];
const identity = id => body(8, v => { v.setUint32(0, id[0], true); v.setUint32(4, id[1], true); });
function hashBytes(hash, bytes) { for (const byte of bytes) hash = Math.imul(hash ^ byte, 16777619) >>> 0; return hash; }
function hashSamples(samples, start = 0, end = samples.length, hash = 2166136261) {
    const bits = new Uint32Array(samples.buffer, samples.byteOffset, samples.length);
    for (let i = start; i < end; i++) hash = Math.imul(hash ^ bits[i], 16777619) >>> 0;
    return hash;
}
function decode(buffer, length) {
    assert(length >= 56 && length <= 512, "response length");
    const bytes = new Uint8Array(buffer, 0, length), v = view(bytes);
    assert(v.getUint32(40, true) === length - HEADER, "response body length");
    return { op: bytes[6], sequence: u64(bytes, 8), epoch: u64(bytes, 24), frame: u64(bytes, 32),
        status: u32(bytes, 48), error: u32(bytes, 52), data: bytes.slice(56), raw: bytes.slice() };
}
class Host {
    constructor(mode, module, stream) {
        this.mode = mode; this.stream = stream; this.module = module; this.sequence = 0; this.epoch = 0; this.nextFrame = 0;
        this.pool = Array.from({ length: 5 }, () => new ArrayBuffer(512)); this.waiters = new Map(); this.journal = [];
        this.reject = null; this.failure = new Promise((_, reject) => { this.reject = reject; });
        this.failure.catch(() => {});
    }
    wait(key) {
        assert(!this.waiters.has(key), `duplicate waiter ${key}`);
        return Promise.race([new Promise(resolve => this.waiters.set(key, resolve)), this.failure]);
    }
    async start() {
        this.context = this.mode === "offline" ? new OfflineAudioContext(1, LIMIT * Q, RATE) : new AudioContext({ sampleRate: RATE });
        await this.context.audioWorklet.addModule("./worklet.js");
        const ready = this.wait("ready");
        this.node = new AudioWorkletNode(this.context, "rt-runtime-fixture", { numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [1], processorOptions: { module: this.module, stream: this.stream } });
        this.node.onprocessorerror = () => this.reject(new Error("AudioWorklet processorerror"));
        this.node.port.onmessage = ({ data }) => {
            if (data.type === "fatal") { this.reject(new Error(data.error)); return; }
            const key = data.slot === undefined ? data.type : `${data.type}:${data.slot}`;
            const resolve = this.waiters.get(key);
            if (!resolve) { this.reject(new Error(`unsolicited ${key}`)); return; }
            this.waiters.delete(key); resolve(data);
        };
        if (this.mode === "offline") {
            this.node.connect(this.context.destination);
            const pause = this.context.suspend(Q / RATE);
            this.finished = this.context.startRendering();
            await Promise.race([pause, this.failure]);
            this.journal.push({ inputs: [], outputs: [] }); this.nextFrame = Q;
        } else {
            this.analyser = this.context.createAnalyser(); this.analyser.fftSize = 256;
            const mute = this.context.createGain(); mute.gain.value = 0;
            this.node.connect(this.analyser).connect(mute).connect(this.context.destination);
            await this.context.resume();
        }
        this.ready = await ready;
        assert(this.ready.marker === 0 && this.ready.shared === false, "worklet must have independent, non-shared instance state");
    }
    packet(op, data, slot, options = {}) {
        const bytes = new Uint8Array(HEADER + data.length); const v = view(bytes);
        bytes.set([82, 84, 82, 49]); v.setUint16(4, 1, true); bytes[6] = op; bytes[7] = options.cancel ? 1 : 0;
        v.setBigUint64(8, BigInt(++this.sequence), true); v.setBigUint64(16, BigInt(options.stream ?? this.stream), true);
        v.setBigUint64(24, BigInt(options.epoch ?? this.epoch), true); v.setBigUint64(32, BigInt(options.frame ?? 0), true);
        v.setUint32(40, data.length, true); v.setUint16(44, slot, true); bytes.set(data, HEADER);
        return bytes;
    }
    async step(requests, options = {}) {
        const inputs = [], outputs = [], staged = [], returned = [];
        for (const request of requests) {
            const { slot, packet } = request;
            const buffer = this.pool[slot]; assert(buffer && buffer.byteLength === 512, "missing transferable credit"); this.pool[slot] = null;
            const bytes = new Uint8Array(buffer); bytes.fill(0); bytes.set(packet);
            staged.push(this.wait(`staged:${slot}`)); returned.push(this.wait(`returned:${slot}`));
            inputs.push({ slot, bytes: packet.slice() });
            this.node.port.postMessage({ type: "frame", slot, buffer, length: packet.length }, [buffer]);
            assert(buffer.byteLength === 0, "outbound buffer did not transfer");
        }
        await Promise.all(staged);
        if (options.duplicate) {
            const buffer = new ArrayBuffer(512); new Uint8Array(buffer).set(inputs[0].bytes);
            const refused = this.wait("refused:0");
            this.node.port.postMessage({ type: "frame", slot: 0, length: inputs[0].bytes.length, buffer }, [buffer]);
            assert((await refused).buffer.byteLength === 512, "duplicate credit lost its buffer");
            const oversized = new ArrayBuffer(513), rejected = this.wait("refused:0");
            this.node.port.postMessage({ type: "frame", slot: 0, length: 513, buffer: oversized }, [oversized]);
            const rejection = await rejected;
            assert(rejection.buffer.byteLength === 513 && rejection.error === "envelope", "oversized frame lost ownership or reached payload decoding");
        }
        if (this.mode === "offline") {
            assert(this.journal.length + 1 < LIMIT, "offline quantum limit");
            const pause = this.context.suspend((this.journal.length + 1) * Q / RATE);
            await this.context.resume(); await Promise.race([pause, this.failure]);
        }
        if (options.resumeLive) await this.context.resume();
        const messages = await Promise.all(returned);
        const replies = messages.map((message, index) => {
            assert(message.buffer.byteLength === 512, "returned buffer lost ownership");
            const reply = decode(message.buffer, message.length), expected = inputs[index];
            assert(reply.sequence === u64(expected.bytes, 8) && reply.op === (expected.bytes[6] | 128), "response correlation");
            if (this.mode === "offline") assert(reply.frame === this.journal.length * Q, `unexpected callback timing ${reply.frame} != ${this.journal.length * Q}`);
            this.nextFrame = Math.max(this.nextFrame, reply.frame + Q); this.epoch = Math.max(this.epoch, reply.epoch);
            if (options.hold) options.hold.push(message); else this.pool[message.slot] = message.buffer;
            outputs.push({ slot: message.slot, bytes: reply.raw }); return reply;
        });
        if (this.mode === "offline") { this.journal.push({ inputs, outputs }); this.nextFrame = this.journal.length * Q; }
        return replies;
    }
    async request(op, data = new Uint8Array(), options = {}) {
        const slot = options.recovery ? 4 : 0;
        const [reply] = await this.step([{ slot, packet: this.packet(op, data, slot, options) }]);
        assert(reply.status === (options.status ?? 0), `op ${op} status ${reply.status} error ${reply.error}`);
        if (options.error !== undefined) assert(reply.error === options.error, `op ${op} error ${reply.error}`);
        return reply;
    }
    async probe() { const reply = this.wait("probe"); this.node.port.postMessage({ type: "probe" }); return reply; }
    async pressure() {
        if (this.mode === "live") await this.context.suspend();
        const requests = [];
        for (let slot = 0; slot < 4; slot++) requests.push({ slot, packet: this.packet(OP.info, identity([0, 1]), slot) });
        requests.push({ slot: 4, packet: this.packet(OP.release, identity([0xffffffff, 1]), 4) });
        const held = [];
        const replies = await this.step(requests, { duplicate: true, hold: held, resumeLive: this.mode === "live" });
        assert(replies.slice(0, 4).every(reply => reply.status === 0), "normal credits failed");
        assert(replies[4].status === 4 && replies[4].error === 6, "recovery credit was not independently processed");
        assert(held.length === 5 && this.pool.every(buffer => buffer === null), "consumer did not hold all credits");
        const before = await this.probe();
        const delayed = this.packet(OP.parameter, body(16, (v, bytes) => { bytes.set(identity([0, 1])); v.setFloat32(12, 1, true); }), 0, { frame: before.frames + 64 });
        if (this.mode === "offline") { for (let i = 0; i < 16; i++) await this.step([]); }
        else {
            for (let attempt = 0; attempt < 250; attempt++) {
                if ((await this.probe()).frames - before.frames >= Q * 16) break;
                await new Promise(resolve => setTimeout(resolve, 20));
            }
        }
        const after = await this.probe();
        assert(after.frames - before.frames >= Q * 16 && after.bytes === this.ready.bytes && after.pending === 0 && after.busy === 0, "rendering stalled or grew while consumers held credits");
        for (const message of held) this.pool[message.slot] = message.buffer;
        const [late] = await this.step([{ slot: 0, packet: delayed }]);
        assert(late.status === 2 && u64(late.data, 16) >= Q * 16 - 64 && u64(late.data, 8) === late.frame, "delayed wire delivery was not reported late");
        const execution = await this.request(OP.execution, new Uint8Array(), { status: 5 });
        assert(u64(execution.data) === u64(late.data) && u64(execution.data, 8) === late.frame && u32(execution.data, 16) === 0, "delayed operation execution");
        return { held_credits: 5, duplicate_refusals: 1, oversized_refusals: 1, continued_frames: after.frames - before.frames, delayed_lateness: u64(late.data, 16) };
    }
    async finish() {
        assert(this.pool.every(buffer => buffer?.byteLength === 512), "unreturned transfer credits");
        const stopped = this.wait("stopped"); this.node.port.postMessage({ type: "stop" }); const summary = await stopped;
        assert(summary.bytes === summary.finalBytes && summary.max_copy <= 2048 && summary.max_steps <= 512, "memory/work bounds");
        const disposed = this.wait("disposed"); this.node.port.postMessage({ type: "dispose" }); await disposed;
        if (this.mode === "offline") { await this.context.resume(); this.audio = await Promise.race([this.finished, this.failure]); assert(summary.frames === this.journal.length * Q, "hidden offline callbacks"); }
        else { await this.context.close(); assert(this.context.state === "closed", "live context not closed"); }
        this.node.disconnect(); return summary;
    }
}
function snapshot(reply) {
    const bytes = reply.data;
    return { id: handle(bytes), revision: u64(bytes, 8), cut: u64(bytes, 16), format: u32(bytes, 24), frames: u64(bytes, 28), bytes: u32(bytes, 36),
        capture: u32(bytes, 40), active: u32(bytes, 44), incomplete: u32(bytes, 48), start: u64(bytes, 52), next: u64(bytes, 60) };
}
async function graph(host, compiler, memory, variant, { stale = false, badChunk = false } = {}) {
    const len = compiler.compile_graph(variant), steps = compiler.steps(), edges = compiler.edges();
    const descriptor = new Uint8Array(memory.buffer, compiler.descriptor_ptr(), len).slice();
    const start = await host.request(OP.begin, body(8, v => { v.setUint32(0, steps, true); v.setUint32(4, edges, true); }), { status: 1, epoch: stale ? 0 : host.epoch });
    const id = handle(start.data);
    let at = 0;
    for (let index = 0; index < steps + edges; index++) {
        const isStep = index < steps, width = isStep ? 12 : 24, offset = isStep ? index : index - steps;
        const chunk = body(16 + width, (v, bytes) => { bytes.set(identity(id)); v.setUint32(8, offset, true); v.setUint32(12, 1, true); bytes.set(descriptor.subarray(at, at + width), 16); });
        await host.request(isStep ? OP.steps : OP.edges, chunk, { status: 1 });
        if (badChunk && index === 0) {
            await host.request(OP.steps, chunk, { status: 4, error: 17 });
            await host.request(OP.cancelPlan, identity(id), { status: 1, recovery: true }); return null;
        }
        at += width;
    }
    return host.request(OP.graph, identity(id), stale ? { status: 4, error: 14 } : {});
}
function audioBytes(value, frames) { const bytes = new Uint8Array(frames * 4); const v = view(bytes); for (let i = 0; i < frames; i++) v.setFloat32(i * 4, value, true); return bytes; }
function midiRecord(frame, packet) { return body(12 + packet.length, (v, bytes) => { v.setBigUint64(0, BigInt(frame), true); v.setUint32(8, packet.length, true); bytes.set(packet, 12); }); }
async function upload(host, resource, revision, format, frames, bytes, { cancelChunk = false, abandon = false } = {}) {
    const start = await host.request(OP.upload, body(32, (v, data) => { data.set(identity(resource)); v.setBigUint64(8, BigInt(revision), true); v.setUint32(16, format, true); v.setBigUint64(20, BigInt(frames), true); v.setUint32(28, bytes.length, true); }));
    const id = handle(start.data);
    for (let offset = 0; offset < bytes.length; offset += 64) {
        const chunk = bytes.subarray(offset, offset + 64);
        const message = body(16 + chunk.length, (v, data) => { data.set(identity(id)); v.setUint32(8, offset, true); v.setUint32(12, chunk.length, true); data.set(chunk, 16); });
        if (cancelChunk && offset === 64) await host.request(OP.write, message, { cancel: true, status: 3 });
        await host.request(OP.write, message);
        if (abandon) {
            await host.request(OP.cancelUpload, identity(id), { recovery: true });
            await host.request(OP.write, message, { status: 4, error: 6 }); return null;
        }
    }
    await host.request(OP.finish, identity(id)); return host.request(OP.commit, identity(id));
}
async function fetchSnapshot(host, value, expected, release = true) {
    assert(value.bytes === expected.length, "snapshot byte length");
    const cursor = handle((await host.request(OP.cursor, identity(value.id))).data);
    let offset = 0;
    for (let step = 0; step <= Math.ceil(value.bytes / 64); step++) {
        const reply = await host.request(OP.read, body(12, (v, bytes) => { bytes.set(identity(cursor)); v.setUint32(8, 64, true); }));
        const count = u32(reply.data), eof = u32(reply.data, 4); const bytes = reply.data.subarray(8);
        assert(bytes.length === count && offset + count <= expected.length, "export range");
        for (let index = 0; index < count; index++) assert(bytes[index] === expected[offset + index], `export mismatch at ${offset + index}`);
        offset += count;
        if (eof) break;
        assert(count > 0, "export made no progress");
    }
    assert(offset === expected.length, "incomplete export");
    await host.request(OP.closeCursor, identity(cursor));
    if (release) await host.request(OP.release, identity(value.id), { recovery: true });
}
async function scenario(host, compiler, memory) {
    const graphEvents = [], gainEvents = [], resourceEvents = [], midiEvents = [];
    const graph0 = await graph(host, compiler, memory, 0); graphEvents.push([graph0.frame, 1]);
    const audioCapture = handle((await host.request(OP.capture, body(20, (v, bytes) => { bytes.set(identity([1, 1])); v.setUint32(16, 1, true); }))).data);
    const midiCapture = handle((await host.request(OP.capture, body(20, (_v, bytes) => bytes.set(identity([2, 1]))))).data);
    const oldPlayback = snapshot(await host.request(OP.snapshot, identity([0, 1])));
    const at = host.nextFrame + Q * 5 + 64;
    const scheduled = await host.request(OP.parameter, body(16, (v, bytes) => { bytes.set(identity([0, 1])); v.setFloat32(12, 0.5, true); }), { frame: at, status: 2 });
    gainEvents.push([u64(scheduled.data, 8), 0.5]);
    if (host.mode === "offline") assert(u64(scheduled.data, 16) === 0, "unexpected deterministic lateness");
    const note = [0x90, 60, 100];
    const on = await host.request(OP.midi, body(19, (v, bytes) => { bytes.set(identity([0, 1])); v.setUint32(12, 3, true); bytes.set(note, 16); }), { frame: at + 32, status: 2 });
    midiEvents.push([u64(on.data, 8), note]);
    for (let i = 0; i < 7; i++) await host.request(OP.status);
    const firstExecution = await host.request(OP.execution, new Uint8Array(), { status: 5 });
    const secondExecution = await host.request(OP.execution, new Uint8Array(), { status: 5 });
    assert(u64(firstExecution.data) === u64(scheduled.data) && u64(firstExecution.data, 8) === gainEvents[0][0] && u32(firstExecution.data, 16) === 0, "parameter execution result");
    assert(u64(secondExecution.data) === u64(on.data) && u64(secondExecution.data, 8) === midiEvents[0][0] && u32(secondExecution.data, 16) === 0, "MIDI execution result");
    const offPacket = [0x80, 60, 0];
    const off = await host.request(OP.midi, body(19, (v, bytes) => { bytes.set(identity([0, 1])); v.setUint32(12, 3, true); bytes.set(offPacket, 16); }), { frame: 0, status: 2 });
    assert(u64(off.data, 16) > 0 && u64(off.data, 8) === off.frame, "late MIDI must report its effective frame"); midiEvents.push([off.frame, offPacket]);
    const lateExecution = await host.request(OP.execution, new Uint8Array(), { status: 5 }); assert(u64(lateExecution.data, 8) === off.frame, "late execution result");
    const rewired = await graph(host, compiler, memory, 1); graphEvents.push([rewired.frame, -1]);
    await graph(host, compiler, memory, 0, { stale: true }); await graph(host, compiler, memory, 0, { badChunk: true });
    const audioPrefix = snapshot(await host.request(OP.snapshot, identity([1, 1])));
    assert(audioPrefix.active === 1 && audioPrefix.incomplete === 0 && audioPrefix.cut - audioPrefix.start === audioPrefix.frames, "audio capture cut/status");
    await host.request(OP.stopCapture, identity(audioCapture), { recovery: true });
    const midiPrefix = snapshot(await host.request(OP.snapshot, identity([2, 1])));
    assert(midiPrefix.active === 1 && midiPrefix.incomplete === 0 && midiPrefix.cut - midiPrefix.start === midiPrefix.frames, "MIDI capture cut/status");
    await host.request(OP.stopCapture, identity(midiCapture), { recovery: true });
    const valueAt = (events, frame, initial) => { let value = initial; for (const [at, next] of events) if (at <= frame) value = next; return value; };
    const captured = new Uint8Array(audioPrefix.bytes), captureView = view(captured);
    for (let frame = 0; frame < audioPrefix.frames; frame++) captureView.setFloat32(frame * 4, valueAt(gainEvents, audioPrefix.start + frame, 1), true);
    await fetchSnapshot(host, audioPrefix, captured);
    const records = midiEvents.filter(([frame]) => frame >= midiPrefix.start && frame < midiPrefix.cut).map(([frame, packet]) => midiRecord(frame - midiPrefix.start, packet));
    const midiCaptured = new Uint8Array(records.reduce((sum, bytes) => sum + bytes.length, 0)); let position = 0;
    for (const bytes of records) { midiCaptured.set(bytes, position); position += bytes.length; }
    await fetchSnapshot(host, midiPrefix, midiCaptured);
    const replacement = audioBytes(2, 8192); assert(replacement.length >= 16 * 2048);
    const replaced = await upload(host, [0, 1], 1, 1, 8192, replacement, { cancelChunk: true }); resourceEvents.push([replaced.frame, 2]);
    assert(u64(replaced.data) === 2, "replacement revision");
    assert(u32((await host.request(OP.currentness, identity(oldPlayback.id))).data) === 1, "old revision not stale");
    if (host.mode === "live") {
        await new Promise(resolve => setTimeout(resolve, 60));
        const samples = new Float32Array(256); host.analyser.getFloatTimeDomainData(samples);
        assert(samples.every(value => value === -1), `live analyser did not receive the worklet output: ${samples.slice(0, 8)}`);
    }
    const current = snapshot(await host.request(OP.snapshot, identity([0, 1]))); await fetchSnapshot(host, current, replacement);
    await fetchSnapshot(host, oldPlayback, audioBytes(1, 16));
    const midiResource = handle((await host.request(OP.create)).data);
    const midiBytes = new Uint8Array(2560 * 15);
    for (let index = 0; index < 2560; index++) midiBytes.set(midiRecord(index * 2, note), index * 15);
    assert(midiBytes.length >= 16 * 2048);
    await upload(host, midiResource, 0, 0, 5220, midiBytes);
    const midiSnapshot = snapshot(await host.request(OP.snapshot, identity(midiResource))); assert(midiSnapshot.frames === 5220, "MIDI trailing duration");
    await fetchSnapshot(host, midiSnapshot, midiBytes);
    await upload(host, [0, 1], 2, 1, 8192, replacement, { abandon: true });
    await host.request(OP.status, new Uint8Array(), { stream: host.stream + 1, status: 4, error: 7 });
    const reset = await host.request(OP.reset, new Uint8Array(), { recovery: true });
    for (let channel = 0; channel < 16; channel++) for (const cc of [120, 123]) midiEvents.push([reset.frame, [0xb0 | channel, cc, 0]]);
    const empty = await graph(host, compiler, memory, 2); graphEvents.push([empty.frame, 0]);
    const removed = await host.request(OP.removeNode, identity([0, 1])); assert(u64(removed.data, 8) > 0, "processor lost continuous state");
    await host.request(OP.release, identity(handle(removed.data)), { recovery: true });
    for (const resource of [[0, 1], [1, 1], [2, 1], midiResource]) await host.request(OP.removeResource, identity(resource));
    for (let i = 0; i < 16; i++) await host.request(OP.status);
    const final = await host.request(OP.status);
    assert(u32(final.data) === 4096 && u32(final.data, 4) === 0 && u32(final.data, 8) === 16 && u32(final.data, 12) === 16, "resource pools not recovered");
    assert(u32(final.data, 16) === 0 && u32(final.data, 20) === 0 && u32(final.data, 24) === 6 && u32(final.data, 28) === 0 && u32(final.data, 32) === 0 && u32(final.data, 36) === 0, "outstanding fixture owners");
    let midiHash = 2166136261;
    for (const [frame, packet] of midiEvents) { const timestamp = body(8, v => v.setBigUint64(0, BigInt(frame), true)); midiHash = hashBytes(hashBytes(midiHash, timestamp), packet); }
    return { graphEvents, gainEvents, resourceEvents, midiHash, midiEvents: midiEvents.length, expectedSample: frame => valueAt(graphEvents, frame, 0) * valueAt(gainEvents, frame, 1) * valueAt(resourceEvents, frame, 1) };
}
function encodeJournal(journal, samples) {
    const arrays = [new Uint8Array([82, 84, 74, 49]), body(4, v => v.setUint32(0, journal.length, true))];
    for (let index = 0; index < journal.length; index++) {
        const record = journal[index];
        for (const kind of ["inputs", "outputs"]) {
            const packets = record[kind].slice().sort((a, b) => a.slot - b.slot);
            const mask = packets.reduce((mask, packet) => mask | (1 << packet.slot), 0);
            arrays.push(body(4, v => v.setUint32(0, mask, true)));
            for (const packet of packets) { arrays.push(body(4, v => v.setUint32(0, packet.bytes.length, true))); arrays.push(packet.bytes); }
        }
        arrays.push(body(4, v => v.setUint32(0, hashSamples(samples, index * Q, (index + 1) * Q), true)));
    }
    const bytes = new Uint8Array(arrays.reduce((sum, bytes) => sum + bytes.length, 0)); let at = 0;
    for (const array of arrays) { bytes.set(array, at); at += array.length; } return bytes;
}
export async function run() {
    assert(crossOriginIsolated === false, "fixture must run without isolation headers");
    const module = await WebAssembly.compile(await (await fetch("./pkg/fixture_bg.wasm")).arrayBuffer());
    const wasm = initSync({ module }), compiler = new Controller(); compiler.set_marker(42); assert(compiler.marker() === 42);
    let graph_hash = 2166136261;
    for (let variant = 0; variant < 3; variant++) { const length = compiler.compile_graph(variant); graph_hash = hashBytes(graph_hash, new Uint8Array(wasm.memory.buffer, compiler.descriptor_ptr(), length)); }
    const reports = [];
    for (const [mode, stream] of [["offline", 1], ["live", 2]]) {
        const host = new Host(mode, module, stream);
        try {
            await host.start(); const pressure = await host.pressure();
            const expected = await scenario(host, compiler, wasm.memory); const summary = await host.finish();
            assert(summary.midi_hash === expected.midiHash && summary.midi_events === expected.midiEvents, "MIDI event journal differs");
            if (mode === "offline") {
                const samples = host.audio.getChannelData(0);
                for (let frame = 0; frame < samples.length; frame++) assert(samples[frame] === expected.expectedSample(frame), `Web Audio sample ${frame}: ${samples[frame]} != ${expected.expectedSample(frame)}`);
                assert(hashSamples(samples, 0, summary.frames) === summary.sample_hash, "actual Web Audio sink differs from Rust output");
                const journal = encodeJournal(host.journal, samples);
                const saved = await fetch("./journal", { method: "POST", body: journal }); assert(saved.ok, "journal was not saved");
            }
            reports.push({ mode, ...summary, graph_hash, pressure, decoder_fallback: host.ready.decoderFallback, independent_instances: true, cross_origin_isolated: false });
        } catch (error) {
            if (mode === "live" && host.context?.state !== "closed") await host.context.close().catch(() => {});
            throw new Error(`${mode}: ${error.stack || error}`);
        }
    }
    compiler.free(); return { passed: true, reports };
}
