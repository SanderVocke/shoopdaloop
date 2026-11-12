"""64 independent logical loops with the same audio graph as the media benchmark."""
import random
import fpga_testbench as wire
from cases import case, spec, step, command, play


def document(kind, value):
    return dict(format_version=3, payload=dict(kind=kind, value=value))


def playback64(reference, seed=20261110, *, length_chunks=16, quanta=4):
    rng = random.Random(seed)
    nodes, views, samples = [], [], []
    for track in range(8):
        mixed = None
        for loop in range(8):
            view = track*8+loop
            nodes.append(dict(kind='playback', view=view, length_chunks=length_chunks, position_chunks=view % length_chunks, playing=True))
            nodes.append(dict(kind='gain', input=len(nodes)-1, gain=8388608))
            gained = len(nodes)-1
            if mixed is None:
                mixed = gained
            else:
                nodes.append(dict(kind='mix', left=mixed, right=gained))
                mixed = len(nodes)-1
            start = len(samples)//32
            samples.extend(rng.randint(-(1 << 24), (1 << 24)-1) for _ in range(length_chunks*32))
            views.append(dict(chunks=list(range(start, start+length_chunks)), frames=length_chunks*32))
        nodes.append(dict(kind='output', input=mixed, port=track))
    compiled = wire.run_reference('compile', document('graph', dict(quantum=32, chunk_frames=32, nodes=nodes)), executable=reference)['payload']['value']
    for node, instruction in zip(nodes, compiled['program']['instructions']):
        if node['kind'] == 'playback':
            replacement = dict(kind='process_logical_playback', loop_id=spec(node['view'], 512)['id'],
                               view=instruction['view'], destination=instruction['destination'])
            instruction.clear()
            instruction.update(replacement)
    transitions = case('coordinated-8x8-playback-q32', [spec(i, length_chunks*32) for i in range(64)],
        [step([command(i, i+1, play((i % length_chunks)*32)) for i in range(64)])]+[step() for _ in range(quanta-1)],
        q=32, requests=128)
    return dict(name=transitions['name'], plan=transitions['plan'], program=compiled['program'],
                initial_state=dict(memory=compiled['initial_state']), queue_capacity=256,
                buffers=len(nodes), output_ports=8,
                media=dict(chunk_frames=32, chunk_limit=64*length_chunks, samples=samples, free_chunks=[], views=views),
                steps=[dict(transition=row, writes=[], work=dict(kind='audio', value=dict(inputs=[])))
                       for row in transitions['steps']])


def streaming64(reference):
    value=playback64(reference,length_chunks=64,quanta=24)
    value['name']='coordinated-8x8-streaming-metadata'
    for quantum in (8,16):
        value['steps'][quantum]['transition']['commands']=[
            command(i,quantum*64+i+1,dict(kind='seek',position=((i*7+quantum*3)%64)*32,retrigger=True))
            for i in range(64)]
    return value


def mixed64(reference):
    rng=random.Random(20261110)
    loops=[spec(i,512 if i<48 else 0) for i in range(64)]
    nodes=[dict(kind='input',port=0)]
    views=[];samples=[]
    for track in range(8):
        mixed=None
        for local in range(8):
            i=track*8+local
            nodes.append(dict(kind='logical_playback',loop_id=loops[i]['id'],view=i))
            nodes.append(dict(kind='gain',input=len(nodes)-1,gain=8388608))
            gained=len(nodes)-1
            if mixed is None:mixed=gained
            else:
                nodes.append(dict(kind='mix',left=mixed,right=gained));mixed=len(nodes)-1
            start=len(samples)//32
            chunks=16 if i<48 else 0
            samples.extend(rng.randint(-(1<<24),(1<<24)-1) for _ in range(chunks*32))
            views.append(dict(chunks=list(range(start,start+chunks)),frames=chunks*32))
        nodes.append(dict(kind='output',input=mixed,port=track))
    for i in range(64):nodes.append(dict(kind='logical_capture',loop_id=loops[i]['id'],view=i,input=0))
    for i in range(32,40):views.append(dict(chunks=views[i]['chunks'][:2],frames=64,range=dict(offset=0)))
    start=len(samples)//32
    samples.extend([0]*(32*32))
    free=list(range(start,start+32))
    compiled=wire.run_reference('compile',document('graph',dict(quantum=32,chunk_frames=32,nodes=nodes)),executable=reference)['payload']['value']
    rows=[step([command(i,i+1,dict(kind='record') if i>=32 else play()) for i in range(64)])]+[step() for _ in range(23)]
    rows[12]=step([command(i,1000+i) for i in range(32,64)])
    rows[13]=step([command(i,2000+i,dict(kind='record')) for i in range(32,64)])
    timing=case('coordinated-8x8-record-play-retake',loops,rows,q=32,requests=128)
    return dict(name=timing['name'],plan=timing['plan'],program=compiled['program'],
        initial_state=dict(memory=compiled['initial_state']),queue_capacity=384,buffers=len(nodes),output_ports=8,
        media=dict(chunk_frames=32,chunk_limit=2048,samples=samples,free_chunks=free,views=views),
        steps=[dict(transition=row,writes=[],work=dict(kind='audio',value=dict(inputs=[
            [((q+1)*40000+f*73)*(-1 if f%3==0 else 1) for f in range(32)]]))) for q,row in enumerate(rows)])
