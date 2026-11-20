from pathlib import Path
import csv,re,collections,json
root=Path(__file__).parent
cells={r['name']:r['ref'] for r in csv.DictReader((root/'cells.tsv').open(),delimiter='\t')}
service='pipeline/core_under_test/core/midi/service'

def bank(name):
    leaf=name.rsplit('/',1)[-1]
    leaf=re.sub(r'_i_\d+(?:__\d+)?$','',leaf)
    leaf=re.sub(r'\[[^]]*\]','',leaf)
    leaf=re.sub(r'_reg(?:__\d+)?$','',leaf)
    return leaf
for scope in [service,service+'/content',service+'/states',service+'/states/transition_engine']:
    selected={n:r for n,r in cells.items() if n.rsplit('/',1)[0]==scope}
    print('\nSCOPE',scope, 'cells',len(selected),'types',collections.Counter(selected.values()))
    for typ,pred in [('FF',lambda r:r.startswith('FD')),('LUT',lambda r:r.startswith('LUT')),('CARRY',lambda r:r.startswith('CARRY'))]:
        groups=collections.Counter(bank(n) for n,r in selected.items() if pred(r))
        print(typ,'total',sum(groups.values()),'top',groups.most_common(28))
print('\nDSP', (root/'dsp.tsv').read_text())
pins=collections.defaultdict(dict)
netdrivers=collections.defaultdict(set)
for r in csv.DictReader((root/'midi-pins.tsv').open(),delimiter='\t'):
    pins[r['cell']][r['pin']]=r['net']
    if r['direction']=='OUT':
        for net in r['net'].split(','):
            if net:netdrivers[net].add(r['cell'])
ffgroups=collections.defaultdict(list)
for n,ref in cells.items():
    if n in pins and ref.startswith('FD'):
        p=pins[n]
        key=(ref,p.get('C',''),p.get('CE',''),p.get('R',p.get('S',p.get('CLR',p.get('PRE','')))))
        ffgroups[key].append(n)
print('\nMIDI logical FF control groups (raw net signatures):',len(ffgroups))
print('Group size histogram',collections.Counter(len(v) for v in ffgroups.values()))
print('Singleton FF groups',sum(len(v)==1 for v in ffgroups.values()))
for scope in [service,service+'/content',service+'/states/transition_engine']:
    bybank=collections.defaultdict(list)
    for key,regs in ffgroups.items():
        for reg in regs:
            if reg.rsplit('/',1)[0]==scope:bybank[bank(reg)].append(key)
    print('CONTROL BANKS',scope,[(b,len(keys),len(set(keys))) for b,keys in sorted(bybank.items(),key=lambda x:-len(set(x[1])))[:20]])
# Combinational cones stop at FF, RAM and DSP boundaries. Counts below are LUT primitives, not packed LUT sites.
cache={}
def cone(net,visiting=None):
    if net in cache:return cache[net]
    if visiting is None:visiting=set()
    if net in visiting:return set()
    visiting.add(net)
    result=set()
    for driver in netdrivers.get(net,[]):
        ref=cells[driver]
        if not (ref.startswith('LUT') or ref.startswith('CARRY') or ref.startswith('MUXF')):continue
        if ref.startswith('LUT'):result.add(driver)
        for pin,othernet in pins[driver].items():
            if (ref.startswith('LUT') and pin.startswith('I')) or (ref.startswith('MUXF') and pin in ['I0','I1','S']) or (ref.startswith('CARRY') and (pin.startswith('DI') or pin.startswith('S') or pin in ['CI','CYINIT'])):
                result.update(cone(othernet,visiting))
    visiting.remove(net)
    cache[net]=result
    return result
bybank=collections.defaultdict(lambda: {'regs':0,'D':set(),'CE':set(),'R':set()})
for n,ref in cells.items():
    if n.rsplit('/',1)[0]!=service or not ref.startswith('FD'):continue
    entry=bybank[bank(n)];entry['regs']+=1
    for pin in ['D','CE','R']:
        if pin in pins[n]:entry[pin].update(cone(pins[n][pin]))
print('\nSERVICE FF input cones (non-additive LUT primitives)')
for b,e in sorted(bybank.items(),key=lambda x:-len(x[1]['D']|x[1]['CE']|x[1]['R']))[:24]:
    print(b,e['regs'],'D',len(e['D']),'CE',len(e['CE']),'R',len(e['R']),'union',len(e['D']|e['CE']|e['R']))
report={'service':service,'primitive_types':dict(collections.Counter(r for n,r in cells.items() if n.rsplit('/',1)[0]==service)),
'ff_banks':{b:{'bits':e['regs'],'D_cone_lut_primitives':len(e['D']),'CE_cone_lut_primitives':len(e['CE']),'R_cone_lut_primitives':len(e['R'])} for b,e in bybank.items()},
'logical_ff_control_groups_midi':len(ffgroups),'singleton_groups_midi':sum(len(v)==1 for v in ffgroups.values())}
(root/'analysis.json').write_text(json.dumps(report,indent=2)+'\n')
