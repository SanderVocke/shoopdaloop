const assert=(ok,message)=>{if(!ok)throw new Error(`staging script: ${message}`);};
const data=packet=>new DataView(Uint8Array.from(packet).buffer);
export function acceptStaging(io,packet){
    const target=new Uint8Array(io.memory.buffer,io.stagingClient.response_ptr(),256),before=io.stagingClient.outstanding();
    target.set(packet);target[8]^=128;assert(io.stagingClient.receive()===9&&io.stagingClient.outstanding()===before,"foreign receipt consumed credit");
    target.set(packet);assert(io.stagingClient.receive()===0&&io.stagingClient.receive()===8,"valid/duplicate receipt");io.stagingResponses.push(Array.from(packet));
}
export async function exerciseStaging(io){
    for(const index of [io.stagingClient.record_count(),0x80000000,0xffffffff])assert(io.stagingClient.prepare(18,0,1,0n,0n,0n,0n,index,false)===7&&io.stagingClient.outstanding()===0,"oversized record index rejected without Wasm32 wrap");
    const rpc=async(kind,bank,args=[0n,0n,0n],index=0,at=null,frames=128)=>{
        const packet=await io.request(kind,bank,args,index,at,frames),v=data(packet);
        assert(v.getUint32(64,true)===2,`domain outcome ${kind}`);return [packet,v];
    };
    const audio=async(value)=>{await rpc(7,0);const [packet,v]=await rpc(8,0);assert(v.getUint32(128,true)===64&&v.getUint32(84,true)===2,"audio read/EOF");for(let frame=0;frame<8;frame++)for(let channel=0;channel<2;channel++)assert(v.getFloat32(144+(frame*2+channel)*4,true)===(value+frame)*(channel?10:1),"immutable uploaded audio");};
    await io.music();await rpc(1,0);await io.probeStop();
    for(const [kind,args,index]of [[2,[0n,8n,64n],0],[3,[0n,0n,0n],0],[4],[5],[6]])await rpc(kind,0,args,index);
    await audio(10);
    for(const [kind,args,index]of [[2,[1n,8n,64n],0],[3,[0n,0n,0n],2],[4],[5]])await rpc(kind,0,args,index);
    await audio(10);await rpc(9,0);const [,snapshot]=await rpc(6,0);assert(snapshot.getBigUint64(88,true)===2n,"new uploaded revision");await audio(100);
    await rpc(13,0);const [,ready]=await rpc(14,0);assert(ready.getBigUint64(88,true)===1n,"audio source ready");await rpc(15,0);await rpc(9,0);await rpc(10,0);
    for(const [kind,args,index]of [[1],[2,[0n,8n,64n],1],[3,[0n,0n,0n],1],[4],[5],[6],[7]])await rpc(kind,1,args,index);
    const [midi,mv]=await rpc(8,1);assert(mv.getUint32(128,true)===64&&mv.getUint32(84,true)===2&&mv.getBigUint64(144,true)===0n&&mv.getUint32(152,true)===3&&midi.slice(156,159).join()==="144,61,90"&&mv.getBigUint64(159,true)===4n&&mv.getUint32(167,true)===37&&midi[171]===240&&midi[207]===247,"raw uploaded MIDI packet boundaries");for(let i=0;i<35;i++)assert(midi[172+i]===i,"complete uploaded SysEx");
    const refusal=await io.request(13,1,[0n,0n,0n],0,null,128),rv=data(refusal);assert(rv.getUint32(64,true)===5&&rv.getUint32(68,true)===522&&rv.getUint32(132,true)===5,"raw MIDI lacks playback checkpoints; preserve owners on domain refusal");
    await rpc(9,1);await rpc(10,1);
    await rpc(17,0);const records=io.stagingClient.record_count();for(let record=0;record<records;record++)await rpc(18,0,[BigInt(record),0n,0n],record);
    let steps=0n;for(;;){const [,progress]=await rpc(19,0);if(progress.getBigUint64(88,true)===1n)break;const total=progress.getBigUint64(96,true);assert(total>steps,"validation progress");steps=total;}
    const [,activated]=await rpc(20,0,[1n,0n,0n]);assert(activated.getBigUint64(88,true)===1n&&activated.getBigUint64(96,true)===1n,"topology activation epochs");await rpc(21,0);
    io.programFrame=io.frame;
    for(const [kind,iteration,at,frames]of [[23,0n,io.programFrame,1],[26,1n,io.programFrame+4n,4],[24,0n,io.programFrame+8n,4]]){const [,v]=await rpc(kind,0,[4294967297n,iteration,0n],0,at,frames);assert(v.getBigUint64(88,true)===at,"exact program play/iteration-seek/stop cut");}
    assert(io.stagingClient.outstanding()===0&&io.stagingClient.retained()===0&&io.stagingClient.generation(0,true)===2&&io.stagingClient.generation(0,false)===2&&io.stagingClient.generation(1,false)===2,"all resource/program owners recycled");
    assert(records===15&&io.frame===6793n&&io.steps===56&&io.programFrame===6784n,"fixed template clocks/counts");
}
export async function stagingBindings(wasm,Client,Bridge,StagingClient){
    const client=new Client(0xffffffffffffffffn),stagingClient=new StagingClient(0xffffffffffffffffn),engine=Bridge.new_staging(0xffffffffffffffffn);
    const io={memory:wasm.memory,stagingClient,frame:0n,steps:0,programFrame:0n,stagingRequests:[],stagingResponses:[],responses:[],samples:[],
        async music(){for(const [kind,at]of [[1,0n],[2,4n],[3,12n]]){assert(client.prepare(kind,at,0n,0)===0,"transition preparation");new Uint8Array(wasm.memory.buffer,engine.request_ptr(),64).set(new Uint8Array(wasm.memory.buffer,client.request_ptr(),64));assert(engine.ingest()===0,"transition ingress");}},
        async request(kind,bank,args=[0n,0n,0n],index=0,at=null,frames=128){
            const requested=at??(kind<=11?0n:this.frame);assert(stagingClient.prepare(kind,bank,stagingClient.generation(bank,kind>=17),requested,...args,index,false)===0,"staging preparation");
            const packet=new Uint8Array(wasm.memory.buffer,stagingClient.request_ptr(),128),target=new Uint8Array(wasm.memory.buffer,engine.staging_request_ptr(),128);
            if(!this.stagingRequests.length){target.set(packet);target[0]^=1;assert(engine.ingest_staging()===2,"malformed ingress");target.set(packet);target[8]^=128;assert(engine.ingest_staging()===9,"foreign ingress");}
            target.set(packet);assert(engine.ingest_staging()===0,"staging ingress");this.stagingRequests.push(Array.from(packet));
            assert(engine.render(frames)===0,"staging render");this.frame=engine.frame();this.steps++;
            if(this.steps===1||kind===23||kind===26||kind===24){const left=new Float32Array(wasm.memory.buffer,engine.output_ptr(0),128),right=new Float32Array(wasm.memory.buffer,engine.output_ptr(1),128);for(let i=0;i<frames;i++)this.samples.push([left[i],right[i]]);}
            for(let i=0;i<10;i++){const code=engine.poll_response();if(!code)break;assert(code===1,"primitive poll");const bytes=new Uint8Array(wasm.memory.buffer,engine.response_ptr(),96);new Uint8Array(wasm.memory.buffer,client.response_ptr(),96).set(bytes);assert(client.receive()===0,"primitive receipt");this.responses.push(Array.from(bytes));}
            for(let i=0;i<2;i++){const code=engine.poll_staging_response();if(!code)break;assert(code===1,"staging poll");acceptStaging(this,new Uint8Array(wasm.memory.buffer,engine.staging_response_ptr(),256));}
            assert(engine.poll_staging_response()===0&&engine.outstanding()===0&&stagingClient.outstanding()===0,"complete receipts");return this.stagingResponses.at(-1);
        },
        async probeStop(){assert(engine.stop()===15,"retained resource allowed stop");}
    };
    const bytes=wasm.memory.buffer.byteLength;await exerciseStaging(io);const counts=[client.outstanding(),stagingClient.outstanding(),stagingClient.retained(),engine.retained_staging()];assert(counts.join()==="0,0,0,0"&&engine.callbacks()===56n,"final binding ownership/clock");
    const midiBytes=Array.from(new Uint8Array(wasm.memory.buffer,engine.journal_ptr(),engine.journal_len()));assert(engine.stop()===0&&wasm.memory.buffer.byteLength===bytes&&[engine.heap_allocations(),engine.heap_reallocations(),engine.heap_deallocations()].join()==="0,0,0","binding fixed memory/heap/stop");
    const result={samples:io.samples,stagingRequests:io.stagingRequests,stagingResponses:io.stagingResponses,responses:io.responses,midiBytes,semanticBytes:Array.from(new Uint8Array(wasm.memory.buffer,engine.semantic_ptr(),engine.semantic_len())),recovered:[engine.recovered_pages(),engine.recovered_versions(),engine.recovered_snapshots(),engine.media_references()],counts,recordCount:stagingClient.record_count(),frames:Number(engine.frame()),callbacks:Number(engine.callbacks()),programFrame:Number(io.programFrame)};
    engine.free();stagingClient.free();client.free();return result;
}
