const RATE = 48000, Q = 128;
const assert = (condition, message) => { if (!condition) throw new Error(`graph host: ${message}`); };
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

export function makeGraphHost(Host) {
    return class GraphHost extends Host {
        constructor(mode, module, memory, client, session, canary, graphClient) {
            super(mode, module, memory, client, session, canary, true);
            this.graphClient = graphClient;
            this.payloadPool = [new ArrayBuffer(96)];
            this.graphPool = Array.from({ length: 2 }, () => new ArrayBuffer(96));
            this.payloads = []; this.graphRequests = []; this.graphResponses = [];
            this.invalidReplies = 0; this.duplicateReplies = 0;
        }
        message(data) {
            if (data.type !== "graph-response") return super.message(data);
            try {
                assert(Number.isInteger(data.id) && data.id >= 0 && data.id < 4 && data.buffer instanceof ArrayBuffer && data.buffer.byteLength === 128, "response envelope");
                const packet = new Uint8Array(data.buffer), target = new Uint8Array(this.memory.buffer, this.graphClient.response_ptr(), 128);
                const before = this.graphClient.outstanding();
                target.set(packet); target[8] ^= 128;
                assert(this.graphClient.receive() === 9 && this.graphClient.outstanding() === before, "foreign response changed credit");
                this.invalidReplies++;
                target.set(packet);
                assert(this.graphClient.receive() === 0 && this.graphClient.receive() === 11, "valid/duplicate response");
                this.duplicateReplies++;
                this.graphResponses.push(Array.from(packet));
                this.node.port.postMessage({ type: "graph-credit", id: data.id, buffer: data.buffer }, [data.buffer]);
                assert(data.buffer.byteLength === 0, "response return did not detach");
            } catch (error) { this.reject(error); }
        }
        async sendGraph(payload, raw, expected = 0, corrupt = null) {
            const packet = Uint8Array.from(raw);
            if (corrupt !== null) packet[corrupt] ^= 128;
            const slot = payload ? 0 : new DataView(packet.buffer).getUint32(24, true);
            const pool = payload ? this.payloadPool : this.graphPool, buffer = pool[slot];
            assert(buffer?.byteLength === 96, "input buffer unavailable");
            pool[slot] = null; new Uint8Array(buffer).set(packet);
            const type = payload ? "graph-payload" : "graph-request", key = payload ? "graph-payload-ack" : "graph-ingested";
            const pending = this.wait(`${key}:${slot}`);
            this.node.port.postMessage({ type, slot, buffer }, [buffer]);
            assert(buffer.byteLength === 0, "input transfer did not detach");
            const ack = await pending;
            assert(ack.result === expected && ack.buffer instanceof ArrayBuffer && ack.buffer.byteLength === 96, `ingress outcome ${ack.result}, expected ${expected}`);
            pool[slot] = ack.buffer;
            (payload ? this.payloads : this.graphRequests).push({ bytes: Array.from(packet), result: ack.result, frame: ack.frame });
            return ack;
        }
        graphRequest(expected = 0, corrupt = null) {
            return this.sendGraph(false, new Uint8Array(this.memory.buffer, this.graphClient.request_ptr(), 96), expected, corrupt);
        }
        async offlineStep(quanta) {
            if (this.mode !== "offline") return;
            const pause = this.context.suspend(quanta * Q / RATE);
            await this.context.resume(); await Promise.race([pause, this.failure]);
        }
        async run() {
            await this.start();
            await Promise.all([[1, 0], [2, 4], [3, 12]].map(([kind, at]) => this.submit(kind, at)));
            for (let index = 0; index < this.graphClient.payload_count(); index++) {
                assert(this.graphClient.prepare_payload(index, 0, 1, 1n, BigInt(index + 1)) === 0, "payload preparation");
                const record = new Uint8Array(this.memory.buffer, this.graphClient.payload_ptr(), 96);
                if (index === 0) await this.sendGraph(true, record, 4, 7);
                await this.sendGraph(true, record);
            }
            assert(this.graphClient.stage(0, 1, 0, 1n, false) === 0, "stage preparation");
            await this.graphRequest(9, 8);
            assert(this.graphClient.outstanding() === 1, "foreign ingress released source credit");
            await this.graphRequest();
            const checkpoint = this.wait("checkpoint");
            await this.call("prime", {}, "primed");
            await this.offlineStep(2);
            const cut = await checkpoint;
            assert(cut.frame === "1", "prime advanced more than one real sample");
            let polled = await this.call("poll", {}, "polled");
            assert(this.graphResponses.length === 1 && this.graphClient.outstanding() === 0 && this.graphClient.retained_graphs() === 1 && polled.frame === "1", "Ready checkpoint");
            const staged = new DataView(Uint8Array.from(this.graphResponses[0]).buffer);
            assert(staged.getUint32(28, true) === 1 && staged.getBigUint64(88, true) === 1n, "stage validation");
            const id = [76, 80, 84].map(at => staged.getUint32(at, true));
            assert(this.graphClient.activate(...id, 4n, 1n, 0n, false) === 0, "activate preparation");
            await this.graphRequest();
            await this.call("arm", {}, "armed");
            await this.offlineStep(3);
            let actual;
            const deadline = performance.now() + 15000;
            do {
                polled = await this.call("poll", {}, "polled");
                actual = await this.call("probe", {}, "probe", this.probe.port);
                if (actual.ready && this.client.outstanding() === 0 && this.graphClient.outstanding() === 0) break;
                assert(performance.now() < deadline, "output/receipt watchdog"); await delay(5);
            } while (true);
            assert(actual.starts[0] === cut.browser && actual.starts[0] === polled.firstFrame && actual.starts[1] === polled.resumeFrame, "downstream segment clocks");
            assert(this.graphResponses.length === 3 && this.responses.length === 6 && this.graphClient.retained_graphs() === 1, "musical receipts or retained graph");
            assert(this.graphClient.release(...id, false) === 0, "release preparation");
            const release = await this.graphRequest();
            await this.offlineStep(4);
            do {
                polled = await this.call("poll", {}, "polled");
                if (this.graphClient.outstanding() === 0 && polled.outstanding === 0) break;
                assert(performance.now() < deadline, "owning return watchdog"); await delay(5);
            } while (true);
            assert(this.graphResponses.length === 4 && this.graphClient.retained_graphs() === 0 && polled.retainedGraphs === 0, "owning return");
            const returned = new DataView(Uint8Array.from(this.graphResponses[3]).buffer);
            assert(returned.getUint32(28, true) === 2 && returned.getBigUint64(64, true) === BigInt(release.frame) && returned.getUint32(108, true) === 2, "release clock/generation");
            assert([112, 116, 120].map(at => returned.getUint32(at, true)).join() === "3,12,2", "actual retired capacities");
            assert([...this.pool, ...this.payloadPool, ...this.graphPool].every(buffer => buffer instanceof ArrayBuffer && buffer.byteLength === (this.pool.includes(buffer) ? 64 : 96)), "input buffers not recovered");
            const summary = await this.call("stop", {}, "stopped");
            assert(summary.returnedBuffers === 10 && summary.returnedGraphBuffers === 4 && summary.retainedGraphs === 0, "response buffers/graph ownership");
            assert(summary.bytes === summary.finalBytes && this.memory.buffer.byteLength === this.controlBytes && summary.journalOverflow === 0 && summary.heap.every(n => n === 0), "memory/heap/journal");
            assert(BigInt(summary.frames) === 1n + (BigInt(summary.callbacks) - 1n) * 128n && BigInt(summary.frames) >= 257n, "engine callback clock");
            assert(summary.marker === 0 && this.client.marker() === 0x51a17 && summary.recovered.join() === "256,16,16,0", "instance isolation/cleanup");
            const chunks = [actual.left, actual.right], audio = chunks.map(channel => [channel[0], ...channel.slice(128)]);
            const left = [1,2,3,4,1,2,3,4,1,2,3,4,0];
            for (let channel = 0; channel < 2; channel++) for (let at = 0; at < 256; at++) {
                const expected = at === 0 ? 1 : at >= 128 ? left[at - 127] ?? 0 : 0;
                assert(chunks[channel][at] === expected * (channel ? 10 : 1), `actual downstream sample ${channel}/${at}`);
            }
            let offlineAudio = null;
            if (this.mode === "offline") {
                await this.context.resume();
                const rendered = await Promise.race([this.finished, this.failure]);
                offlineAudio = [0, 1].map(channel => Array.from(rendered.getChannelData(channel)));
                const starts = actual.starts.map(frame => Number(BigInt(frame)));
                for (let channel = 0; channel < 2; channel++) for (let at = 0; at < 768; at++) {
                    const segment = starts.findIndex(start => at >= start && at < start + 128);
                    const expected = segment < 0 ? 0 : chunks[channel][segment * 128 + at - starts[segment]];
                    assert(offlineAudio[channel][at] === expected, "OfflineAudioContext graph output differs from downstream probe");
                }
            } else { await this.context.close(); }
            this.node.disconnect(); this.probe.disconnect();
            return { mode: this.mode, session: this.session, ready: this.ready, controlBytes: this.controlBytes, controlFinalBytes: this.memory.buffer.byteLength, controlMarker: this.client.marker(),
                counts: [this.client.outstanding(), this.graphClient.outstanding(), this.graphClient.retained_graphs(), polled.outstanding, polled.retainedGraphs], returnedInputBuffers: [this.pool.length, this.payloadPool.length, this.graphPool.length],
                summary: { ...summary, journal: undefined, semantic: undefined }, checkpoint: cut, starts: actual.starts,
                requests: this.requests, responses: this.responses, payloads: this.payloads, graphRequests: this.graphRequests, graphResponses: this.graphResponses,
                invalidReplies: this.invalidReplies, duplicateReplies: this.duplicateReplies,
                midiBytes: Array.from(new Uint8Array(summary.journal)), semanticBytes: Array.from(new Uint8Array(summary.semantic)), audio, chunks, offlineAudio };
        }
    };
}
