import { acceptExport, exerciseExports } from "./export_script.js";
const Q = 128, RATE = 48000;
const assert = (ok, message) => { if (!ok) throw new Error(`export host: ${message}`); };
export function makeExportHost(Host) {
    return class ExportHost extends Host {
        constructor(mode, module, memory, client, session, exportClient) {
            super(mode, module, memory, client, session, null);
            this.exportMode = true; this.exportClient = exportClient; this.frame = 0n; this.steps = 0;
            this.exportPool = new ArrayBuffer(96); this.exportRequests = []; this.exportResponses = []; this.ingress = []; this.checkpoints = [];
        }
        message(data) {
            if (data.type !== "export-response") return super.message(data);
            try {
                assert(Number.isInteger(data.id) && data.id >= 0 && data.id < 2 && data.buffer instanceof ArrayBuffer && data.buffer.byteLength === 1024, "response envelope");
                acceptExport(this, new Uint8Array(data.buffer));
                this.node.port.postMessage({ type: "export-credit", id: data.id, buffer: data.buffer }, [data.buffer]);
                assert(data.buffer.byteLength === 0, "response return detachment");
            } catch (error) { this.reject(error); }
        }
        async music(epoch) { for (const [kind, at] of [[1,epoch],[2,epoch+4],[3,epoch+12]]) await this.submit(kind, at); }
        async sendExport(packet, expected) {
            const buffer = this.exportPool; assert(buffer?.byteLength === 96, "input pool");
            this.exportPool = null; new Uint8Array(buffer).set(packet);
            const pending = this.wait("export-ingested:0");
            this.node.port.postMessage({ type: "export-request", slot: 0, buffer }, [buffer]);
            assert(buffer.byteLength === 0, "input detachment");
            const reply = await pending;
            assert(reply.buffer instanceof ArrayBuffer && reply.buffer.byteLength === 96 && reply.result === expected && reply.frame === String(this.frame), "input outcome/clock/buffer");
            this.exportPool = reply.buffer; this.ingress.push({ bytes: Array.from(packet), result: reply.result, frame: reply.frame });
        }
        async command(kind, bank, at, channel = 0) {
            assert(this.exportClient.prepare(kind, bank, 1, at, 0n, channel, 0, false) === 0, "request preparation");
            const packet = new Uint8Array(this.memory.buffer, this.exportClient.request_ptr(), 96).slice();
            if (!this.exportRequests.length) { const bad = packet.slice(); bad[95] = 1; await this.sendExport(bad, 4); const foreign = packet.slice(); foreign[8] ^= 128; await this.sendExport(foreign, 9); }
            await this.sendExport(packet, 0); this.exportRequests.push(Array.from(packet));
        }
        async step() {
            const checkpoint = this.wait("checkpoint");
            await this.call("export-step", { frame: String(this.frame) }, "export-stepping");
            if (this.mode === "offline") {
                const pause = this.context.suspend((3 + this.steps * 2) * Q / RATE);
                await this.context.resume(); await Promise.race([pause, this.failure]);
            }
            const cut = await checkpoint;
            assert(BigInt(cut.frame) === this.frame + 128n, "one exact engine step");
            this.frame = BigInt(cut.frame); this.steps++; this.checkpoints.push(cut);
        }
        async drain() {
            const result = await this.call("poll", {}, "polled");
            assert(result.frame === String(this.frame) && result.outstanding === 0 && this.exportClient.outstanding() === 0 && this.client.outstanding() === 0, "complete replies before next step");
            this.polled = result;
        }
        async probeStop() { await this.call("export-stop-probe", {}, "export-stop-refused"); }
        async run() {
            await this.start(); await exerciseExports(this);
            const probe = await this.call("probe", {}, "probe", this.probe.port);
            assert(probe.ready && probe.starts[0] === this.checkpoints[0].browser && probe.starts[1] === this.checkpoints[1].browser && probe.starts[0] === this.polled.firstFrame && probe.starts[1] === this.polled.resumeFrame, "actual downstream clock segments");
            assert(this.exportPool?.byteLength === 96 && this.pool.every(buffer => buffer?.byteLength === 64), "all input buffers returned");
            const summary = await this.call("stop", {}, "stopped");
            assert(summary.returnedBuffers === 10 && summary.returnedExportBuffers === 2 && summary.retainedExports === 0 && summary.frames === "5632" && summary.callbacks === "44", "normal ownership/clock shutdown");
            assert(summary.bytes === summary.finalBytes && this.memory.buffer.byteLength === this.controlBytes && summary.heap.join() === "0,0,0" && summary.journalOverflow === 0 && summary.recovered.join() === "256,16,16,0", "fixed memory/heap/full recovery");
            assert(this.client.marker() === 0x51a17 && summary.marker === 0 && this.ready.shared === false, "ordinary instance isolation");
            const audio = [probe.left, probe.right], samples = audio[0].map((value, i) => [value, audio[1][i]]);
            let offlineAudio = null;
            if (this.mode === "offline") {
                await this.context.resume(); const rendered = await Promise.race([this.finished, this.failure]);
                offlineAudio = [0,1].map(channel => Array.from(rendered.getChannelData(channel)));
                const starts = probe.starts.map(Number);
                for (let channel = 0; channel < 2; channel++) for (let at = 0; at < 90 * Q; at++) {
                    const segment = starts.findIndex(start => at >= start && at < start + Q);
                    assert(offlineAudio[channel][at] === (segment < 0 ? 0 : audio[channel][segment * Q + at - starts[segment]]), "complete offline export output");
                }
            } else { await this.context.close(); }
            return { mode: this.mode, session: this.session, ready: this.ready, summary: { ...summary, journal: undefined, semantic: undefined }, controlBytes: this.controlBytes, controlFinalBytes: this.memory.buffer.byteLength, controlMarker: this.client.marker(),
                ingress: this.ingress, checkpoints: this.checkpoints, starts: probe.starts, offlineAudio,
                data: { samples, exportRequests: this.exportRequests, exportResponses: this.exportResponses, responses: this.responses, midiBytes: Array.from(new Uint8Array(summary.journal)), semanticBytes: Array.from(new Uint8Array(summary.semantic)), recovered: summary.recovered,
                    counts: [this.client.outstanding(), this.exportClient.outstanding(), this.exportClient.retained(), this.polled.retainedExports], storage: this.ready.exportStorage, frames: Number(summary.frames), callbacks: Number(summary.callbacks) } };
        }
    };
}
