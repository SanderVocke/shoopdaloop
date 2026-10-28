#!/usr/bin/env python3
"""Integration checks that prevent silent bypass/missing-plugin benchmark results."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile

root=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('runner',root/'experiments/carla/run.py')
runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
work=Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(dir=work) as temporary:
    temporary=Path(temporary)
    for mode in ['native','lv2']:
        library=work/('Carla-reference/bin/libcarla_native-plugin.so' if mode=='native' else 'Carla-reference/bin/carla.lv2/carla.so')
        for name in ['empty','gain','synthetic']:
            args=[work/'abi_benchmark',mode,library,work/'Carla-reference/bin/carla.lv2',work/f'states/{name}.xml',128,10,temporary/'samples.csv']
            assert runner.command(args,temporary/'run.log',runner.environment(work))==0,(mode,name,(temporary/'run.log').read_text())
        text=(work/'states/synthetic.xml').read_text()
        for name, altered in [('missing',text.replace(str(work/'workload.so'),str(temporary/'absent.so'))),
                              ('bypass',text.replace('<Active>Yes</Active>','<Active>No</Active>'))]:
            state=temporary/f'{name}.xml';state.write_text(altered)
            args=[work/'abi_benchmark',mode,library,work/'Carla-reference/bin/carla.lv2',state,128,10,temporary/'bad.csv']
            assert runner.command(args,temporary/'bad.log',runner.environment(work))!=0,(mode,name,'silently accepted invalid workload')
    assert runner.command([work/'abi_benchmark','native',work/'absent.so',work,work/'states/empty.xml',128,10,temporary/'bad.csv'],temporary/'missing.log',runner.environment(work))!=0
print('PASS: six positive workload checks; missing-plugin and bypass rejection through both interfaces; missing-runtime rejection')
