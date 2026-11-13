"""Strict shared-session JSON and batched calls to the production Rust runner."""
from functools import lru_cache
import json
import math
import struct
from pathlib import Path
import subprocess
from jsonschema import Draft202012Validator, FormatChecker, validators

SCHEMA_PATH = Path(__file__).with_name('schema.json')

@lru_cache(maxsize=1)
def schema():
    return json.loads(SCHEMA_PATH.read_text(encoding='utf-8'))

@lru_cache(maxsize=None)
def validator(kind):
    definition = schema()[kind]
    Draft202012Validator.check_schema(definition)
    checker = Draft202012Validator.TYPE_CHECKER.redefine(
        'integer', lambda checker, value: type(value) is int
    ).redefine('string', lambda checker, value: isinstance(value, str)
               and not any(0xD800 <= ord(c) <= 0xDFFF for c in value))
    formats = FormatChecker()
    for bits in (8, 16, 32, 64):
        for signed, prefix in ((False, 'uint'), (True, 'int')):
            lower = -(1 << (bits-1)) if signed else 0
            upper = (1 << (bits-1))-1 if signed else (1 << bits)-1
            formats.checks(prefix+str(bits))(lambda value, lo=lower, hi=upper:
                value is None or (type(value) is int and lo <= value <= hi))
    formats.checks('float')(lambda value: value is None or (type(value) in (int, float)
        and math.isfinite(value) and abs(value) <= 3.4028234663852886e38))
    return validators.extend(Draft202012Validator, type_checker=checker)(definition, format_checker=formats)

def validate(document, kind='scenario'):
    validator(kind).validate(document)
    if document['format_version'] != 1:
        raise ValueError('unsupported format_version')

def _object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate JSON field: {key}')
        result[key] = value
    return result

def _constant(value):
    raise ValueError(f'non-finite JSON number: {value}')

def loads(text, kind='scenario'):
    document = json.loads(text, object_pairs_hook=_object, parse_constant=_constant)
    validate(document, kind)
    result=document['result'] if kind=='audit' else document
    if kind in ('result','audit'):
        samples=[q['outputs'] for q in result['quanta']]+[m['samples'] for m in result['media']]
        for sample in samples:
            if sample['encoding']=='float':
                sample['channels']=[[struct.unpack('<f',struct.pack('<f',v))[0] for v in channel] for channel in sample['channels']]
    return document

def dumps(document, kind='scenario'):
    validate(document, kind)
    return json.dumps(document, sort_keys=True, allow_nan=False) + '\n'

def load(path, kind='scenario'):
    return loads(Path(path).read_text(encoding='utf-8'), kind)

def dump(document, path, kind='scenario'):
    validate(document, kind)
    Path(path).write_text(json.dumps(document, indent=2, sort_keys=True, allow_nan=False)+'\n', encoding='utf-8')

def run_batch(scenarios, *, executable, command='run', numeric='q824', kind='scenario'):
    if command not in ('run', 'compile', 'validate', 'roundtrip', 'audit', 'transition-run', 'transition-compile') or numeric not in ('q824', 'float'):
        raise ValueError('unsupported command/numeric profile')
    args = [str(executable), command]
    if command == 'run':
        args += ['--numeric', numeric]
    if command == 'roundtrip':
        args += ['--kind', kind]
    process = subprocess.run(args, input=''.join(dumps(s, kind) for s in scenarios),
                             text=True, capture_output=True, check=False)
    if process.returncode:
        raise RuntimeError(process.stderr.strip())
    kind = {'run':'result', 'compile':'image', 'validate':'scenario', 'roundtrip':kind, 'audit':'audit', 'transition-run':'transition_trace', 'transition-compile':'transition_image'}[command]
    results = [loads(line, kind) for line in process.stdout.splitlines()]
    if len(results) != len(scenarios):
        raise ValueError('runner returned an incorrect document count')
    return results

def run(scenario, **options):
    return run_batch([scenario], **options)[0]

class TransitionService:
    """Transport the shared Rust request owner without implementing its policy."""
    def __init__(self, image, executable=None):
        executable = executable or Path(__file__).resolve().parents[2] / 'target/debug/rt-backend-testbench'
        self.process = subprocess.Popen([str(executable), 'transition-service'],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, bufsize=1)
        self.timings = []
        configured = self.request(kind='configure', image={key:image[key] for key in ('description','limits','dependency_order')})
        self.metadata_bytes = configured['metadata_bytes']
        self.abi = configured['abi']

    def request(self, **message):
        self.process.stdin.write(json.dumps(message, allow_nan=False)+'\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError('transition service exited: '+self.process.stderr.read())
        result = json.loads(line)
        if 'service_ns' in result: self.timings.append(dict(kind=message['kind'],ns=result['service_ns']))
        if 'fault' in result:
            raise RuntimeError('transition service fault: '+result['fault'])
        return result

    def close(self):
        process = getattr(self, 'process', None)
        if process is not None:
            if process.stdin and not process.stdin.closed: process.stdin.close()
            try: process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill();process.wait()
            process.stdout.close();process.stderr.close()
            self.process = None

    def __del__(self):
        self.close()

def port_words(value, count):
    return [(value >> (32*i)) & 0xffffffff for i in range(count)]

def port_value(words):
    return sum(word << (32*i) for i,word in enumerate(words))

def first_difference(actual, expected, path='$'):
    if type(actual) is not type(expected):
        return f'{path}: type {type(actual).__name__} != {type(expected).__name__}'
    if isinstance(expected, dict):
        if actual.keys() != expected.keys():
            return f'{path}: keys {sorted(actual)} != {sorted(expected)}'
        children = ((actual[k], value, f'{path}.{k}') for k, value in expected.items())
    elif isinstance(expected, list):
        if len(actual) != len(expected):
            return f'{path}: length {len(actual)} != {len(expected)}'
        children = ((a, b, f'{path}[{i}]') for i, (a, b) in enumerate(zip(actual, expected)))
    else:
        return None if actual == expected else f'{path}: {actual!r} != {expected!r}'
    for a, b, child_path in children:
        difference = first_difference(a, b, child_path)
        if difference:
            return difference
    return None

def assert_exact(actual, expected):
    difference = first_difference(actual, expected)
    if difference:
        raise AssertionError(difference)
