import { decoderFallback } from "./text_decoder.js";
import { initSync, BrowserBridge, BrowserPlannerCanary } from "./pkg/fixture.js";

class LoopFixture extends AudioWorkletProcessor {
    constructor(options) {
        super();
        this.armed = false; this.stopped = false; this.failed = false; this.firstFrame = null;
        this.canary = options.processorOptions.canary;
        this.plannerMode = ["planner-control", "planner", "planner-rust-heap"].includes(this.canary);
        this.graphMode = options.processorOptions.graph === true;
        this.exportMode = options.processorOptions.exports === true;
        this.stagingMode = options.processorOptions.staging === true; this.stagingFrames = 0;
        this.denseMode = options.processorOptions.dense === true; this.denseFrames = 0;
        this.mediaMode = options.processorOptions.media === true;
        this.stagingReturns = this.stagingMode ? Array.from({length:2},()=>new ArrayBuffer(256)) : [];
        this.exportReturns = this.exportMode ? Array.from({ length: 2 }, () => new ArrayBuffer(1024)) : [];
        this.primeRequested = false; this.primed = false; this.resumeFrame = null;
        this.returns = Array.from({ length: 10 }, () => new ArrayBuffer(96));
        this.graphReturns = this.graphMode ? Array.from({ length: 4 }, () => new ArrayBuffer(128)) : [];
        try {
            this.wasm = initSync({ module: options.processorOptions.module });
            const session = BigInt(options.processorOptions.session), fault = this.canary === "processor" ? 1 : this.canary === "rust-heap" ? 2 : 0;
            this.engine = this.plannerMode ? new BrowserPlannerCanary(this.canary === "planner" ? 1 : this.canary === "planner-rust-heap" ? 2 : 0) : this.mediaMode ? BrowserBridge.new_media(session) : this.denseMode ? BrowserBridge.new_dense(session) : this.stagingMode ? BrowserBridge.new_staging(session) : this.exportMode ? BrowserBridge.new_exports(session) : this.graphMode ? BrowserBridge.new_graphs(session, fault) : new BrowserBridge(session, fault);
            this.memory = this.wasm.memory;
            if (!(this.memory.buffer instanceof ArrayBuffer)) throw new Error("ordinary non-shared Wasm memory required");
            this.bytes = this.memory.buffer.byteLength;
            if (!this.plannerMode) {
                this.request = new Uint8Array(this.memory.buffer, this.engine.request_ptr(), 64);
                this.response = new Uint8Array(this.memory.buffer, this.engine.response_ptr(), 96);
            }
            if (this.graphMode) {
                this.graphPayload = new Uint8Array(this.memory.buffer, this.engine.graph_payload_ptr(), 96);
                this.graphRequest = new Uint8Array(this.memory.buffer, this.engine.graph_request_ptr(), 96);
                this.graphResponse = new Uint8Array(this.memory.buffer, this.engine.graph_response_ptr(), 128);
            }
            if (this.exportMode) {
                this.exportRequest = new Uint8Array(this.memory.buffer, this.engine.export_request_ptr(), 96);
                this.exportResponse = new Uint8Array(this.memory.buffer, this.engine.export_response_ptr(), 1024);
            }
            if(this.stagingMode){this.stagingRequest=new Uint8Array(this.memory.buffer,this.engine.staging_request_ptr(),128);this.stagingResponse=new Uint8Array(this.memory.buffer,this.engine.staging_response_ptr(),256);}
            this.audio = [0, 1].map(channel => new Float32Array(this.memory.buffer, this.engine.output_ptr(channel), 128));
            this.port.onmessage = event => this.message(event.data);
            this.port.postMessage({ type: "ready", bytes: this.bytes, marker: this.engine.marker(), shared: false, decoderFallback, exportStorage: this.exportMode ? [this.engine.export_resource_bytes(),this.engine.export_read_bytes(),96,1024,2,1] : null });
        } catch (error) { this.fail(error); }
    }
    fail(error) {
        this.failed = true;
        const witness = this.engine ? { code: this.lastResult ?? null, processor: this.plannerMode ? 0 : this.engine.fault_witness(),
            planner: this.plannerMode ? this.engine.fault_witness() : 0,
            plans: this.plannerMode ? this.engine.plans() : null, finishes: this.plannerMode ? this.engine.finishes() : null,
            frame: String(this.engine.frame()), callbacks: String(this.engine.callbacks()),
            bytes: this.bytes, finalBytes: this.memory.buffer.byteLength,
            heap: [this.engine.heap_allocations(), this.engine.heap_reallocations(), this.engine.heap_deallocations()] } : null;
        this.port.postMessage({ type: "fatal", error: String(error.stack || error), witness });
    }
    checkMemory() { if (this.memory.buffer.byteLength !== this.bytes) throw new Error("loop Wasm memory grew after preparation"); }
    message(data) {
        try {
            if (this.failed || this.stopped) throw new Error("message after loop host stopped");
            if (this.plannerMode) { this.plannerMessage(data); return; }
            if (data.type === "request") {
                if (!(data.buffer instanceof ArrayBuffer) || data.buffer.byteLength !== 64) throw new Error("request envelope");
                this.request.set(new Uint8Array(data.buffer));
                const result = this.engine.ingest();
                this.checkMemory();
                this.port.postMessage({ type: "ingested", slot: data.slot, result, buffer: data.buffer }, [data.buffer]);
            } else if(data.type === "dense-step") {
                if(!this.denseMode||this.armed||String(this.engine.frame())!==data.frame||!Number.isInteger(data.frames)||data.frames<1||data.frames>128)throw new Error("dense step clock/shape");
                this.denseFrames=data.frames;this.armed=true;this.port.postMessage({type:"dense-stepping"});
            } else if(data.type === "dense-state") {
                if(!this.denseMode||this.armed)throw new Error("dense state while armed");
                this.checkMemory();this.port.postMessage({type:"dense-state",frame:String(this.engine.frame()),outstanding:this.engine.outstanding(),stop:this.engine.stop()});
            } else if(data.type === "staging-request") {
                if(!this.stagingMode||data.slot!==0||!(data.buffer instanceof ArrayBuffer)||data.buffer.byteLength!==128)throw new Error("staging input envelope");
                this.stagingRequest.set(new Uint8Array(data.buffer));const frame=String(this.engine.frame()),result=this.engine.ingest_staging();this.checkMemory();this.port.postMessage({type:"staging-ingested",slot:0,result,frame,buffer:data.buffer},[data.buffer]);if(data.buffer.byteLength!==0)throw new Error("staging input did not detach");
            } else if(data.type === "staging-step") {
                if(!this.stagingMode||this.armed||this.primeRequested||String(this.engine.frame())!==data.frame||!Number.isInteger(data.frames)||data.frames<1||data.frames>128)throw new Error("staging step clock/shape");this.stagingFrames=data.frames;this.armed=true;this.port.postMessage({type:"staging-stepping"});
            } else if(data.type === "staging-stop-probe") {
                if(!this.stagingMode||this.engine.stop()!==15)throw new Error("retained staging owner allowed stop");this.port.postMessage({type:"staging-stop-refused"});
            } else if(data.type === "staging-credit") {
                if(!this.stagingMode||!Number.isInteger(data.id)||data.id<0||data.id>=2||this.stagingReturns[data.id]!==null||!(data.buffer instanceof ArrayBuffer)||data.buffer.byteLength!==256)throw new Error("staging response credit");this.stagingReturns[data.id]=data.buffer;
            } else if (data.type === "export-request") {
                if (!this.exportMode || data.slot !== 0 || !(data.buffer instanceof ArrayBuffer) || data.buffer.byteLength !== 96) throw new Error("export request envelope");
                this.exportRequest.set(new Uint8Array(data.buffer));
                const frame = String(this.engine.frame()), result = this.engine.ingest_export();
                this.checkMemory(); this.port.postMessage({ type: "export-ingested", slot: 0, result, frame, buffer: data.buffer }, [data.buffer]);
                if (data.buffer.byteLength !== 0) throw new Error("export input did not detach");
            } else if (data.type === "export-step") {
                if (!this.exportMode || this.armed || this.primeRequested || String(this.engine.frame()) !== data.frame) throw new Error("export step clock/state");
                this.armed = true; this.port.postMessage({ type: "export-stepping" });
            } else if (data.type === "export-stop-probe") {
                if (!this.exportMode || this.engine.stop() !== 15) throw new Error("retained export allowed stop");
                this.port.postMessage({ type: "export-stop-refused" });
            } else if (data.type === "export-credit") {
                if (!this.exportMode || !Number.isInteger(data.id) || data.id < 0 || data.id >= 2 || this.exportReturns[data.id] !== null || !(data.buffer instanceof ArrayBuffer) || data.buffer.byteLength !== 1024) throw new Error("export response credit");
                this.exportReturns[data.id] = data.buffer;
            } else if (data.type === "graph-payload" || data.type === "graph-request") {
                if (!this.graphMode || !(data.buffer instanceof ArrayBuffer) || data.buffer.byteLength !== 96 || !Number.isInteger(data.slot) || data.slot < 0 || data.slot >= (data.type === "graph-payload" ? 1 : 2)) throw new Error("graph input envelope");
                const payload = data.type === "graph-payload", frame = String(this.engine.frame());
                (payload ? this.graphPayload : this.graphRequest).set(new Uint8Array(data.buffer));
                const result = payload ? this.engine.ingest_graph_payload() : this.engine.ingest_graph();
                this.checkMemory();
                this.port.postMessage({ type: payload ? "graph-payload-ack" : "graph-ingested", slot: data.slot, result, frame, buffer: data.buffer }, [data.buffer]);
                if (data.buffer.byteLength !== 0) throw new Error("graph input return did not detach");
            } else if (data.type === "prime") {
                if (!this.graphMode || this.primed || this.primeRequested || this.armed || this.engine.frame() !== 0n || this.engine.outstanding() !== 4) throw new Error("invalid graph prime state");
                this.primeRequested = true;
                this.port.postMessage({ type: "primed" });
            } else if (data.type === "arm") {
                if (this.armed || this.primeRequested || this.engine.frame() !== (this.graphMode ? 1n : 0n) || (this.graphMode && !this.primed) || this.engine.outstanding() !== 3) throw new Error("invalid arm state");
                this.armed = true;
                this.port.postMessage({ type: "armed" });
            } else if (data.type === "credit") {
                if (!Number.isInteger(data.id) || data.id < 0 || data.id >= 10 || this.returns[data.id] !== null || !(data.buffer instanceof ArrayBuffer) || data.buffer.byteLength !== 96) throw new Error("response credit");
                this.returns[data.id] = data.buffer;
            } else if (data.type === "graph-credit") {
                if (!this.graphMode || !Number.isInteger(data.id) || data.id < 0 || data.id >= 4 || this.graphReturns[data.id] !== null || !(data.buffer instanceof ArrayBuffer) || data.buffer.byteLength !== 128) throw new Error("graph response credit");
                this.graphReturns[data.id] = data.buffer;
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
                for (let id = 0; id < this.graphReturns.length; id++) {
                    const buffer = this.graphReturns[id];
                    if (!buffer) continue;
                    const result = this.engine.poll_graph_response();
                    if (result === 0) break;
                    if (result !== 1) throw new Error(`graph response polling failed: ${result}`);
                    new Uint8Array(buffer).set(this.graphResponse);
                    this.graphReturns[id] = null;
                    this.port.postMessage({ type: "graph-response", id, buffer }, [buffer]);
                    if (buffer.byteLength !== 0) throw new Error("graph response did not detach");
                }
                for (let id = 0; id < this.exportReturns.length; id++) {
                    const buffer = this.exportReturns[id]; if (!buffer) continue;
                    const result = this.engine.poll_export_response(); if (result === 0) break;
                    if (result !== 1) throw new Error(`export poll failed: ${result}`);
                    new Uint8Array(buffer).set(this.exportResponse); this.exportReturns[id] = null;
                    this.port.postMessage({ type: "export-response", id, buffer }, [buffer]);
                    if (buffer.byteLength !== 0) throw new Error("export response did not detach");
                }
                for(let id=0;id<this.stagingReturns.length;id++){const buffer=this.stagingReturns[id];if(!buffer)continue;const result=this.engine.poll_staging_response();if(result===0)break;if(result!==1)throw new Error(`staging poll failed: ${result}`);new Uint8Array(buffer).set(this.stagingResponse);this.stagingReturns[id]=null;this.port.postMessage({type:"staging-response",id,buffer},[buffer]);if(buffer.byteLength!==0)throw new Error("staging response did not detach");}
                this.checkMemory();
                this.port.postMessage({ type: "polled", availableResponses:this.returns.filter(buffer=>buffer!==null).length, retainedStaging:this.engine.retained_staging(), outstanding: this.engine.outstanding(), retainedExports: this.engine.retained_exports(), retainedGraphs: this.engine.retained_graphs(), frame: String(this.engine.frame()), firstFrame: this.firstFrame, resumeFrame: this.resumeFrame });
            } else if (data.type === "stop") {
                if (this.returns.some(buffer => !buffer) || this.graphReturns.some(buffer => !buffer) || this.exportReturns.some(buffer => !buffer) || this.stagingReturns.some(buffer=>!buffer) || this.engine.retained_staging()!==0 || this.engine.outstanding() !== 0 || this.engine.retained_graphs() !== 0 || this.engine.retained_exports() !== 0) throw new Error("stop with outstanding credits/graphs");
                this.stopped = true;
                this.checkMemory();
                const journal = new Uint8Array(this.memory.buffer, this.engine.journal_ptr(), this.engine.journal_len()).slice();
                const summary = { bytes: this.bytes, finalBytes: this.memory.buffer.byteLength,
                    frames: String(this.engine.frame()), callbacks: String(this.engine.callbacks()), firstFrame: this.firstFrame, resumeFrame: this.resumeFrame,
                    returnedStagingBuffers:this.stagingReturns.length,retainedStaging:this.engine.retained_staging(),journalOverflow: this.engine.journal_overflow(), returnedBuffers: this.returns.length, returnedGraphBuffers: this.graphReturns.length, returnedExportBuffers: this.exportReturns.length, retainedExports: this.engine.retained_exports(), retainedGraphs: this.engine.retained_graphs(),
                    marker: this.engine.marker(), heap: [this.engine.heap_allocations(), this.engine.heap_reallocations(), this.engine.heap_deallocations()] };
                const result = this.engine.stop();
                if (result !== 0) throw new Error(`stopped cleanup failed: ${result}`);
                this.checkMemory();
                const semantic = new Uint8Array(this.memory.buffer, this.engine.semantic_ptr(), this.engine.semantic_len()).slice();
                summary.recovered = [this.engine.recovered_pages(), this.engine.recovered_versions(), this.engine.recovered_snapshots(), this.engine.media_references()];
                this.engine.free(); this.engine = null;
                this.port.postMessage({ type: "stopped", ...summary, journal: journal.buffer, semantic: semantic.buffer }, [journal.buffer, semantic.buffer]);
            } else { throw new Error("unknown loop host message"); }
        } catch (error) { this.fail(error); }
    }
    plannerMessage(data) {
        if (data.type === "planner-arm") {
            if (this.armed || this.engine.frame() !== 0n || this.engine.plans() !== 0) throw new Error("planner arm state");
            this.armed = true; this.port.postMessage({type:"planner-armed"});
        } else if (data.type === "planner-stop") {
            if (this.armed || this.engine.frame() !== 128n || this.engine.plans() !== 128 || this.engine.finishes() !== 128 || this.engine.fault_witness() !== 0) throw new Error("planner successful stop state");
            this.checkMemory();
            const summary = {bytes:this.bytes, frame:String(this.engine.frame()), callbacks:String(this.engine.callbacks()),
                plans:this.engine.plans(), finishes:this.engine.finishes(), marker:this.engine.marker(),
                heap:[this.engine.heap_allocations(),this.engine.heap_reallocations(),this.engine.heap_deallocations()]};
            if (this.engine.stop() !== 0) throw new Error("planner stop failed");
            this.checkMemory(); summary.finalBytes=this.memory.buffer.byteLength;
            summary.recovered=[this.engine.recovered_pages(),this.engine.recovered_versions(),this.engine.recovered_snapshots()];
            const semantic=new Uint8Array(this.memory.buffer,this.engine.semantic_ptr(),this.engine.semantic_len()).slice();
            this.engine.free();this.engine=null;this.stopped=true;
            this.port.postMessage({type:"planner-stopped",...summary,semantic:semantic.buffer},[semantic.buffer]);
        } else throw new Error("unknown planner host message");
    }
    process(_inputs, outputs) {
        if (this.stopped || this.failed) return false;
        if (!this.armed && !this.primeRequested) return true;
        try {
            if (outputs.length !== (this.denseMode ? 2 : 1) || outputs[0].length !== 2 || outputs[0].some(channel => channel.length !== 128)) throw new Error("loop quantum shape");
            if (this.firstFrame === null) this.firstFrame = String(currentFrame);
            const priming = this.primeRequested;
            if ((this.graphMode && !priming || this.exportMode && this.engine.frame() === 128n) && this.resumeFrame === null) this.resumeFrame = String(currentFrame);
            const result = this.engine.render(priming ? 1 : this.denseMode ? this.denseFrames : this.stagingMode ? this.stagingFrames : 128);
            this.lastResult = result;
            if (result === 10 && this.canary === "planner" && this.engine.fault_witness() === 3 && this.engine.plans() === 1 && this.engine.finishes() === 0) throw new Error("intentional loop planner failure canary detected");
            if (result === 0xffff0001 && this.canary === "planner-rust-heap" && this.engine.fault_witness() === 4 && this.engine.heap_allocations() > 0 && this.engine.heap_reallocations() > 0 && this.engine.heap_deallocations() > 0) throw new Error("intentional loop planner Rust heap failure canary detected");
            if (result === 0xffff0001 && this.canary === "rust-heap" && this.engine.fault_witness() === 2 && this.engine.heap_allocations() > 0 && this.engine.heap_reallocations() > 0 && this.engine.heap_deallocations() > 0) {
                throw new Error(`intentional loop Rust heap failure canary detected: ${this.engine.heap_allocations()}/${this.engine.heap_reallocations()}/${this.engine.heap_deallocations()}`);
            }
            if (result === 10 && this.canary === "processor" && this.engine.fault_witness() === 1) throw new Error("intentional loop processor failure canary detected");
            if (result !== 0) throw new Error(`loop rendering failed: ${result}`);
            this.checkMemory();
            outputs[0][0].set(this.audio[0]); outputs[0][1].set(this.audio[1]);
            if(this.denseMode)outputs[1][0][0]=1;
            if (priming || this.exportMode || this.stagingMode || this.denseMode || this.plannerMode) {
                if (this.exportMode || this.stagingMode || this.denseMode || this.plannerMode) this.armed = false;
                else { this.primeRequested = false; this.primed = true; }
                this.port.postMessage({ type: "checkpoint", frame: String(this.engine.frame()), browser: String(currentFrame) });
            }
            return true;
        } catch (error) { this.fail(error); return false; }
    }
}

// A separate node observes actual Web Audio buffers in fixed storage.
class AudioProbe extends AudioWorkletProcessor {
    constructor(options) {
        super();
        this.graphMode = options.processorOptions?.graph === true;
        this.denseMode=options.processorOptions?.dense===true;
        this.length=options.processorOptions?.media===true?3840:this.denseMode?(options.processorOptions?.stalls===true?512:384):options.processorOptions?.staging===true?512:256;
        this.left = new Float32Array(this.length); this.right = new Float32Array(this.length);
        this.start = null; this.starts = Array(this.length/128).fill(null); this.count = 0;
        this.port.onmessage = () => {
            const ready = this.count === this.length;
            this.port.postMessage({ type: "probe", ready, start: this.start, starts: this.starts,
                left: ready ? Array.from(this.left) : null, right: ready ? Array.from(this.right) : null });
        };
    }
    process(inputs, outputs) {
        const input = inputs[0], output = outputs[0];
        if (!input || input.length !== 2) return true;
        for (let channel = 0; channel < 2; channel++) output[channel].set(input[channel]);
        if (this.start === null && input[0].some(value => value !== 0)) this.start = String(currentFrame);
        if (this.start !== null && this.count < this.length && (this.denseMode ? inputs[1]?.[0]?.[0] === 1 : !this.graphMode || input[0].some(value => value !== 0))) {
            this.starts[this.count / 128] = String(currentFrame);
            this.left.set(input[0], this.count); this.right.set(input[1], this.count); this.count += 128;
        }
        return true;
    }
}
registerProcessor("loop-fixture", LoopFixture);
registerProcessor("loop-audio-probe", AudioProbe);
