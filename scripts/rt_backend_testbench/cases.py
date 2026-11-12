"""Deterministic shared inputs. This module computes no backend results."""
from copy import deepcopy
import random

def lid(slot,generation=1):return dict(slot=slot,generation=generation)
def ep(node=None,port=0):return dict(node=lid(node) if node is not None else None,port=port)
def edge(source,destination):return dict(source=source,destination=destination)
def action(name,target,at,kind,*,position=0,retrigger=False,source=None,skip=0):
    operation=dict(kind=kind)
    if kind in ('play','seek'):operation.update(position=position,retrigger=retrigger)
    timing=dict(kind='at') if source is None else dict(kind='after_ticks',source=lid(source),skip=skip)
    return dict(kind='transition',name=name,target=lid(target),at=at,timing=timing,operation=operation)
def step(q=32,ports=1,actions=(),collect=True,ordinal=0):
    return dict(inputs=[[(-1 if (i+port)%3==0 else 1)*(1000001+ordinal*1013+port*919+i*31) for i in range(q)] for port in range(ports)],actions=list(actions),collect_completed=collect)
def base(name,*,loops=1,channels=1,q=32,cf=None,inputs=None,outputs=None,requests=8):
    value=dict(format_version=1,name=name,quantum=q,chunk_frames=cf or q,input_ports=channels if inputs is None else inputs,
               output_ports=channels if outputs is None else outputs,request_capacity=requests,loops=[],nodes=[],connections=[],media=[],steps=[])
    for i in range(loops):
        value['loops'].append(dict(id=lid(i),channels=channels,sync_source=None,repeat_sync=False,content=None))
        value['nodes'] += [dict(id=lid(2*i),operation=dict(kind='playback',target=lid(i))),dict(id=lid(2*i+1),operation=dict(kind='capture',target=lid(i)))]
        for c in range(channels):
            if c<value['input_ports']:value['connections'].append(edge(ep(None,c),ep(2*i+1,c)))
            if c<value['output_ports']:value['connections'].append(edge(ep(2*i,c),ep(None,c)))
    value['nodes'].reverse()
    return value

def loaded(value,frames=64,seed=20261112):
    rng=random.Random(seed)
    for row in value['loops']:
        name=f"media-{row['id']['slot']}"
        samples=[[rng.randint(-16777216,16777215) for _ in range(frames)] for _ in range(row['channels'])]
        value['media'].append(dict(name=name,channels=samples));row['content']=dict(media=name,start_chunks=0,length_chunks=frames//value['chunk_frames'])
    return value

def directed():
    # Three ordered sources expose intermediate saturation even when float's final
    # result lies inside the fixed range.
    for label,values in [('saturating',[100,100,-100]),('cancelling',[100,-100,100])]:
        value=base('ordered-fan-in-'+label,loops=0,inputs=3,outputs=1)
        if label=='saturating':value['arithmetic']='intentional_clipping'
        value['connections']=[edge(ep(None,p),ep(None,0)) for p in (2,0,1)]
        value['steps']=[dict(inputs=[[v<<24]*32 for v in values],actions=[],collect_completed=True) for _ in range(3)]
        yield value
    value=base('successive-gains-keep-intermediate-rounding',loops=0)
    value['nodes']=[dict(id=lid(i),operation=dict(kind='gain',channels=1,initial=5592405)) for i in range(2)]
    value['connections']=[edge(ep(),ep(0)),edge(ep(0),ep(1)),edge(ep(1),ep())]
    value['steps']=[step(ordinal=i) for i in range(5)]
    yield value
    value=base('stereo-capture-retake-retains-old-export',channels=2)
    controls={0:[action('record',0,0,'record')],2:[action('stop',0,64,'stop')],3:[action('play',0,96,'play')],
              4:[dict(kind='export',name='old-take',target=lid(0),selection='playback')],5:[action('retake',0,160,'record')],
              7:[action('new-play',0,224,'play')],8:[dict(kind='export',name='new-take',target=lid(0),selection='latest')]}
    value['steps']=[step(ports=2,ordinal=i,actions=controls.get(i,())) for i in range(11)]
    yield value
    value=base('live-recording-prefix-is-immutable')
    controls={0:[action('record',0,0,'record')],1:[dict(kind='export',name='prefix',target=lid(0),selection='recording')],3:[action('play',0,96,'play')]}
    value['steps']=[step(ordinal=i,actions=controls.get(i,())) for i in range(8)]
    yield value
    value=base('shared-initial-ranges-survive-retake-and-slot-reuse',loops=2)
    value['media']=[dict(name='shared',channels=[[(-1 if i%3==0 else 1)*(1000001+i*137) for i in range(96)]])]
    for i,row in enumerate(value['loops']):row['content']=dict(media='shared',start_chunks=i,length_chunks=2)
    controls={0:[action('play-a',0,0,'play'),action('play-b',1,0,'play')],2:[action('retake-b',1,64,'record')],4:[action('resume-b',1,128,'play')],6:[action('stop-b',1,192,'stop')]}
    value['steps']=[step(ordinal=i,actions=controls.get(i,())) for i in range(10)]
    yield value
    value=loaded(base('absolute-time-rounding-and-same-frame-supersession'),128)
    controls={0:[action('later-play',0,17,'play'),action('superseded-play',0,79,'play',position=32),action('last-stop',0,79,'stop')],5:[action('resume',0,160,'play',position=64,retrigger=True)]}
    value['steps']=[step(ordinal=i,actions=controls.get(i,())) for i in range(10)]
    yield value
    value=loaded(base('cancel-before-absolute-delivery'),64)
    controls={0:[action('future',0,96,'play')],1:[dict(kind='cancel',name='cancel-future',request='future')],4:[action('play',0,128,'play')]}
    value['steps']=[step(ordinal=i,actions=controls.get(i,())) for i in range(8)]
    yield value
    value=loaded(base('source-chain-clock-order-is-independent-of-audio-order',loops=3,requests=8),64)
    for i in (1,2):value['loops'][i]['sync_source']=lid(i-1)
    controls={0:[action('source',0,0,'play'),action('follower',1,0,'play',source=0,skip=1),action('last',2,0,'play',source=1)],
              1:[dict(kind='cancel',name='cancel-last',request='last')],6:[action('restart-last',2,192,'play',source=1,skip=1)]}
    value['steps']=[step(ordinal=i,actions=controls.get(i,())) for i in range(13)]
    yield value
    for count in (1,3):
        value=loaded(base('self-sync' if count==1 else 'legal-sync-cycle',loops=count),64)
        for i,row in enumerate(value['loops']):row['sync_source']=lid((i+1)%count);row['repeat_sync']=True
        controls={0:[action(f'play-{i}',i,0,'play') for i in range(count)],3:[action('delayed-retrigger',0,96,'seek',position=32,retrigger=True,source=(1%count),skip=1)]}
        value['steps']=[step(ordinal=i,actions=controls.get(i,())) for i in range(14)]
        yield value
    value=loaded(base('unread-completions-retain-admission-credit',requests=2),64)
    controls={0:[action('first',0,0,'play'),action('second',0,0,'play',position=32)],1:[action('refused-stop',0,32,'stop')],2:[action('accepted-stop',0,64,'stop')]}
    value['steps']=[step(ordinal=i,actions=controls.get(i,()),collect=i!=1) for i in range(5)]
    yield value
    value=loaded(base('stale-loop-generation-is-refused'),64)
    invalid=action('stale',0,0,'play');invalid['target']=lid(0,2)
    value['steps']=[step(actions=[invalid]),step()]
    yield value

def seeded(count,seed=20261112):
    rng=random.Random(seed)
    for case in range(count):
        value=loaded(base(f'seed-{seed}-{case}',loops=rng.randint(1,3)),64)
        serial=0
        for i in range(24):
            actions=[]
            if rng.randrange(3)==0:
                target=rng.randrange(len(value['loops']));kind=rng.choice(('play','stop','seek','record'))
                actions.append(action(f'action-{serial}',target,i*32,kind,position=rng.choice((0,32)),retrigger=bool(rng.randrange(2))))
                serial+=1
            if i==23:
                actions=[action(f'final-stop-{target}',target,i*32,'stop') for target in range(len(value['loops']))]
            value['steps'].append(step(ordinal=i,actions=actions))
        yield value

def allocation_boundary(block_chunks,chunk_frames,q=32):
    value=base('stereo-block-boundary-retake-retains-old-export',channels=2,q=q,cf=chunk_frames)
    end=(block_chunks*chunk_frames+q-1)//q+3
    controls={0:[action('record',0,0,'record')],end:[action('play',0,end*q,'play')],
        end+1:[dict(kind='export',name='old-take',target=lid(0),selection='playback')],
        end+2:[action('retake',0,(end+2)*q,'record')],end+5:[action('new-play',0,(end+5)*q,'play')],
        end+6:[dict(kind='export',name='new-take',target=lid(0),selection='latest')]}
    value['steps']=[step(q=q,ports=2,ordinal=i,actions=controls.get(i,())) for i in range(end+8)]
    return value

def benchmark64(seed=20261110):
    value=base('coordinated-8x8-production-playback-q32',loops=64,inputs=0,outputs=8,requests=128)
    loaded(value,512,seed)
    value['connections']=[]
    for i in range(64):
        gain=128+i
        value['nodes'].append(dict(id=lid(gain),operation=dict(kind='gain',channels=1,initial=8388608)))
        value['connections'] += [edge(ep(2*i),ep(gain)),edge(ep(gain),ep(None,i//8))]
    value['steps']=[step(ports=0,actions=[action(f'play-{i}',i,0,'play',position=(i%16)*32) for i in range(64)])]+[step(ports=0) for _ in range(3)]
    return value

def long_sessions():
    value=loaded(base('long-readonly-playback-and-interrupted-ramps',inputs=0),512)
    value['nodes'].append(dict(id=lid(2),operation=dict(kind='gain',channels=1,initial=8388608)))
    value['connections']=[edge(ep(0),ep(2)),edge(ep(2),ep())]
    settings={100:(-12582912,4097),150:(15100001,257),300:(4194305,33),301:(-8388609,1),800:(16777216,0)}
    for i in range(1024):
        actions=[action('play',0,0,'play')] if i==0 else []
        if i in settings:
            target,duration=settings[i];actions.append(dict(kind='gain',name=f'gain-{i}',node=lid(2),at=i*32,target=target,duration=duration))
        value['steps'].append(step(ports=0,actions=actions))
    yield value
    value=base('long-repeated-ramp-interruption',loops=0)
    value['nodes']=[dict(id=lid(0),operation=dict(kind='gain',channels=1,initial=16777217))]
    value['connections']=[edge(ep(),ep(0)),edge(ep(0),ep())]
    for i in range(256):
        target=15100001 if i%2 else -12582913
        value['steps'].append(step(ordinal=i,actions=[dict(kind='gain',name=f'gain-{i}',node=lid(0),at=i*32,target=target,duration=129)]))
    yield value

def geometry_cases():
    for q,cf in [(1,1),(16,16),(32,8),(64,32)]:
        value=base(f'alternate-geometry-q{q}-chunk{cf}',q=q,cf=cf)
        frames=2*q+cf
        value['media']=[dict(name='range',channels=[[1000001+i*137 for i in range(frames)]])]
        value['loops'][0]['content']=dict(media='range',start_chunks=1,length_chunks=2*q//cf)
        controls={0:[action('play',0,0,'play')],3:[action('record',0,3*q,'record')],5:[action('resume',0,5*q,'play')],8:[action('stop',0,8*q,'stop')]}
        value['steps']=[step(q=q,ordinal=i,actions=controls.get(i,())) for i in range(10)]
        yield value

def scale_workload(kind):
    if kind=='original':return benchmark64()
    if kind=='streaming':
        value=base('production-8x8-streaming-metadata',loops=64,inputs=0,outputs=8,requests=128)
        loaded(value,2048,20261110)
        value['connections']=[]
        for i in range(64):
            value['nodes'].append(dict(id=lid(128+i),operation=dict(kind='gain',channels=1,initial=8388608)))
            value['connections'] += [edge(ep(2*i),ep(128+i)),edge(ep(128+i),ep(None,i//8))]
        value['steps']=[step(ports=0,actions=[action(f'play-{i}',i,0,'play',position=(i%64)*32) for i in range(64)])]+[step(ports=0) for _ in range(23)]
        for q in (8,16):value['steps'][q]['actions']=[action(f'seek-{q}-{i}',i,q*32,'seek',position=((i*7+q*3)%64)*32,retrigger=True) for i in range(64)]
        return value
    if kind!='mixed':raise ValueError('unknown scale workload')
    value=benchmark64();value['name']='production-8x8-mixed-record-play-retake';value['input_ports']=1
    for i,row in enumerate(value['loops']):
        if i>=48:row['content']=None
        value['connections'].append(edge(ep(),ep(2*i+1)))
    value['steps']=[step(ordinal=q) for q in range(24)]
    value['steps'][0]['actions']=[action(f'start-{i}',i,0,'play' if i<32 else 'record') for i in range(64)]
    value['steps'][12]['actions']=[action(f'play-take-{i}',i,384,'play') for i in range(32,64)]
    value['steps'][13]['actions']=[action(f'retake-{i}',i,416,'record') for i in range(32,64)]
    value['steps'][23]['actions']=[action(f'stop-{i}',i,736,'stop') for i in range(32,64)]
    return value
