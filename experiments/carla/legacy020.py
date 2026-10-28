#!/usr/bin/env python3
"""Exercise the as-shipped 0.2.0 backend through its public dummy-driver ABI."""
import base64
import ctypes as c
import csv
import json
import os
from pathlib import Path
import struct
import sys
import time


def main():
    state_path, frames, blocks, output = sys.argv[1:]
    frames, blocks = int(frames), int(blocks)
    import faulthandler
    faulthandler.dump_traceback_later(max(30, (blocks+200)*frames/48000*3), exit=True)
    library = c.CDLL(os.environ['SHOOP_BENCH_020_LIBRARY'])
    pointer, unsigned = c.c_void_p, c.c_uint

    class Settings(c.Structure):
        _fields_ = [('name', c.c_char_p), ('rate', unsigned), ('frames', unsigned)]

    class ChainState(c.Structure):
        _fields_ = [('ready', unsigned), ('active', unsigned), ('visible', unsigned)]

    def function(name, result, *args):
        f = getattr(library, name); f.restype = result; f.argtypes = args
        def invoke(*values):
            if os.environ.get('SHOOP_BENCH_DEBUG'): print(name, flush=True)
            return f(*values)
        return invoke

    create_session = function('create_backend_session', pointer)
    create_driver = function('create_audio_driver', pointer, c.c_int, pointer)
    start = function('start_dummy_driver', None, pointer, Settings)
    set_driver = function('set_audio_driver', c.c_int, pointer, pointer)
    controlled = function('dummy_audio_enter_controlled_mode', None, pointer)
    create_chain = function('create_fx_chain', pointer, pointer, c.c_int, c.c_char_p)
    status = function('get_fx_chain_state', c.POINTER(ChainState), pointer)
    free_status = function('destroy_fx_chain_state', None, c.POINTER(ChainState))
    restore = function('restore_fx_chain_internal_state', None, pointer, c.c_char_p)
    active = function('set_fx_chain_active', None, pointer, unsigned)
    open_port = function('open_driver_audio_port', pointer, pointer, pointer, c.c_char_p, c.c_int, unsigned)
    connect = function('connect_audio_port_internal', None, pointer, pointer)
    fx_input = function('fx_chain_audio_input_port', pointer, pointer, unsigned)
    fx_output = function('fx_chain_audio_output_port', pointer, pointer, unsigned)
    queue = function('dummy_audio_port_queue_data', None, pointer, unsigned, c.POINTER(c.c_float))
    request = function('dummy_audio_port_request_data', None, pointer, unsigned)
    dequeue = function('dummy_audio_port_dequeue_data', None, pointer, unsigned, c.POINTER(c.c_float))
    request_frames = function('dummy_audio_request_controlled_frames', None, pointer, unsigned)
    remaining = function('dummy_audio_n_requested_frames', unsigned, pointer)
    destroy_driver = function('destroy_audio_driver', None, pointer)
    destroy_session = function('destroy_backend_session', None, pointer)
    session, driver = create_session(), create_driver(2, None)
    assert session and driver, 'could not create backend/driver'
    start(driver, Settings(b'benchmark', 48000, frames)); controlled(driver)
    assert set_driver(session, driver) == 0
    function('wait_process', None, pointer)(driver)
    assert function('get_buffer_size', unsigned, pointer)(driver) == frames
    assert function('get_sample_rate', unsigned, pointer)(driver) == 48000
    chain = create_chain(session, 0, b'benchmark')
    assert chain, 'Carla chain unavailable'
    deadline = time.monotonic()+10
    while True:
        result = status(chain); ready = result.contents.ready; free_status(result)
        if ready: break
        assert time.monotonic() < deadline, 'chain initialization timed out'
        time.sleep(.01)
    xml = Path(state_path).read_bytes()
    state = {'http://kxstudio.sf.net/ns/carla/chunk': {'type': 'http://lv2plug.in/ns/ext/atom#String',
              'value': base64.b64encode(xml+b'\0').decode()}}
    restore(chain, json.dumps(state).encode())
    time.sleep(.2)
    active(chain, 1)
    total = (blocks+200)*frames
    outputs = []
    for channel in range(2):
        source = open_port(session, driver, f'source{channel}'.encode(), 0, 0)
        sink = open_port(session, driver, f'output{channel}'.encode(), 1, 0)
        assert source and sink
        connect(source, fx_input(chain, channel)); connect(fx_output(chain, channel), sink)
        data = (c.c_float*total)(*([.125*(channel+1)]*total))
        queue(source, total, data); request(sink, total); outputs.append(sink)
    time.sleep(.1)
    request_frames(driver, total)
    deadline = time.monotonic()+max(30, total/48000*3)
    while remaining(driver):
        assert time.monotonic() < deadline, 'processing timed out'
        time.sleep(.01)
    time.sleep(.05)
    for channel, sink in enumerate(outputs):
        data = (c.c_float*total)(); dequeue(sink, total, data)
        expected = .125*(channel+1)
        if b'shoop_benchmark' in xml:
            for _ in range(192):
                expected = c.c_float(c.c_float(expected*.9999).value + .00001).value
            expected *= .5
        elif b'audiogain_s' in xml: expected *= .5
        assert all(abs(v-expected)<.00002 for v in data[200*frames:]), 'invalid historical output'
    destroy_session(session); destroy_driver(driver)
    capture = Path(os.environ['SHOOP_BENCH_CAPTURE']).read_bytes()
    count, capacity, rate, configured = struct.unpack_from('QQQQ', capture)
    assert rate == 48000 and configured == frames, ('invalid configured period', rate, configured)
    assert count < capacity and count >= blocks+200, ('missing process calls', count)
    samples = [struct.unpack_from('QQQQ', capture,32+i*32) for i in range(count)]
    samples = [v for v in samples if v[3] == frames]
    assert len(samples) == blocks+200, ('unexpected process sizes/count', len(samples))
    with open(output,'w') as f:
        writer=csv.writer(f); writer.writerow(['block','wall_ns','cpu_ns'])
        for i,(a,b,cpu,n) in enumerate(samples[200:]): writer.writerow([i,b-a,cpu])
    print(f'validated as-shipped 0.2.0 frames={frames} blocks={blocks}; timings are inner LV2 calls only')


if __name__ == '__main__':
    main()
