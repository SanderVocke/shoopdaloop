"""64 independent logical loops with the same audio graph as the media benchmark."""
import random
import fpga_testbench as wire
from cases import case, spec, step, command, play


def document(kind, value):
    return dict(format_version=3, payload=dict(kind=kind, value=value))


def playback64(reference, seed=20261110):
    rng = random.Random(seed)
    nodes, views, samples = [], [], []
    for track in range(8):
        mixed = None
        for loop in range(8):
            view = track*8+loop
            nodes.append(dict(kind='playback', view=view, length_chunks=16, position_chunks=view % 16, playing=True))
            nodes.append(dict(kind='gain', input=len(nodes)-1, gain=8388608))
            gained = len(nodes)-1
            if mixed is None:
                mixed = gained
            else:
                nodes.append(dict(kind='mix', left=mixed, right=gained))
                mixed = len(nodes)-1
            start = len(samples)//32
            samples.extend(rng.randint(-(1 << 24), (1 << 24)-1) for _ in range(512))
            views.append(dict(chunks=list(range(start, start+16)), frames=512))
        nodes.append(dict(kind='output', input=mixed, port=track))
    compiled = wire.run_reference('compile', document('graph', dict(quantum=32, chunk_frames=32, nodes=nodes)), executable=reference)['payload']['value']
    for node, instruction in zip(nodes, compiled['program']['instructions']):
        if node['kind'] == 'playback':
            replacement = dict(kind='process_logical_playback', loop_id=spec(node['view'], 512)['id'],
                               view=instruction['view'], destination=instruction['destination'])
            instruction.clear()
            instruction.update(replacement)
    transitions = case('coordinated-8x8-playback-q32', [spec(i, 512) for i in range(64)],
        [step([command(i, i+1, play((i % 16)*32)) for i in range(64)])]+[step() for _ in range(3)],
        q=32, requests=128)
    return dict(name=transitions['name'], plan=transitions['plan'], program=compiled['program'],
                initial_state=dict(memory=compiled['initial_state']), queue_capacity=256,
                buffers=len(nodes), output_ports=8,
                media=dict(chunk_frames=32, chunk_limit=1024, samples=samples, free_chunks=[], views=views),
                steps=[dict(transition=row, writes=[], work=dict(kind='audio', value=dict(inputs=[])))
                       for row in transitions['steps']])
