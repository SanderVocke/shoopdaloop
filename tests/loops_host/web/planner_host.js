const assert = (ok, message) => { if (!ok) throw new Error(`planner host: ${message}`); };
export function plannerBindings(wasm, Canary) {
    let rejected = false;
    try { new Canary(3); } catch (error) { rejected = error === 5; }
    assert(rejected, "unknown fault constructor");
    const engine = new Canary(0), memory = wasm.memory, bytes = memory.buffer.byteLength;
    try {
        assert(engine.render(0) === 0 && engine.plans() === 0 && engine.finishes() === 0 && engine.callbacks() === 0n, "zero call inert");
        assert(engine.render(128) === 0, "real resolver control render");
        const samples = Array.from({length:128}, (_,i) => [0,1].map(c => new Float32Array(memory.buffer,engine.output_ptr(c),128)[i]));
        assert(engine.stop() === 0, "control stop");
        const result = {samples,semanticBytes:Array.from(new Uint8Array(memory.buffer,engine.semantic_ptr(),engine.semantic_len())),
            recovered:[engine.recovered_pages(),engine.recovered_versions(),engine.recovered_snapshots()],
            frames:Number(engine.frame()),callbacks:Number(engine.callbacks()),plans:engine.plans(),finishes:engine.finishes()};
        assert(memory.buffer.byteLength === bytes && [engine.heap_allocations(),engine.heap_reallocations(),engine.heap_deallocations()].join() === "0,0,0", "control memory/heap");
        for (const fault of [1,2]) {
            const failed = new Canary(fault), before = memory.buffer.byteLength;
            try {
                assert(failed.render(128) === (fault === 1 ? 10 : 0xffff0001) && failed.fault_witness() === (fault === 1 ? 3 : 4), "Rust planner fault origin");
                const heap = [failed.heap_allocations(),failed.heap_reallocations(),failed.heap_deallocations()];
                assert(fault === 1 ? heap.every(v=>v===0) : heap.every(v=>v>0), "planner heap witnesses");
                assert(failed.plans() === (fault === 1 ? 1 : 128) && failed.finishes() === (fault === 1 ? 0 : 128), "planner lifecycle counts");
                assert(failed.render(1) === 10 && memory.buffer.byteLength === before, "permanent fault/fixed memory");
                assert(failed.stop() === 0, "explicit fault abandonment");
            } finally { failed.free(); }
        }
        return result;
    } finally { engine.free(); }
}
export function makePlannerHost(Host) {
    return class PlannerHost extends Host {
        async run() {
            await this.start();
            const checkpoint = this.wait("checkpoint");
            await this.call("planner-arm", {}, "planner-armed");
            if (this.mode === "offline") {
                const pause = this.context.suspend(384 / 48000);
                await this.context.resume(); await Promise.race([pause, this.failure]);
            }
            const cut = await checkpoint;
            assert(cut.frame === "128", "control engine cut");
            let probe;
            const deadline = performance.now() + 15000;
            do {
                probe = await this.call("probe", {}, "probe", this.probe.port);
                if (probe.ready) break;
                assert(performance.now() < deadline, "planner downstream watchdog");
                await new Promise(resolve=>setTimeout(resolve,5));
            } while (true);
            assert(probe.start === cut.browser, "acknowledged downstream clock");
            const summary = await this.call("planner-stop", {}, "planner-stopped");
            assert(summary.frame === "128" && summary.callbacks === "1" && summary.plans === 128 && summary.finishes === 128, "successful lifecycle");
            assert(summary.bytes === summary.finalBytes && summary.bytes === this.ready.bytes && summary.heap.join() === "0,0,0" && summary.recovered.join() === "8,4,4", "fixed memory/complete recovery");
            assert(summary.marker === 0 && this.ready.marker === 0 && this.client.marker() === 0x51a17 && this.memory.buffer.byteLength === this.controlBytes, "separate ordinary memories");
            const audio = [probe.left,probe.right];
            for (let c=0;c<2;c++) for(let i=0;i<256;i++) assert(audio[c][i] === (i<128 ? (i+1)*(c===0?1:10) : 0), "complete downstream control output");
            let offlineAudio = null;
            if (this.mode === "offline") {
                await this.context.resume();
                const rendered = await Promise.race([this.finished,this.failure]);
                offlineAudio = [0,1].map(c=>Array.from(rendered.getChannelData(c)));
                const start = Number(cut.browser);
                for(let c=0;c<2;c++) for(let i=0;i<512;i++) assert(offlineAudio[c][i] === (i>=start && i<start+256 ? audio[c][i-start] : 0), "complete planner offline output");
            } else await this.context.close();
            return {mode:this.mode,ready:this.ready,summary:{...summary,semantic:undefined},audio,offlineAudio,start:cut.browser,
                controlBytes:this.controlBytes,controlFinalBytes:this.memory.buffer.byteLength,controlMarker:this.client.marker(),
                data:{samples:Array.from({length:128},(_,i)=>audio.map(c=>c[i])),semanticBytes:Array.from(new Uint8Array(summary.semantic)),
                    recovered:summary.recovered,frames:128,callbacks:1,plans:summary.plans,finishes:summary.finishes}};
        }
    };
}
