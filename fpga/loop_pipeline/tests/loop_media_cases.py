"""Audio stimuli for the transition/state/job/media path; Rust computes results."""
from copy import deepcopy
import random
from cases import case, spec, step, command, play, seek, lid
from pipeline_cases import connect
import fpga_testbench as wire


def document(kind, value):
    return {'format_version': 1, 'payload': {'kind': kind, 'value': value}}


def empty(capacities, views):
    _, slots, cf = capacities
    return dict(chunk_frames=cf, chunk_limit=slots, samples=[], free_chunks=[],
                views=[dict(chunks=[], frames=0) for _ in range(views)])


def audio(value, depths, capacities, views=None):
    result = connect(value, depths)
    loops = sorted(value['plan']['description']['loops'], key=lambda x: x['id']['slot'], reverse=True)
    result['initial_state']['memory'] = [dict(kind='word', value=i)
                                       for i in range(len(loops)) for _ in range(2)]
    for i in range(len(loops)):
        result['program']['instructions'][1 + 2*i]['view'] = 2*i
        result['program']['instructions'][2 + 2*i]['view'] = 2*i + 1
    result['media'] = empty(capacities, views if views is not None else len(loops))
    q = result['program']['quantum']
    for i, row in enumerate(result['steps']):
        row['work'] = dict(kind='audio', value=dict(inputs=[[
            (-1 if f % 3 == 0 else 1) * (100000 + i*7919 + f*257) for f in range(q)]]))
    return result


def recorded(capacities, steps, executable):
    initial = empty(capacities, 0)
    source = document('media_case', dict(name='prepare-loop-media', media=initial, steps=steps))
    trace = wire.run_reference('trace', source, executable=executable)['payload']['value']
    assert all(row['status']['kind'] == 'completed' for row in trace['results'])
    return trace['results'][-1]['media']


def directed(depths, capacities, executable):
    n = min(depths[0], depths[1], (depths[2]-1)//2, (depths[3]-2)//2,
            depths[4]//2, (depths[5]-2)//2, depths[7]-1, capacities[0], capacities[1])
    q = max(q for q in (1, 16, 32) if q <= min(depths[6], capacities[2]))
    yield audio(case('actual-all-loop-slots-record-and-play', [spec(i, 0) for i in range(n)], [
        step([command(i, i+1, dict(kind='record')) for i in range(n)]),
        step([command(i, n+i+1) for i in range(n)]), step(),
        step([command(i, 2*n+i+1, seek(0, True)) for i in range(n)]),
        step([command(i, 3*n+i+1, dict(kind='stop')) for i in range(n)]),
    ], q=q, requests=n), depths, capacities)

    for q in (1, 16, 32):
        if q > depths[6] or 2*((q + capacities[2]-1)//capacities[2]) > capacities[1]:
            continue
        value = audio(case(f'actual-record-play-retake-q{q}', [spec(0, 0)], [
            step([command(0, 1, {'kind': 'record'})], previews=[[], []]),
            step([command(0, 2)]), step(), step([command(0, 3, seek(0, True))]),
            step([command(0, 4, {'kind': 'stop'})]),
            step([command(0, 5, {'kind': 'record'})]), step([command(0, 6)]), step(),
        ], q=q, requests=2), depths, capacities, views=2)
        value['steps'][5]['writes'] = [dict(address=1, value=dict(kind='word', value=1))]
        value['steps'][6]['writes'] = [dict(address=0, value=dict(kind='word', value=1))]
        yield value

    q = 1
    loops = [spec(0, 2, 1, True), spec(1, 2, 0, True)]
    value = audio(case('actual-sync-cycle-and-delayed-retrigger', loops, [
        step([command(0, 1), command(1, 2)], previews=[[], [], []]),
        step([command(1, 3, seek(0, True), source=0, skip=1)], previews=[[], []]),
        step(), step(), step(), step([command(0, 4, {'kind': 'stop'})]), step(),
    ], q=q, requests=min(4, depths[1])), depths, capacities)
    value['media'] = recorded(capacities, [
        dict(kind='create'), dict(kind='append', view=0, samples=[-12345, 67890]),
        dict(kind='create'), dict(kind='append', view=1, samples=[2147483647, -2147483648]),
    ], executable)
    yield value

    q = 16 if depths[6] >= 16 and capacities[1]*capacities[2] >= 32 else 1
    value = audio(case('actual-refused-record-keeps-playing-old-view', [spec(0, 2*q)], [
        step([command(0, 1)]),
        step([command(0, 2, {'kind': 'record'})], previews=[[], [dict(kind='capture_unavailable', target=lid(0), reason='capacity')]]),
        step([command(0, 3, {'kind': 'stop'})]),
    ], q=q, requests=2), depths, capacities, views=2)
    value['steps'][1]['writes'] = [dict(address=1, value=dict(kind='word', value=1))]
    value['media'] = recorded(capacities, [dict(kind='create'),
        dict(kind='append', view=0, samples=[1111+i for i in range(2*q)]), dict(kind='create')], executable)
    yield value

    q = min(16, depths[6])
    value = audio(case('actual-unaligned-retained-range-wrap', [spec(0, q)], [
        step([command(0, 1)]), step(), step([command(0, 2, seek(0, True))]), step(),
    ], q=q, requests=2), depths, capacities, views=2)
    value['initial_state']['memory'] = [dict(kind='word', value=1), dict(kind='word', value=0)]
    start = min(3, capacities[2]-1)
    value['media'] = recorded(capacities, [dict(kind='create'),
        dict(kind='append', view=0, samples=[100000+i*37 for i in range(q+start+1)]),
        dict(kind='slice', source=0, start=start, frames=q), dict(kind='clear', view=0)], executable)
    yield value

    for name, view, limit in [('full-payload-capacity', 0, 0), ('missing-media-view', 0xffffffff, capacities[1])]:
        value = audio(case('actual-job-failure-'+name, [spec(0, 0)], [
            step([command(0, 1, {'kind': 'record'})])], q=1, requests=1), depths, capacities)
        value['media']['chunk_limit'] = limit
        value['initial_state']['memory'][1]['value'] = view
        yield value

    value = deepcopy(value)
    value['name'] = 'actual-read-only-capture-fails-without-clock-advance'
    value['initial_state']['memory'][1]['value'] = 1
    value['media'] = recorded(capacities, [dict(kind='create'), dict(kind='append', view=0, samples=[19]),
        dict(kind='slice', source=0, start=0, frames=1)], executable)
    yield value


def seeded(depths, capacities, count):
    rng = random.Random(20261111)
    for index in range(count):
        n = rng.randint(1, min(3, depths[0], depths[7]-3, (depths[4]-1)//2))
        q = rng.choice([q for q in (1, 16, 32) if q <= depths[6]
                        and (index % 5 == 0 or 2*n*((2*q + capacities[2]-1)//capacities[2]) <= capacities[1])])
        sequences = 0
        rows = []
        for frame in range(18):
            commands, writes = [], []
            for slot in range(n):
                operation = None
                if frame in (0, 6):
                    operation = dict(kind='record')
                    if frame == 6:
                        writes.append(dict(address=2*(n-1-slot)+1, value=dict(kind='word', value=n+(n-1-slot))))
                elif frame in (2, 8):
                    operation = play()
                    if frame == 8:
                        writes.append(dict(address=2*(n-1-slot), value=dict(kind='word', value=n+(n-1-slot))))
                elif frame not in (1, 7) and rng.randrange(3) == 0:
                    operation = rng.choice([play(), seek(rng.choice([0, q]), bool(rng.randrange(2))), dict(kind='stop')])
                if operation is not None:
                    sequences += 1
                    commands.append(command(slot, sequences, operation))
            rows.append(dict(transition=step(commands, previews=[[] for _ in range(rng.randint(1, 3))]), writes=writes))
        value = audio(case(f'actual-seeded-{index}', [spec(i, 0) for i in range(n)],
            [r['transition'] for r in rows], q=q, requests=min(depths[1], 2*n)), depths, capacities, views=2*n)
        for row, generated in zip(value['steps'], rows):
            row['writes'] = generated['writes']
            row['work']['value']['inputs'] = [[rng.choice([-2147483648, 2147483647, -1, 0, 1, rng.randint(-2000000000, 2000000000)]) for _ in range(q)]]
        ramp = len(value['initial_state']['memory'])
        value['initial_state']['memory'].append(dict(kind='ramp', value=dict(start=-(1<<24), target=rng.randint(0, 1<<24), duration=rng.choice([0, q, 3*q, 9*q]), elapsed=0)))
        output = value['program']['instructions'].pop()
        value['program']['instructions'].extend([
            dict(kind='process_gain', ramp=ramp, source=1, destination=n+1),
            dict(kind='process_mix', left=n+1, right=0, destination=n+2),
            dict(kind='process_output', source=n+2, port=output['port']),
        ])
        value['program']['jobs_per_quantum'] += 2
        value['buffers'] = n+3
        yield value
