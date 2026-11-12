#!/usr/bin/env python3
"""Compile the ordinary production graph into prepared fabric-loader ROMs."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT/'scripts'))
import rt_backend_testbench as wire


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


tp = module('tp_abi', ROOT/'fpga/transition_processor/tests/codec.py')
sp = module('sp_abi', ROOT/'fpga/state_processor/tests/codec.py')


def packet(sequence=0, kind=0, opcode=0, index=0, data=0):
    return sequence << 682 | kind << 680 | opcode << 672 | index << 640 | data


def make_scenario(q=32, chunk=None, channels=16, monitoring=False):
    identity = lambda slot: dict(slot=slot, generation=1)
    endpoint = lambda node, port=0: dict(node=node, port=port)
    loops = [dict(id=identity(c), channels=1, sync_source=None, repeat_sync=False, content=None)
             for c in range(channels)]
    nodes, connections = [], []
    for c in range(channels):
        playback, capture = identity(2*c), identity(2*c+1)
        nodes += [dict(id=playback, operation=dict(kind='playback', target=identity(c))),
                  dict(id=capture, operation=dict(kind='capture', target=identity(c)))]
        connections += [dict(source=endpoint(None, c), destination=endpoint(capture)),
                        dict(source=endpoint(playback), destination=endpoint(None, c))]
        if monitoring:
            connections.append(dict(source=endpoint(None, c), destination=endpoint(None, c)))
    steps = []
    for n in range(18):
        actions = []
        for c in range(channels):
            operation = None
            if n == 0: operation = dict(kind='record')
            if n == 2: operation = dict(kind='play', position=0, retrigger=True)
            if n == 5 and c % 2 == 0: operation = dict(kind='stop')
            if n == 6 and c % 2 == 0: operation = dict(kind='record')
            if n == 8 and c % 2 == 0: operation = dict(kind='play', position=0, retrigger=True)
            if operation:
                actions.append(dict(kind='transition', name=f'q{n}-c{c}', target=identity(c),
                                    at=n*q, timing=dict(kind='at'), operation=operation))
        inputs = [[(c+1)*100000+(n*q+f)*37+1 for f in range(q)] for c in range(channels)]
        steps.append(dict(inputs=inputs, actions=actions, collect_completed=False))
    return dict(format_version=1, name=f'audio-io-{channels}x{channels}-q{q}', quantum=q,
                chunk_frames=chunk or q, input_ports=channels, output_ports=channels,
                request_capacity=128, loops=loops, nodes=nodes, connections=connections, media=[], steps=steps)


def prepare(output, q=32, chunk=None, channels=16, monitoring=False):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    scenario = make_scenario(q, chunk, channels, monitoring)
    executable = ROOT/'target/debug/rt-backend-testbench'
    image = wire.run_batch([scenario], executable=executable, command='compile')[0]
    expected = wire.run_batch([scenario], executable=executable)[0]
    floating = wire.run_batch([scenario], executable=executable, numeric='float')[0]
    for name, value in [('scenario', scenario), ('image', image), ('expected', expected), ('float', floating)]:
        (output/f'{name}.json').write_text(json.dumps(value, indent=2)+'\n')
    transition = image['transition']
    limits = transition['limits']
    boot = []
    specs = {s['id']['slot']: s for s in transition['description']['loops']}
    for i in range(limits['loop_slots']):
        boot.append(packet(opcode=tp.WRITE_SPEC, index=i, data=tp.encode_spec(specs.get(i))))
    offset = 0
    for component in transition['components']:
        for i, member in enumerate(component):
            boot.append(packet(opcode=tp.WRITE_MEMBER, index=offset,
                               data=member['slot'] | (int(i == len(component)-1) << 32)))
            offset += 1
    data = limits['loop_slots'] | limits['pending_requests'] << 32 | limits['events'] << 64 | offset << 96 | q << 128
    boot.append(packet(opcode=tp.PREPARE, data=data))
    for i, instruction in enumerate(image['program']['instructions']):
        boot.append(packet(kind=1, index=i, data=sp.encode_instruction(instruction)))
    for i, value in enumerate(image['initial_state']['memory']):
        boot.append(packet(kind=1, index=i | (1 << 31), data=sp.encode_value(value)))
    for i in range(32):
        boot.append(packet(kind=2, opcode=1, index=i, data=0))
    for i, value in enumerate([0, 0, 0, 256]):
        boot.append(packet(kind=2, opcode=0, index=i, data=value))
    boot.append(packet(kind=3))
    events, sequence = [], 0
    for n, step in enumerate(scenario['steps']):
        for action in step['actions']:
            sequence += 1
            command = {k: action[k] for k in ('target', 'at', 'timing', 'operation')}
            command['sequence'] = sequence
            events.append(packet(sequence=n, opcode=tp.SUBMIT, data=tp.encode_command(command)))
        events.append(packet(sequence=n, kind=2))
    for name, records in [('boot', boot), ('events', events)]:
        (output/f'{name}.hex').write_text(''.join(f'{r:0187x}\n' for r in records))
    config = dict(q=q, channels=channels, boot_words=len(boot), event_words=len(events),
                  instructions=len(image['program']['instructions']), states=len(image['initial_state']['memory']),
                  buffers=image['buffers'], jobs=image['program']['jobs_per_quantum'],
                  loop_slots=limits['loop_slots'], requests=limits['pending_requests'], events=limits['events'])
    (output/'config.json').write_text(json.dumps(config, indent=2)+'\n')
    return config


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=HERE/'examples/default')
    parser.add_argument('--q', type=int, default=32)
    parser.add_argument('--chunk', type=int)
    parser.add_argument('--channels', type=int, default=16)
    parser.add_argument('--monitoring', action='store_true')
    args = parser.parse_args()
    subprocess.run(['cargo', 'build', '--locked', '-p', 'rt_backend_testbench'], cwd=ROOT, check=True)
    print(json.dumps(prepare(args.output, args.q, args.chunk, args.channels, args.monitoring), indent=2))


if __name__ == '__main__':
    main()
