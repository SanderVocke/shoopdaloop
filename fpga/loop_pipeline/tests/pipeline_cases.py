"""Programs and commands only; Rust supplies all expected behavior."""
from copy import deepcopy
from cases import directed as transition_cases, seeded as transition_seeded, case, spec, step, command, play

def connect(value,depths):
    loops=sorted(value['plan']['description']['loops'],key=lambda l:l['id']['slot'],reverse=True)
    instructions=[dict(kind='process_input',port=0,destination=0)]
    memory=[]
    for i,loop in enumerate(loops):
        memory.append(dict(kind='word',value=i))
        instructions.extend([dict(kind='process_logical_playback',loop_id=deepcopy(loop['id']),view=i,destination=i+1),
                             dict(kind='process_logical_capture',loop_id=deepcopy(loop['id']),view=i,source=0)])
    instructions.append(dict(kind='process_output',source=1 if loops else 0,port=0))
    return dict(name=value['name'],plan=value['plan'],program=dict(chunk_frames=1,instructions=instructions,quantum=value['plan']['description']['quantum'],jobs_per_quantum=len(instructions)),
        initial_state={'memory':memory},queue_capacity=depths[5],buffers=len(loops)+1,output_ports=1,media=None,
        steps=[dict(transition=s,writes=[],work=dict(kind='external',value=dict(kind='completed'))) for s in value['steps']])

def directed(depths):
    for value in transition_cases(depths[:3]):
        n=len(value['plan']['description']['loops'])
        if value['plan']['limits']['loop_slots']<=depths[0] and 2*n+2<=depths[3] and n<=depths[4] and n+1<=depths[7]:
            yield connect(value,depths)
    base=connect(case('generation-mismatch-in-signal-program',[spec(0,32)],[step([command(0,1)])],requests=1),depths)
    base['program']['instructions'][1]['loop_id']['generation']+=1
    yield base
    missing=deepcopy(base);missing['name']='out-of-range-loop-in-signal-program'
    missing['program']['instructions'][1]['loop_id']['slot']=depths[0]
    yield missing
    missing=deepcopy(base);missing['name']='hole-in-shared-loop-table'
    missing['program']['instructions'][1]['loop_id']['slot']=1
    missing['plan']['limits']['loop_slots']=min(2,depths[0]);missing['plan']['limits']['events']=2*min(2,depths[0])+1
    yield missing
    for kind in ('invalid_view','memory_full'):
        failed=connect(case('external-job-'+kind,[spec(0,32)],[step([command(0,1)])],requests=1),depths)
        failed['steps'][0]['work']['value']=dict(kind='failed',value={'error':kind})
        yield failed
    wide=connect(case('full-generation-and-wide-position',[spec(0,2**64-32)],[step([command(0,1,play(2**64-64))])],q=16,requests=1),depths)
    wide_id={'slot':0,'generation':2**32-1}
    wide['plan']['description']['loops'][0]['id']=wide_id
    wide['steps'][0]['transition']['commands'][0]['target']=wide_id
    for instruction in wide['program']['instructions']:
        if 'loop_id' in instruction:instruction['loop_id']=wide_id
    yield wide
    # BRAM audio executes with the logical loop stopped, so playback emits Silence.
    for q in (1,16):
        audio=connect(case(f'shared-state-and-real-BRAM-jobs-q{q}',[spec(0,2*q)], [step() for _ in range(4)],q=q,requests=1),depths)
        audio['program']['instructions']=[
            dict(kind='process_input',port=0,destination=0),
            dict(kind='process_gain',ramp=1,source=0,destination=1),
            dict(kind='process_logical_playback',loop_id=spec(0,2*q)['id'],view=0,destination=2),
            dict(kind='process_mix',left=1,right=2,destination=3),
            dict(kind='process_output',source=3,port=0)]
        audio['program']['jobs_per_quantum']=5;audio['buffers']=4
        audio['initial_state']['memory']=[dict(kind='word',value=0),dict(kind='ramp',value=dict(start=-(1<<24),target=1<<24,duration=3*q,elapsed=0))]
        for i,row in enumerate(audio['steps']):row['work']=dict(kind='audio',value={'inputs':[[(-1 if f%2 else 1)*(1234567+i+f) for f in range(q)]]})
        yield audio
    audio=connect(case('BRAM-job-failure-keeps-committed-clock',[spec(0,32)], [step([command(0,1)])],requests=1),depths)
    audio['program']['instructions']=[dict(kind='process_input',port=0,destination=0),
        dict(kind='process_output',source=1,port=0),dict(kind='process_output',source=0,port=0)]
    audio['program']['jobs_per_quantum']=3;audio['buffers']=2
    audio['steps'][0]['work']=dict(kind='audio',value={'inputs':[[1234]*16]})
    yield audio

def seeded(depths,count):
    yield from (connect(value,depths) for value in transition_seeded(depths[:3],count))
