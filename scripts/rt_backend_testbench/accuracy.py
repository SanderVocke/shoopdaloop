"""A separate float accuracy gate. Fixed/RTL comparisons remain exact."""
import math
import struct
from . import assert_exact

def identity(value):return value["slot"],value["generation"]

LSB=2.0**-24
EPS=2.0**-23
MAX_FIXED=128.0

def f32(value):return struct.unpack('<f',struct.pack('<f',value))[0]

def source_bound(samples):
    values=[s*LSB for s in samples]
    return max(map(abs,values),default=0.0),max((abs(v-f32(v)) for v in values),default=0.0)

def mix(a,b):
    magnitude=min(MAX_FIXED,a[0]+b[0])
    error=a[1]+b[1]+EPS*(a[0]+b[0]+a[1]+b[1])
    return magnitude,error

def gain(a,coefficient):
    magnitude=min(MAX_FIXED,a[0]*coefficient[0])
    error=(coefficient[0]*a[1]+a[0]*coefficient[1]+a[1]*coefficient[1]+LSB
           +EPS*(a[0]+a[1])*(coefficient[0]+coefficient[1]))
    return magnitude,error

class RampBound:
    def __init__(self,magnitude,error):
        self.start=(magnitude,error);self.target=self.start;self.duration=0;self.elapsed=0
    def roundoff(self):
        return LSB+8*EPS*(max(self.start[0],self.target[0])+max(self.start[1],self.target[1]))
    def current(self):
        if not self.duration or self.elapsed>=self.duration:return self.target
        w=self.elapsed/self.duration
        magnitude=(1-w)*self.start[0]+w*self.target[0]
        error=(1-w)*self.start[1]+w*self.target[1]
        return magnitude,error+(self.roundoff() if self.elapsed else 0.0)
    def set(self,target,duration):
        self.start=self.current();self.target=source_bound([target]);self.duration=duration;self.elapsed=0
    def bound(self):
        if not self.duration or self.elapsed>=self.duration:return self.target
        return max(self.start[0],self.target[0]),max(self.start[1],self.target[1])+self.roundoff()
    def advance(self,frames):self.elapsed=min(self.duration,self.elapsed+frames)

def error_budgets(scenario,image,fixed):
    """Propagate magnitude/error bounds in the real compiled arithmetic order.

    State and media ownership decisions come from equal observed logical states;
    this computes error bounds only, never audio or expected FPGA decisions.
    A recording retains the largest bound of its captured chunks. Thus repeated
    read-only playback does not accumulate error with session duration.
    """
    loop_rows={identity(v['id']):v for v in image['loop_views']}
    view_owner={v:(identity(row['id']),c) for row in image['loop_views'] for c,v in enumerate(row['views'])}
    banks={m['name']:m for m in scenario['media']}
    media={};recording={};previous={};exported={}
    for logical in scenario['loops']:
        key=identity(logical['id']);content=logical['content']
        for channel in range(logical['channels']):
            if content:
                first=content['start_chunks']*scenario['chunk_frames'];count=content['length_chunks']*scenario['chunk_frames']
                media[key,channel]=source_bound(banks[content['media']]['channels'][channel][first:first+count])
            else:media[key,channel]=(0.0,0.0)
            recording[key,channel]=(0.0,0.0)
        previous[key]='stopped'
    controls={}
    gain_bindings={identity(b['id']):b['ramps'] for b in image['gains']}
    for node in scenario['nodes']:
        if node['operation']['kind']=='gain':
            magnitude,error=source_bound([node['operation']['initial']])
            for address in gain_bindings[identity(node['id'])]:controls[address]=RampBound(magnitude,error)
    deliveries={o['name']:o['delivered'] for row in fixed['quanta'] for o in row['outcomes'] if o['outcome']=='applied'}
    gain_actions=[a for step in scenario['steps'] for a in step['actions'] if a['kind']=='gain']
    output_budgets=[]
    for step,row in zip(scenario['steps'],fixed['quanta']):
        states={identity(s['id']):s for s in row['states']}
        for action in gain_actions:
            if deliveries.get(action['name'])==row['frame']:
                for address in gain_bindings[identity(action['node'])]:controls[address].set(action['target'],action['duration'])
        for logical in scenario['loops']:
            key=identity(logical['id']);state=states[key]
            if previous[key]=='recording' and state['mode']!='recording':
                for channel in range(logical['channels']):media[key,channel]=recording[key,channel]
            if state['mode']=='recording' and state['capture_frames']==scenario['quantum']:
                for channel in range(logical['channels']):recording[key,channel]=(0.0,0.0)
        for action in step['actions']:
            if action['kind']=='export':
                key=identity(action['target'])
                exported[action['name']]=[recording[key,c] if action['selection']=='recording' else media[key,c]
                                          for c in range(len(loop_rows[key]['views']))]
        buffers={};output=[0.0]*scenario['output_ports']
        for instruction in image['program']['instructions']:
            kind=instruction['kind']
            if kind=='process_input':value=source_bound(step['inputs'][instruction['port']])
            elif kind=='process_silence':value=(0.0,0.0)
            elif kind=='process_logical_playback':
                owner=view_owner[image['initial_state']['memory'][instruction['view']]['value']]
                value=media[owner] if states[owner[0]]['mode']=='playing' else (0.0,0.0)
            elif kind=='process_gain':
                control=controls[instruction['ramp']]
                value=gain(buffers[instruction['source']],control.bound())
                control.advance(scenario['quantum'])
            elif kind=='process_mix':value=mix(buffers[instruction['left']],buffers[instruction['right']])
            elif kind=='process_logical_capture':
                owner=view_owner[image['initial_state']['memory'][instruction['view']]['value']]
                if states[owner[0]]['mode']=='recording':
                    a,b=recording[owner],buffers[instruction['source']]
                    recording[owner]=(max(a[0],b[0]),max(a[1],b[1]))
                continue
            elif kind=='process_output':output[instruction['port']]=buffers[instruction['source']][1];continue
            else:raise ValueError(f'no numeric bound rule for {kind}')
            buffers[instruction['destination']]=value
        output_budgets.append(output)
        previous={key:state['mode'] for key,state in states.items()}
    for logical in scenario['loops']:
        key=identity(logical['id'])
        exported[f"loop:{logical['id']['slot']}:{logical['id']['generation']}"]=[media[key,c] for c in range(logical['channels'])]
    return output_budgets,{name:[bound[1] for bound in values] for name,values in exported.items()}

def compare(scenario,image,fixed,floating,audit):
    assert_exact(audit['result'],fixed)
    assert len(fixed['quanta'])==len(floating['quanta'])==len(scenario['steps'])
    clips=sum(row['counts'][k] for row in audit['quanta'] for k in
              ('clipped_products','clipped_mixes','clipped_subtractions','clipped_negations'))
    intentional=scenario.get('arithmetic','bounded')=='intentional_clipping'
    if clips and not intentional:raise AssertionError(f'{scenario["name"]}: {clips} undeclared saturations')
    if intentional and not clips:raise AssertionError('intentional clipping scenario did not exercise saturation')
    output_budgets,media_budgets=error_budgets(scenario,image,fixed)
    errors=[];maximum_budget=0.0;worst=None;outside=[];per_quantum=[]
    def samples(qa,fa,budgets,path):
        nonlocal maximum_budget,worst
        assert qa['encoding']=='q824' and fa['encoding']=='float'
        assert len(qa['channels'])==len(fa['channels'])==len(budgets)
        current=[]
        for channel,(qs,fs,budget) in enumerate(zip(qa['channels'],fa['channels'],budgets)):
            assert len(qs)==len(fs)
            maximum_budget=max(maximum_budget,budget)
            for sample,(q,f) in enumerate(zip(qs,fs)):
                assert math.isfinite(f),'nonfinite float output'
                error=q*LSB-f;where=f'{path}.channels[{channel}][{sample}]'
                if worst is None or abs(error)>worst['absolute_error']:worst=dict(path=where,absolute_error=abs(error))
                if not intentional and abs(error)>budget+1e-15:
                    raise AssertionError(f'{where}: float error {error} exceeds derived bound {budget}')
                if not -128<=f<=2147483647*LSB:outside.append(where)
                current.append(error);errors.append(error)
        return current
    for i,(qa,fa,budgets) in enumerate(zip(fixed['quanta'],floating['quanta'],output_budgets)):
        assert_exact({k:v for k,v in qa.items() if k!='outputs'},{k:v for k,v in fa.items() if k!='outputs'})
        current=samples(qa['outputs'],fa['outputs'],budgets,f'quanta[{i}].outputs')
        per_quantum.append(sum(current)/max(1,len(current)))
    assert len(fixed['media'])==len(floating['media'])
    for qa,fa in zip(fixed['media'],floating['media']):
        assert (qa['name'],qa['frames'])==(fa['name'],fa['frames'])
        samples(qa['samples'],fa['samples'],media_budgets[qa['name']],f'media[{qa["name"]}]')
    n=len(errors)
    return dict(name=scenario['name'],classification='intentional_clipping' if intentional else 'bounded',samples=n,
        max_abs=max(map(abs,errors),default=0.0),rms=math.sqrt(sum(e*e for e in errors)/max(1,n)),signed_bias=sum(errors)/max(1,n),
        drift_first_to_last_quantum=(per_quantum[-1]-per_quantum[0]) if per_quantum else 0.0,
        max_derived_error_budget=maximum_budget,worst=worst,saturating_operations=clips,
        clipping_quanta=[row for row in audit['quanta'] if row['counts']['first_clip_operation'] is not None],
        float_samples_outside_fixed_range=outside)
