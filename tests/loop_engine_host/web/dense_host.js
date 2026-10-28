const assert=(ok,message)=>{if(!ok)throw new Error(`dense host: ${message}`);};
const Q=128,RATE=48000;
const commands=[[1,0],[2,1],[4,64],[4,130]];
export async function denseBindings(wasm,Client,Bridge){
    const client=new Client(0xffffffffffffffffn),bridge=Bridge.new_dense(0xffffffffffffffffn);
    const bytes=wasm.memory.buffer.byteLength,requests=[],responses=[],samples=[];
    try{
        const submit=(kind,at,flags=0)=>{assert(client.prepare(kind,BigInt(at),0n,flags)===0,"binding preparation");const request=new Uint8Array(wasm.memory.buffer,client.request_ptr(),64);requests.push(Array.from(request));new Uint8Array(wasm.memory.buffer,bridge.request_ptr(),64).set(request);assert(bridge.ingest()===0,"binding ingress");};
        const step=frames=>{assert(bridge.render(frames)===0,"binding rendering");for(let i=0;i<frames;i++)samples.push([0,1].map(channel=>new Float32Array(wasm.memory.buffer,bridge.output_ptr(channel),128)[i]));assert(wasm.memory.buffer.byteLength===bytes&&[bridge.heap_allocations(),bridge.heap_reallocations(),bridge.heap_deallocations()].join()==="0,0,0","binding fixed memory/heap");};
        for(const[kind,at]of commands)submit(kind,at);
        assert(client.prepare(3,0n,0n,0)===10,"normal credit saturation");step(128);step(1);
        assert(bridge.outstanding()===4&&bridge.stop()===15,"unread results retained");submit(3,129,4);step(2);
        assert(bridge.outstanding()===5,"recovery independent of four unread normal results");
        while(true){const result=bridge.poll_response();if(result===0)break;assert(result===1,"binding polling");const response=new Uint8Array(wasm.memory.buffer,bridge.response_ptr(),96);responses.push(Array.from(response));new Uint8Array(wasm.memory.buffer,client.response_ptr(),96).set(response);assert(client.receive()===0&&client.receive()===11,"valid then duplicate receipt");}
        assert(responses.length===10&&client.outstanding()===0&&bridge.outstanding()===0&&bridge.journal_overflow()===0,"complete binding results");
        const midiBytes=Array.from(new Uint8Array(wasm.memory.buffer,bridge.journal_ptr(),bridge.journal_len()));assert(bridge.stop()===0,"binding cleanup");
        const semanticBytes=Array.from(new Uint8Array(wasm.memory.buffer,bridge.semantic_ptr(),bridge.semantic_len()));
        const recovered=[bridge.recovered_pages(),bridge.recovered_versions(),bridge.recovered_snapshots(),bridge.media_references()];assert(recovered.join()==="256,16,16,0"&&wasm.memory.buffer.byteLength===bytes,"binding full recovery/fixed memory");
        return{samples,requests,responses,midiBytes,semanticBytes,recovered,counts:[client.outstanding(),bridge.outstanding(),bridge.retained_graphs(),bridge.retained_exports()]};
    }finally{bridge.free();client.free();}
}
export function makeDenseHost(Host){return class DenseHost extends Host{
    constructor(...args){super(...args);this.denseMode=true;this.offlineFrames=8*Q;this.frame=0n;this.checkpoints=[];this.states=[];}
    async submitDense(kind,at,flags=0){
        assert(this.client.prepare(kind,BigInt(at),0n,flags)===0,"preparation");const request=new Uint8Array(this.memory.buffer,this.client.request_ptr(),64),slot=new DataView(request.buffer,request.byteOffset,64).getUint32(32,true),buffer=this.pool[slot];assert(buffer?.byteLength===64,"input pool");this.pool[slot]=null;new Uint8Array(buffer).set(request);this.requests.push(Array.from(request));const pending=this.wait(`ingested:${slot}`);this.node.port.postMessage({type:"request",slot,buffer},[buffer]);assert(buffer.byteLength===0,"input detached");const reply=await pending;assert(reply.result===0&&reply.buffer instanceof ArrayBuffer&&reply.buffer.byteLength===64,"input returned");this.pool[slot]=reply.buffer;
    }
    async step(frames){
        const pending=this.wait("checkpoint");await this.call("dense-step",{frame:String(this.frame),frames},"dense-stepping");
        if(this.mode==="offline"){const pause=this.context.suspend((3+this.checkpoints.length*2)*Q/RATE);await this.context.resume();await Promise.race([pause,this.failure]);}
        const cut=await pending;assert(BigInt(cut.frame)===this.frame+BigInt(frames),"engine cut");this.frame=BigInt(cut.frame);this.checkpoints.push({...cut,frames});
    }
    async run(){
        await this.start();for(const[kind,at]of commands)await this.submitDense(kind,at);
        assert(this.client.prepare(3,0n,0n,0)===10,"normal credits full");
        await this.step(128);await this.step(1);
        this.states.push(await this.call("dense-state"));assert(this.states[0].frame==="129"&&this.states[0].outstanding===4&&this.states[0].stop===15&&this.responses.length===0,"normal results genuinely stalled");
        await this.submitDense(3,129,4);await this.step(2);
        this.states.push(await this.call("dense-state"));assert(this.states[1].frame==="131"&&this.states[1].outstanding===5&&this.states[1].stop===15,"recovery with no normal credit returned");
        const poll=await this.call("poll",{},"polled"),probe=await this.call("probe",{},"probe",this.probe.port);
        assert(this.responses.length===10&&this.client.outstanding()===0&&poll.outstanding===0,"all actual results returned");assert(probe.ready&&probe.starts.join()===this.checkpoints.map(c=>c.browser).join(),"downstream stereo/sideband clocks");
        const chunks=[probe.left,probe.right],samples=[];for(let segment=0;segment<3;segment++)for(let i=0;i<Q;i++){if(i<this.checkpoints[segment].frames)samples.push(chunks.map(channel=>channel[segment*Q+i]));else assert(chunks.every(channel=>channel[segment*Q+i]===0),"short callback tail");}
        assert(this.pool.every(buffer=>buffer?.byteLength===64),"all input buffers returned");const summary=await this.call("stop",{},"stopped");
        assert(summary.frames==="131"&&summary.callbacks==="3"&&summary.returnedBuffers===10&&summary.bytes===summary.finalBytes&&summary.heap.join()==="0,0,0"&&summary.journalOverflow===0&&summary.recovered.join()==="256,16,16,0","fixed memory and complete cleanup");assert(this.memory.buffer.byteLength===this.controlBytes&&this.client.marker()===0x51a17&&summary.marker===0&&this.ready.shared===false,"ordinary independent memories");
        let offlineAudio=null;if(this.mode==="offline"){await this.context.resume();const output=await Promise.race([this.finished,this.failure]);offlineAudio=[0,1].map(channel=>Array.from(output.getChannelData(channel)));for(let channel=0;channel<2;channel++)for(let at=0;at<this.offlineFrames;at++){const segment=probe.starts.findIndex(start=>at>=Number(start)&&at<Number(start)+Q);assert(offlineAudio[channel][at]===(segment<0?0:chunks[channel][segment*Q+at-Number(probe.starts[segment])]),"complete offline output");}}else await this.context.close();
        return{mode:this.mode,session:this.session,ready:this.ready,summary:{...summary,journal:undefined,semantic:undefined},controlBytes:this.controlBytes,controlFinalBytes:this.memory.buffer.byteLength,controlMarker:this.client.marker(),checkpoints:this.checkpoints,states:this.states,starts:probe.starts,chunks,offlineAudio,data:{samples,requests:this.requests,responses:this.responses,midiBytes:Array.from(new Uint8Array(summary.journal)),semanticBytes:Array.from(new Uint8Array(summary.semantic)),recovered:summary.recovered,counts:[this.client.outstanding(),poll.outstanding,poll.retainedGraphs,poll.retainedExports]}};
    }
};}
