import { initSync, BrowserClient, BrowserBridge, BrowserGraphReceiver, BrowserGraphClient } from "./pkg/fixture.js";
import { checkBindings } from "./bindings_checks.js";
const RATE = 48000, Q = 128;
const assert = (condition, message) => { if (!condition) throw new Error(message); };
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

class Host {
    constructor(mode, module, memory, client, session, canary) {
        Object.assign(this, { mode, module, memory, client, session, canary });
        this.waiters = new Map(); this.requests = []; this.responses = [];
        this.pool = Array.from({ length: 5 }, () => new ArrayBuffer(64));
        this.failure = new Promise((_, reject) => { this.reject = reject; });
        this.failure.catch(() => {});
        this.controlBytes = memory.buffer.byteLength;
    }
    wait(key) {
        assert(!this.waiters.has(key), `duplicate waiter ${key}`);
        return Promise.race([new Promise(resolve => this.waiters.set(key, resolve)), this.failure]);
    }
    message(data) {
        try {
            if (data.type === "fatal") {
                const error = new Error(data.error); error.workletFailure = data.witness; throw error;
            }
            if (data.type === "response") {
                assert(data.buffer instanceof ArrayBuffer && data.buffer.byteLength === 96, "response envelope");
                const bytes = new Uint8Array(data.buffer);
                new Uint8Array(this.memory.buffer, this.client.response_ptr(), 96).set(bytes);
                assert(this.client.receive() === 0, "Rust client refused response");
                this.responses.push(Array.from(bytes));
                this.node.port.postMessage({ type: "credit", id: data.id, buffer: data.buffer }, [data.buffer]);
                assert(data.buffer.byteLength === 0, "response credit did not transfer");
                return;
            }
            const key = data.slot === undefined ? data.type : `${data.type}:${data.slot}`;
            const resolve = this.waiters.get(key);
            assert(resolve, `unsolicited message ${key}`);
            this.waiters.delete(key); resolve(data);
        } catch (error) { this.reject(error); }
    }
    async call(type, data = {}, key = type, port = this.node.port) {
        const result = this.wait(key); port.postMessage({ type, ...data }); return result;
    }
    async start() {
        this.context = this.mode === "offline" ? new OfflineAudioContext(2, 512, RATE) : new AudioContext({ sampleRate: RATE });
        assert(this.context.sampleRate === RATE, "unexpected sample rate");
        await this.context.audioWorklet.addModule("./worklet.js");
        const ready = this.wait("ready");
        this.node = new AudioWorkletNode(this.context, "loop-fixture", { numberOfInputs: 0, numberOfOutputs: 1,
            outputChannelCount: [2], processorOptions: { module: this.module, session: this.session, canary: this.canary } });
        this.node.port.onmessage = ({ data }) => this.message(data);
        this.node.onprocessorerror = () => this.reject(new Error("loop AudioWorklet processorerror"));
        this.probe = new AudioWorkletNode(this.context, "loop-audio-probe", { numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [2] });
        this.probe.port.onmessage = ({ data }) => this.message(data);
        this.probe.onprocessorerror = () => this.reject(new Error("audio probe processorerror"));
        this.node.connect(this.probe);
        if (this.mode === "offline") {
            this.probe.connect(this.context.destination);
            const pause = this.context.suspend(Q / RATE);
            this.finished = this.context.startRendering();
            await Promise.race([pause, this.failure]);
        } else {
            const mute = this.context.createGain(); mute.gain.value = 0;
            this.probe.connect(mute).connect(this.context.destination);
            await this.context.resume();
        }
        this.ready = await ready;
        assert(this.ready.marker === 0 && this.ready.shared === false, "worklet instance isolation");
    }
    async submit(kind, at) {
        assert(this.client.prepare(kind, BigInt(at), 0n, 0) === 0, "request preparation");
        const request = new Uint8Array(this.memory.buffer, this.client.request_ptr(), 64);
        const slot = new DataView(request.buffer, request.byteOffset, 64).getUint32(32, true);
        const buffer = this.pool[slot];
        assert(buffer?.byteLength === 64, "missing request buffer");
        this.pool[slot] = null; new Uint8Array(buffer).set(request);
        this.requests.push(Array.from(request));
        const ingested = this.wait(`ingested:${slot}`);
        this.node.port.postMessage({ type: "request", slot, buffer }, [buffer]);
        assert(buffer.byteLength === 0, "request did not transfer");
        const reply = await ingested;
        assert(reply.result === 0 && reply.buffer.byteLength === 64, "ingress refusal or lost ownership");
        this.pool[slot] = reply.buffer;
    }
    async run() {
        await this.start();
        await Promise.all([[1, 0], [2, 4], [3, 12]].map(([kind, at]) => this.submit(kind, at)));
        await this.call("arm", {}, "armed");
        if (this.mode === "offline") {
            const pause = this.context.suspend(3 * Q / RATE);
            await this.context.resume(); await Promise.race([pause, this.failure]);
        }
        let actual, poll;
        const deadline = performance.now() + 15000;
        do {
            poll = await this.call("poll", {}, "polled");
            actual = await this.call("probe", {}, "probe", this.probe.port);
            if (actual.ready && this.client.outstanding() === 0) break;
            assert(performance.now() < deadline, "live output/receipt watchdog");
            await delay(5);
        } while (true);
        assert(actual.start === poll.firstFrame, "actual graph output start differs from acknowledged engine start");
        assert(this.responses.length === 6 && this.requests.length === 3, "complete wire receipts");
        assert(this.pool.every(buffer => buffer?.byteLength === 64), "unreturned request buffers");
        const summary = await this.call("stop", {}, "stopped");
        assert(summary.returnedBuffers === 10 && summary.bytes === summary.finalBytes, "response pool or render memory changed");
        assert(summary.journalOverflow === 0 && summary.heap.every(count => count === 0), "render guard/journal failure");
        assert(BigInt(summary.frames) >= 256n && BigInt(summary.frames) === BigInt(summary.callbacks) * 128n, "render clock/callback counts");
        assert(summary.marker === 0 && this.client.marker() === 0x51a17, "instance marker changed");
        assert(this.memory.buffer.byteLength === this.controlBytes, "control memory grew");
        assert(JSON.stringify(summary.recovered) === "[256,16,16,0]", "incomplete stopped recovery");
        const audio = [actual.left, actual.right];
        const expected = [1, 2, 3, 4, .5, 1, 1.5, 2, .5, 1, 1.5, 2];
        for (let channel = 0; channel < 2; channel++) for (let frame = 0; frame < 256; frame++) {
            assert(audio[channel][frame] === (expected[frame] ?? 0) * (channel === 0 ? 1 : 10), `actual Web Audio sample ${channel}/${frame}`);
        }
        let offlineAudio = null;
        if (this.mode === "offline") {
            await this.context.resume();
            const rendered = await Promise.race([this.finished, this.failure]);
            offlineAudio = [0, 1].map(channel => Array.from(rendered.getChannelData(channel)));
            const start = Number(BigInt(actual.start));
            for (let channel = 0; channel < 2; channel++) for (let frame = 0; frame < 512; frame++) {
                assert(offlineAudio[channel][frame] === (frame >= start && frame < start + 256 ? audio[channel][frame - start] : 0), "OfflineAudioContext output differs from downstream probe");
            }
        } else { await this.context.close(); }
        this.node.disconnect(); this.probe.disconnect();
        return { mode: this.mode, ready: this.ready, summary: { ...summary, journal: undefined, semantic: undefined },
            controlBytes: this.controlBytes, requests: this.requests, responses: this.responses,
            midiBytes: Array.from(new Uint8Array(summary.journal)), semanticBytes: Array.from(new Uint8Array(summary.semantic)), audio, offlineAudio };
    }
    async cleanup() {
        this.node?.disconnect(); this.probe?.disconnect();
        if (this.mode === "live" && this.context && this.context.state !== "closed") await this.context.close();
    }
}
async function runInner() {
    assert(!crossOriginIsolated, "fixture unexpectedly cross-origin isolated");
    const module = await WebAssembly.compile(await (await fetch("./pkg/fixture_bg.wasm")).arrayBuffer());
    const wasm = initSync({ module });
    assert(wasm.memory.buffer instanceof ArrayBuffer, "ordinary non-shared control memory required");
    const canary = new URLSearchParams(location.search).get("failure");
    const bindingChecks = canary ? null : checkBindings(wasm, BrowserClient, BrowserBridge, BrowserGraphReceiver, BrowserGraphClient);
    const reports = [];
    for (const mode of canary ? ["offline"] : ["offline", "live"]) {
        const session = mode === "offline" ? "18446744073709551615" : "18446744073709551614";
        const client = new BrowserClient(BigInt(session)); client.set_marker(0x51a17);
        const host = new Host(mode, module, wasm.memory, client, session, canary);
        try { reports.push(await host.run()); }
        finally { await host.cleanup(); client.free(); }
    }
    return { format: "LLH2", passed: true, cross_origin_isolated: crossOriginIsolated, bindingChecks, reports };
}
export async function run() {
    try { return await runInner(); }
    catch (error) { return { format: "LLH2", passed: false, failure: String(error.stack || error), workletFailure: error.workletFailure ?? null }; }
}
