"""Programs and commands only; Rust supplies all expected behavior."""
from copy import deepcopy

def connect(value,depths):
    loops=sorted(value['plan']['description']['loops'],key=lambda l:l['id']['slot'],reverse=True)
    instructions=[dict(kind='process_input',port=0,destination=0)]
    memory=[]
    for i,loop in enumerate(loops):
        memory.append(dict(kind='word',value=i))
        instructions.extend([dict(kind='process_logical_playback',loop_id=deepcopy(loop['id']),view=i,destination=i+1),
                             dict(kind='process_logical_capture',loop_id=deepcopy(loop['id']),view=i,source=0)])
    instructions.append(dict(kind='process_output',source=1 if loops else 0,port=0))
    return dict(name=value['name'],plan=value['plan'],program=dict(chunk_frames=1,instructions=instructions,quantum=value['plan']['description']['quantum'],jobs_per_quantum=len(instructions)+len(loops)),
        initial_state={'memory':memory},queue_capacity=depths[5],buffers=len(loops)+1,output_ports=1,media=None,
        steps=[dict(transition=s,writes=[],work=dict(kind='audio',value=dict(inputs=[[0]*value['plan']['description']['quantum']]))) for s in value['steps']])
