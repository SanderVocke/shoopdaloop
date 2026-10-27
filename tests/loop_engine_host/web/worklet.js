import { decoderFallback } from "./text_decoder.js";
import { initSync, BrowserBridge } from "./pkg/fixture.js";

class LoopFixture extends AudioWorkletProcessor {
    constructor(options) {
        super();
        this.armed = false; this.stopped = false; this.failed = false; this.firstFrame = null;
        this.canary = options.processorOptions.canary;
        this.returns = Array.from({ length: 10 }, () => new ArrayBuffer(96));
        try {
            this.wasm = initSync({ module: options.processorOptions.module });
            this.engine = new BrowserBridge(BigInt(options.processorOptions.session), this.canary === "processor" ? 1 : this.canary === "rust-heap" ? 2 : 0);
            this.memory = this.wasm.memory;
            if (!(this.memory.buffer instanceof ArrayBuffer)) throw new Error("ordinary non-shared Wasm memory required");
            this.bytes = this.memory.buffer.byteLength;
            this.request = new Uint8Array(this.memory.buffer, this.engine.request_ptr(), 64);
            this.response = new Uint8Array(this.memory.buffer, this.engine.response_ptr(), 96);
            this.audio = [0, 1].map(channel => new Float32Array(this.memory.buffer, this.engine.output_ptr(channel), 128));
            this.port.onmessage = event => this.message(event.data);
            this.port.postMessage({ type: "ready", bytes: this.bytes, marker: this.engine.marker(), shared: false, decoderFallback });
        } catch (error) { this.fail(error); }
    }
    fail(error) {
        this.failed = true;
        const witness = this.engine ? { code: this.lastResult ?? null, processor: this.engine.fault_witness(),
            heap: [this.engine.heap_allocations(), this.engine.heap_reallocations(), this.engine.heap_deallocations()] } : null;
        this.port.postMessage({ type: "fatal", error: String(error.stack || error), witness });
    }
    checkMemory() { if (this.memory.buffer.byteLength !== this.bytes) throw new Error("loop Wasm memory grew after preparation"); }
    message(data) {
        try {
            if (this.failed || this.stopped) throw new Error("message after loop host stopped");
            if (data.type === "request") {
                if (!(data.buffer instanceof ArrayBuffer) || data.buffer.byteLength !== 64) throw new Error("request envelope");
                this.request.set(new Uint8Array(data.buffer));
                const result = this.engine.ingest();
                this.checkMemory();
                this.port.postMessage({ type: "ingested", slot: data.slot, result, buffer: data.buffer }, [data.buffer]);
            } else if (data.type === "arm") {
                if (this.armed || this.engine.frame() !== 0n || this.engine.outstanding() !== 3) throw new Error("invalid arm state");
                this.armed = true;
                this.port.postMessage({ type: "armed" });
            } else if (data.type === "credit") {
                if (!Number.isInteger(data.id) || data.id < 0 || data.id >= 10 || this.returns[data.id] !== null || !(data.buffer instanceof ArrayBuffer) || data.buffer.byteLength !== 96) throw new Error("response credit");
                this.returns[data.id] = data.buffer;
            } else if (data.type === "poll") {
                for (let id = 0; id < 10; id++) {
                    const buffer = this.returns[id];
                    if (!buffer) continue;
                    const result = this.engine.poll_response();
                    if (result === 0) break;
                    if (result !== 1) throw new Error(`response polling failed: ${result}`);
                    new Uint8Array(buffer).set(this.response);
                    this.returns[id] = null;
                    this.port.postMessage({ type: "response", id, buffer }, [buffer]);
                }
                this.checkMemory();
                this.port.postMessage({ type: "polled", outstanding: this.engine.outstanding(), firstFrame: this.firstFrame });
            } else if (data.type === "stop") {
                if (this.returns.some(buffer => !buffer) || this.engine.outstanding() !== 0) throw new Error("stop with outstanding credits");
                this.stopped = true;
                this.checkMemory();
                const journal = new Uint8Array(this.memory.buffer, this.engine.journal_ptr(), this.engine.journal_len()).slice();
                const summary = { bytes: this.bytes, finalBytes: this.memory.buffer.byteLength,
                    frames: String(this.engine.frame()), callbacks: String(this.engine.callbacks()), firstFrame: this.firstFrame,
                    journalOverflow: this.engine.journal_overflow(), returnedBuffers: this.returns.length,
                    marker: this.engine.marker(), heap: [this.engine.heap_allocations(), this.engine.heap_reallocations(), this.engine.heap_deallocations()] };
                const result = this.engine.stop();
                if (result !== 0) throw new Error(`stopped cleanup failed: ${result}`);
                summary.recovered = [this.engine.recovered_pages(), this.engine.recovered_versions(), this.engine.recovered_snapshots(), this.engine.media_references()];
                this.engine.free(); this.engine = null;
                this.port.postMessage({ type: "stopped", ...summary, journal: journal.buffer }, [journal.buffer]);
            } else { throw new Error("unknown loop host message"); }
        } catch (error) { this.fail(error); }
    }
    process(_inputs, outputs) {
        if (this.stopped || this.failed) return false;
        if (!this.armed) return true;
        try {
            if (outputs.length !== 1 || outputs[0].length !== 2 || outputs[0].some(channel => channel.length !== 128)) throw new Error("loop quantum shape");
            if (this.firstFrame === null) this.firstFrame = String(currentFrame);
            const result = this.engine.render(128);
            this.lastResult = result;
            if (result === 0xffff0001 && this.canary === "rust-heap" && this.engine.fault_witness() === 2 && this.engine.heap_allocations() > 0 && this.engine.heap_reallocations() > 0 && this.engine.heap_deallocations() > 0) {
                throw new Error(`intentional loop Rust heap failure canary detected: ${this.engine.heap_allocations()}/${this.engine.heap_reallocations()}/${this.engine.heap_deallocations()}`);
            }
            if (result === 10 && this.canary === "processor" && this.engine.fault_witness() === 1) throw new Error("intentional loop processor failure canary detected");
            if (result !== 0) throw new Error(`loop rendering failed: ${result}`);
            this.checkMemory();
            outputs[0][0].set(this.audio[0]); outputs[0][1].set(this.audio[1]);
            return true;
        } catch (error) { this.fail(error); return false; }
    }
}

// A separate graph node observes actual Web Audio buffers, not Rust's output memory.
// It retains the first non-silent quantum and the following one in fixed storage.
class AudioProbe extends AudioWorkletProcessor {
    constructor() {
        super();
        this.left = new Float32Array(256); this.right = new Float32Array(256);
        this.start = null; this.count = 0;
        this.port.onmessage = () => {
            const ready = this.count === 256;
            this.port.postMessage({ type: "probe", ready, start: this.start,
                left: ready ? Array.from(this.left) : null, right: ready ? Array.from(this.right) : null });
        };
    }
    process(inputs, outputs) {
        const input = inputs[0], output = outputs[0];
        if (!input || input.length !== 2) return true;
        for (let channel = 0; channel < 2; channel++) output[channel].set(input[channel]);
        if (this.start === null && input[0].some(value => value !== 0)) this.start = String(currentFrame);
        if (this.start !== null && this.count < 256) {
            this.left.set(input[0], this.count); this.right.set(input[1], this.count); this.count += 128;
        }
        return true;
    }
}
registerProcessor("loop-fixture", LoopFixture);
registerProcessor("loop-audio-probe", AudioProbe);
