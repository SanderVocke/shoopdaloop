import json,pathlib,re,sys
root=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else pathlib.Path(__file__).resolve().parent
def util(path):
 text=path.read_text(); d={}
 for label,key in [('Slice LUTs','luts'),('Slice Registers','registers'),('Block RAM Tile','bram36_equivalent'),('DSPs','dsps'),('RAMB36/FIFO*','ramb36'),('RAMB18','ramb18')]:
  m=re.search(r'^\|\s*'+re.escape(label)+r'\*?\s*\|\s*([\d.]+)',text,re.M)
  if m: d[key]=float(m[1]) if '.' in m[1] else int(m[1])
 return d
results=[]
for version in ('before','after'):
 for mhz in (100,150):
  p=root/f'{version}-{mhz}'; result=dict(version=version,mhz=mhz)
  result['provenance']=json.loads((p/'provenance.json').read_text())
  result['outcome']=(p/'outcome.txt').read_text() if (p/'outcome.txt').exists() else 'running'
  result['stages']={}
  for stage in ('synth','opt','place','route'):
   r=p/f'post_{stage}_utilization.rpt'
   if not r.exists(): continue
   data=dict(total=util(r))
   for core in ('audio','midi'):
    r=p/f'post_{stage}_{core}_utilization.rpt'
    if r.exists(): data[core]=util(r)
   f=p/f'post_{stage}.json'
   if f.exists(): data['timing']=json.loads(f.read_text())
   r=p/f'post_{stage}_timing_summary.rpt'
   if r.exists():
    m=re.search(r'WNS\(ns\).*?\n\s*-+.*?\n\s*([^\n]+)',r.read_text(),re.S)
    if m:
     vals=m[1].split()
     if len(vals)>=8: data['summary']=dict(zip(('wns_ns','tns_ns','setup_failing','setup_total','whs_ns','ths_ns','hold_failing','hold_total'),map(float,vals[:8])))
   result['stages'][stage]=data
  result['timing_estimates']={}
  for core in ('audio','midi'):
   f=p/'timing-estimates'/f'{core}_timing.json'
   if f.exists(): result['timing_estimates'][core]=json.loads(f.read_text())
  log=(p/'console.log').read_text() if (p/'console.log').exists() else ''
  failure={}
  for key,pattern in [
   ('control_sets',r'Control sets: (\d+)'),
   ('packed_luts',r'Luts: (\d+) \(combined\)'),
   ('primitive_luts',r'\(combined\) (\d+) \(total\)'),
   ('available_slices',r'of which (\d+) slices are available'),
   ('unplaced_required_slices',r'unplaced instances require (\d+) slices')]:
   m=re.search(pattern,log)
   if m: failure[key]=int(m[1])
  if failure: result['placement_failure']=failure
  results.append(result)
(root/'results.json').write_text(json.dumps(results,indent=2)+'\n')
for result in results:
 print(result['version'],result['mhz'],result['outcome'].strip())
 for s,d in result['stages'].items(): print(s,d.get('total'),d.get('timing'),d.get('summary'))
