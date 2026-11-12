#!/usr/bin/env python3
"""Compile an autonomous record/play workload; no audio correctness evaluation."""
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
from rt_backend_testbench.cases import base, lid, ep, edge, action, step


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


tp = module('tp_codec', ROOT/'fpga/transition_processor/tests/codec.py')
sp = module('sp_codec', ROOT/'fpga/state_processor/tests/codec.py')


def packet(sequence=0, kind=0, opcode=0, index=0, data=0):
    return sequence << 682 | kind << 680 | opcode << 672 | index << 640 | data


def scenario(loops, tracks, q, record_quanta, mixed):
    value = base('zedboard-autonomous-timing', loops=loops, inputs=1, outputs=tracks,
                 q=q, requests=4*loops+16)
    value['connections'] = []
    for i in range(loops):
        gain = 2*loops+i
        value['nodes'].append(dict(id=lid(gain), operation=dict(kind='gain', channels=1, initial=8388608)))
        value['connections'] += [edge(ep(), ep(2*i+1)), edge(ep(2*i), ep(gain)),
                                 edge(ep(gain), ep(None, i % tracks))]
    controls = {0: [action(f'record-{i}', i, 0, 'record') for i in range(loops)],
                record_quanta: [action(f'play-{i}', i, record_quanta*q, 'play') for i in range(loops)]}
    if mixed:
        for boundary, operation in ((record_quanta+2, 'record'), (record_quanta+4, 'play')):
            controls[boundary] = [action(f'{operation}-retake-{i}', i, boundary*q, operation)
                                  for i in range(0, loops, 2)]
    value['steps'] = [step(q=q, ports=1, ordinal=n, actions=controls.get(n, ()), collect=False)
                      for n in range(record_quanta+8)]
    return value


def prepare(output, loops=64, tracks=8, q=32, record_quanta=64, mixed=True, build=True):
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    if build:
        subprocess.run(['cargo', 'build', '--locked', '-p', 'rt_backend_testbench'], cwd=ROOT, check=True)
    value = scenario(loops, tracks, q, record_quanta, mixed)
    image = wire.run_batch([value], executable=ROOT/'target/debug/rt-backend-testbench', command='compile')[0]
    transition = image['transition']
    limits = transition['limits']
    block_chunks = 32
    slots = loops*((record_quanta+block_chunks-1)//block_chunks+2)*block_chunks
    views = max(32, loops*4)
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
    for i, state in enumerate(image['initial_state']['memory']):
        boot.append(packet(kind=1, index=i | (1 << 31), data=sp.encode_value(state)))
    for i in range(views):
        boot.append(packet(kind=2, opcode=1, index=i))
    for i, count in enumerate([0, 0, 0, slots//block_chunks]):
        boot.append(packet(kind=2, opcode=0, index=i, data=count))
    boot.append(packet(kind=3))
    events, serial = [], 0
    for n, item in enumerate(value['steps']):
        for control in item['actions']:
            serial += 1
            command = {k: control[k] for k in ('target', 'at', 'timing', 'operation')}
            command['sequence'] = serial
            events.append(packet(sequence=n, opcode=tp.SUBMIT, data=tp.encode_command(command)))
        events.append(packet(sequence=n, kind=2))
    for name, records in [('boot', boot), ('events', events)]:
        (output/f'{name}.hex').write_text(''.join(f'{word:0187x}\n' for word in records))
    params = dict(LOOP_DEPTH=limits['loop_slots'], REQUEST_DEPTH=limits['pending_requests'],
                  EVENT_DEPTH=limits['events'], PROGRAM_DEPTH=max(2, len(image['program']['instructions'])),
                  STATE_DEPTH=max(2, len(image['initial_state']['memory'])), JOB_DEPTH=max(2, image['program']['jobs_per_quantum']),
                  BUFFERS=max(2, image['buffers']), MAX_FRAMES=q, CHUNK_FRAMES=q,
                  INPUT_PORTS=1, OUTPUT_PORTS=tracks, VIEWS=views, SLOTS=slots, BLOCK_CHUNKS=block_chunks,
                  RETIRED_TABLES=256, RECLAIM_BLOCKS=20, RECLAIM_CYCLES=10000, RECLAIM_GUARD=1024,
                  AXI_DATA_WIDTH=64, EXTERNAL_IO=1, INSTRUCTION_COUNT=len(image['program']['instructions']),
                  STATE_COUNT=len(image['initial_state']['memory']), BUFFER_COUNT=image['buffers'],
                  JOB_BUDGET=image['program']['jobs_per_quantum'], BOOT_WORDS=len(boot), EVENT_WORDS=len(events))
    config = dict(parameters=params, warmup_quanta=record_quanta+8, loops=loops, tracks=tracks,
                  record_quanta=record_quanta, mixed=mixed,
                  revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip())
    wire.dump(value, output/'scenario.json')
    wire.dump(image, output/'image.json', 'image')
    (output/'config.json').write_text(json.dumps(config, indent=2)+'\n')
    return config


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT/'work/zedboard/workload')
    parser.add_argument('--loops', type=int, default=64)
    parser.add_argument('--tracks', type=int, default=8)
    parser.add_argument('--q', type=int, default=32)
    parser.add_argument('--record-quanta', type=int, default=64)
    parser.add_argument('--playback-only', action='store_true')
    parser.add_argument('--no-build', action='store_true')
    args = parser.parse_args()
    if min(args.loops, args.tracks, args.q, args.record_quanta) < 1 or args.tracks > args.loops:
        parser.error('positive loop/track/quantum/recording counts required; tracks <= loops')
    print(json.dumps(prepare(args.output, args.loops, args.tracks, args.q, args.record_quanta,
                             not args.playback_only, not args.no_build), indent=2))


if __name__ == '__main__':
    main()
