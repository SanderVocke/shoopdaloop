import { decoderFallback } from "./text_decoder.js";
import { initSync, Bridge } from "./pkg/fixture.js";

class RuntimeFixture extends AudioWorkletProcessor {
    constructor(options) {
        super();
        this.stopped = false;
        this.disposed = false;
        this.failed = false;
        this.head = 0; this.tail = 0; this.count = 0;
        this.queue = new Uint8Array(5);
        this.busy = new Uint8Array(5);
        this.slots = Array.from({ length: 5 }, () => ({ buffer: null, length: 0 }));
        try {
            this.wasm = initSync({ module: options.processorOptions.module });
            this.engine = new Bridge(BigInt(options.processorOptions.stream));
            this.memory = this.wasm.memory;
            if (typeof SharedArrayBuffer !== "undefined" && this.memory.buffer instanceof SharedArrayBuffer) throw new Error("shared memory is forbidden");
            this.bytes = this.memory.buffer.byteLength;
            this.views = Array.from({ length: 5 }, (_, slot) => new Uint8Array(this.memory.buffer, this.engine.frame_ptr(slot), 512));
            this.output = new Float32Array(this.memory.buffer, this.engine.output_ptr(), 128);
            this.port.onmessage = event => this.message(event.data);
            this.port.postMessage({ type: "ready", bytes: this.bytes, marker: this.engine.marker(), shared: false, decoderFallback });
        } catch (error) { this.fail(error); }
    }
    fail(error) {
        this.failed = true;
        this.port.postMessage({ type: "fatal", error: String(error.stack || error) });
    }
    message(message) {
        try {
            if (message.type === "frame") {
                const { slot, length, buffer } = message;
                if (!Number.isInteger(slot) || slot < 0 || slot >= 5 || !(buffer instanceof ArrayBuffer) || buffer.byteLength !== 512 || !Number.isInteger(length) || length < 0 || length > 512) {
                    if (buffer instanceof ArrayBuffer) this.port.postMessage({ type: "refused", slot, buffer, error: "envelope" }, [buffer]);
                    else this.port.postMessage({ type: "refused", slot, error: "envelope" });
                    return;
                }
                if (this.stopped || this.busy[slot] || this.count === 5) {
                    this.port.postMessage({ type: "refused", slot, buffer }, [buffer]); return;
                }
                this.busy[slot] = 1; this.slots[slot].buffer = buffer; this.slots[slot].length = length;
                this.queue[this.tail] = slot; this.tail = (this.tail + 1) % 5; this.count++;
                this.port.postMessage({ type: "staged", slot });
            } else if (message.type === "probe") {
                if (this.disposed) throw new Error("probe after disposal");
                this.port.postMessage({ type: "probe", frames: Number(this.engine.next_frame()), bytes: this.memory.buffer.byteLength,
                    pending: this.count, busy: Array.from(this.busy).reduce((sum, value) => sum + value, 0) });
            } else if (message.type === "stop") {
                if (this.count || this.busy.some(value => value)) throw new Error("stop with outstanding transfer ownership");
                this.stopped = true;
                this.summary = { bytes: this.bytes, finalBytes: this.memory.buffer.byteLength, callbacks: Number(this.engine.callbacks()),
                    frames: Number(this.engine.next_frame()), sample_hash: this.engine.sample_hash(), midi_hash: this.engine.midi_hash(),
                    midi_events: Number(this.engine.midi_events()), max_copy: this.engine.max_copy(), max_steps: this.engine.max_steps() };
                this.port.postMessage({ type: "stopped", ...this.summary });
            } else if (message.type === "dispose") {
                if (!this.stopped || this.disposed) throw new Error("dispose requires a stopped callback");
                this.engine.free(); this.engine = null; this.disposed = true;
                this.port.postMessage({ type: "disposed", frames: this.summary.frames });
            } else { throw new Error("unknown host message"); }
        } catch (error) { this.fail(error); }
    }
    process(_inputs, outputs) {
        if (this.stopped || this.failed) { for (const channel of outputs[0] || []) channel.fill(0); return false; }
        try {
            for (let work = 0; work < 5 && this.count > 0; work++) {
                const slot = this.queue[this.head]; this.head = (this.head + 1) % 5; this.count--;
                this.views[slot].set(new Uint8Array(this.slots[slot].buffer));
                const error = this.engine.ingest(slot, this.slots[slot].length);
                if (error) throw new Error(`prepared ingress refused owned credit: ${error}`);
            }
            const ready = this.engine.render();
            if (ready & 0x80000000) throw new Error(`Rust rendering failed: ${this.engine.error()}`);
            if (this.memory.buffer.byteLength !== this.bytes) throw new Error("Wasm memory grew after preparation");
            if (outputs.length !== 1 || outputs[0].length !== 1 || outputs[0][0].length !== 128) throw new Error("unexpected browser quantum shape");
            outputs[0][0].set(this.output);
            for (let slot = 0; slot < 5; slot++) if (ready & (1 << slot)) {
                const buffer = this.slots[slot].buffer;
                if (!this.busy[slot] || !buffer) throw new Error("completion without transfer ownership");
                new Uint8Array(buffer).set(this.views[slot]);
                const length = this.engine.reply_len(slot);
                this.slots[slot].buffer = null; this.busy[slot] = 0;
                this.port.postMessage({ type: "returned", slot, length, buffer }, [buffer]);
            }
            return true;
        } catch (error) { this.fail(error); return false; }
    }
}
registerProcessor("rt-runtime-fixture", RuntimeFixture);
