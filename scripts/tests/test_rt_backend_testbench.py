"""Cross-language common contract and useful exact-comparison failures."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
import rt_backend_testbench as wire

class CommonContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(['cargo','build','-p','rt_backend_testbench'],cwd=ROOT,check=True,capture_output=True)
        cls.executable=ROOT/'target/debug/rt-backend-testbench'
        cls.scenario=wire.load(ROOT/'fpga/verification/scenarios/mono-record.json')

    def test_checked_in_schema_matches_rust(self):
        actual=json.loads(subprocess.check_output([self.executable,'schema'],text=True))
        self.assertEqual(actual,wire.schema())

    def test_roundtrip_all_documents_and_raw_integer_extremes_in_a_batch(self):
        scenario=deepcopy(self.scenario)
        scenario['steps'][0]['inputs'][0][:4]=[-2147483648,2147483647,-16777217,16777217]
        image=wire.run(scenario,executable=self.executable,command='compile')
        image['initial_state']['memory'] += [dict(kind='word',value=18446744073709551615),dict(kind='ramp',value=dict(start=-2147483648,target=2147483647,duration=4294967295,elapsed=4294967295))]
        result=wire.run(scenario,executable=self.executable)
        floating=wire.run(scenario,executable=self.executable,numeric='float')
        for kind,value in [('scenario',scenario),('image',image),('result',result),('result',floating)]:
            self.assertEqual(wire.loads(wire.dumps(value,kind),kind),value)
            self.assertEqual(wire.run_batch([value,value],executable=self.executable,command='roundtrip',kind=kind),[value,value])
        self.assertEqual(wire.run(scenario,executable=self.executable,command='validate'),scenario)

    def test_unknown_duplicate_nonfinite_and_lossy_fields_are_rejected(self):
        encoded=wire.dumps(self.scenario)
        malformed=[encoded.replace('"quantum": 32','"quantum": 32.0',1),
                   encoded.replace('"quantum": 32','"quantum": 32, "quantum": 32',1),
                   encoded.replace('{','{"extra": 0,',1),
                   encoded.replace('"quantum": 32','"quantum": true',1),
                   encoded.replace('"quantum": 32','"quantum": NaN',1)]
        for text in malformed:
            with self.subTest(text=text[:80]):
                with self.assertRaises(Exception):wire.loads(text)
                process=subprocess.run([self.executable,'validate'],input=text,text=True,capture_output=True)
                self.assertNotEqual(process.returncode,0)
        for invalid in [2147483648,-2147483649,0.5,False]:
            value=deepcopy(self.scenario);value['steps'][0]['inputs'][0][0]=invalid
            with self.subTest(invalid=invalid):
                with self.assertRaises(Exception):wire.validate(value)
        invalid=deepcopy(self.scenario);invalid['nodes'][0]['operation']['kind']='midi'
        with self.assertRaises(Exception):wire.validate(invalid)
        invalid=deepcopy(self.scenario);invalid['format_version']=2
        with self.assertRaises(Exception):wire.validate(invalid)
        process=subprocess.run([self.executable,'validate'],input=json.dumps(invalid),text=True,capture_output=True)
        self.assertNotEqual(process.returncode,0)

    def test_float_accuracy_gate_detects_changes_and_requires_explicit_clipping(self):
        from rt_backend_testbench.accuracy import compare
        scenario=wire.load(ROOT/'fpga/verification/scenarios/gain-ramps.json')
        def execute(source):
            return (wire.run(source,executable=self.executable,command='compile'),
                    wire.run(source,executable=self.executable),
                    wire.run(source,executable=self.executable,numeric='float'),
                    wire.run(source,executable=self.executable,command='audit'))
        image,q,f,audit=execute(scenario)
        report=compare(scenario,image,q,f,audit)
        self.assertLess(report['max_abs'],1e-7)
        self.assertEqual(report['saturating_operations'],0)
        bad=deepcopy(f);bad['quanta'][0]['outputs']['channels'][0][7]+=1e-3
        with self.assertRaisesRegex(AssertionError,'exceeds derived bound'):compare(scenario,image,q,bad,audit)
        clip=deepcopy(scenario);clip['nodes'][0]['operation']['initial']=2<<24
        for step in clip['steps']:step['actions']=[];step['inputs'][0]=[100<<24]*32
        image,q,f,audit=execute(clip)
        with self.assertRaisesRegex(AssertionError,'undeclared saturations'):compare(clip,image,q,f,audit)
        clip['arithmetic']='intentional_clipping'
        report=compare(clip,image,q,f,audit)
        self.assertEqual(report['saturating_operations'],320)
        self.assertGreater(report['max_abs'],70)

    def test_standalone_transition_fixture_uses_the_production_resolver_and_versioned_types(self):
        source=dict(format_version=1,name='transport',plan=dict(description=dict(quantum=32,
            loops=[dict(id=dict(slot=0,generation=1),kind='primitive',length=64,sync_source=None,repeat_sync=False,timeline=None)]),
            limits=dict(loop_slots=1,pending_requests=1,events=3)),steps=[dict(collect_completed=True,collect=[],cancel=[],commands=[
            dict(target=dict(slot=0,generation=1),sequence=1,at=0,timing=dict(kind='at'),operation=dict(kind='play',position=0,retrigger=True))])])
        image=wire.run(source,kind='transition_case',command='transition-compile',executable=self.executable)
        trace=wire.run(source,kind='transition_case',command='transition-run',executable=self.executable)
        self.assertEqual(image['components'],[[dict(slot=0,generation=1)]])
        self.assertEqual(trace['results'][0]['states'][0]['position'],32)
        for kind,value in [('transition_case',source),('transition_image',image),('transition_trace',trace)]:
            self.assertEqual(wire.run(value,kind=kind,command='roundtrip',executable=self.executable),value)
        bad=deepcopy(source);bad['plan']['description']['loops'][0]['kind']='composite'
        with self.assertRaisesRegex(RuntimeError,'Unsupported'):
            wire.run(bad,kind='transition_case',command='transition-compile',executable=self.executable)

    def test_a_deliberate_one_bit_mismatch_reports_frame_port_and_sample(self):
        expected=wire.run(self.scenario,executable=self.executable)
        actual=deepcopy(expected);actual['quanta'][3]['outputs']['channels'][0][17]^=1
        difference=wire.first_difference(actual,expected)
        self.assertIn('$.quanta[3].outputs.channels[0][17]',difference)
        with self.assertRaisesRegex(AssertionError,r'quanta\[3\].*channels\[0\]\[17\]'):
            wire.assert_exact(actual,expected)

if __name__=='__main__':unittest.main()
