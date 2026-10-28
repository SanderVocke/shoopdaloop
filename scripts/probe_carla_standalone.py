#!/usr/bin/env python3
"""Verify packaged Carla GUI startup and actual standalone-engine DSP without ShoopDaLoop."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import struct
import subprocess
import sys
import tempfile
import time
import wave


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--carla-source', type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    env = os.environ.copy()
    for key in ['DYLD_LIBRARY_PATH', 'DYLD_FALLBACK_LIBRARY_PATH', 'PYTHONPATH', 'PYTHONHOME',
                'SHOOP_CARLA_NATIVE_LIBRARY', 'SHOOP_CARLA_RESOURCE_DIR']:
        env.pop(key, None)
    results = {}
    with tempfile.TemporaryDirectory() as directory:
        temporary = Path(directory)
        env['XDG_CONFIG_HOME'] = str(temporary / 'config')
        for name in ['Carla.app', 'Carla-Control.app']:
            app = root / name
            info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
            executable = app / 'Contents/MacOS' / info['CFBundleExecutable']
            result = subprocess.run([str(executable), '--version'], env=env, cwd=temporary,
                                    text=True, capture_output=True, timeout=30)
            if result.returncode or '2.5.10' not in result.stdout + result.stderr:
                raise RuntimeError(f'{name} version probe failed: {result.stdout}\n{result.stderr}')
            with (root / (name + '.startup.log')).open('w') as log:
                process = subprocess.Popen([str(executable)], env=env, cwd=temporary, stdout=log, stderr=subprocess.STDOUT)
                try:
                    time.sleep(6)
                    if process.poll() is not None:
                        raise RuntimeError(f'{name} GUI exited: {process.returncode}')
                finally:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill(); process.wait()
            text = (root / (name + '.startup.log')).read_text()
            if any(message in text for message in ['Traceback (most recent call last)', 'Library not loaded:', 'Could not load the Qt platform plugin']):
                raise RuntimeError(f'{name} GUI startup reported an error: {text}')
            results[name] = {'version': '2.5.10', 'gui_alive_after_seconds': 6}
        sys.path.insert(0, str(args.carla_source.resolve() / 'source/frontend'))
        import carla_backend as carla
        binary_dir = root / 'Carla.app/Contents/MacOS'
        host = carla.CarlaHostDLL(str(binary_dir / 'libcarla_standalone2.dylib'), False)
        host.set_engine_option(carla.ENGINE_OPTION_PROCESS_MODE, carla.ENGINE_PROCESS_MODE_CONTINUOUS_RACK, '')
        host.set_engine_option(carla.ENGINE_OPTION_TRANSPORT_MODE, carla.ENGINE_TRANSPORT_MODE_INTERNAL, '')
        host.set_engine_option(carla.ENGINE_OPTION_AUDIO_BUFFER_SIZE, 1024, '')
        host.set_engine_option(carla.ENGINE_OPTION_AUDIO_SAMPLE_RATE, 48000, '')
        host.set_engine_option(carla.ENGINE_OPTION_PATH_BINARIES, 0, str(binary_dir))
        host.set_engine_option(carla.ENGINE_OPTION_PATH_RESOURCES, 0, str(binary_dir / 'resources'))
        if not host.engine_init('Dummy', 'standalone-package-probe'):
            raise RuntimeError(host.get_last_error())
        try:
            tone = temporary / 'constant.wav'
            with wave.open(str(tone), 'wb') as writer:
                writer.setnchannels(2); writer.setsampwidth(2); writer.setframerate(48000)
                writer.writeframes(struct.pack('<hh', 8192, 8192) * (48000 * 4))
            for label in ['audiofile', 'audiogain_s']:
                if not host.add_plugin(carla.BINARY_NATIVE, carla.PLUGIN_INTERNAL, None, label, label, 0, None, 0):
                    raise RuntimeError(host.get_last_error())
            host.set_custom_data(0, carla.CUSTOM_DATA_TYPE_STRING, 'file', str(tone))
            host.set_parameter_value(1, 0, 0.5)
            host.set_active(0, True); host.set_active(1, True); host.transport_play()
            source = gain = 0.0
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                host.engine_idle()
                source = host.get_output_peak_value(0, True)
                gain = host.get_output_peak_value(1, True)
                if 0.2 < source < 0.3 and 0.09 < gain < 0.15 and abs(gain / source - 0.5) < 0.02:
                    break
                time.sleep(0.02)
            else:
                raise RuntimeError(f'Standalone DSP produced wrong output: source={source}, gain={gain}')
            if host.get_buffer_size() != 1024 or host.get_sample_rate() != 48000:
                raise RuntimeError('Standalone engine period mismatch')
            results['standalone_processing'] = {'driver': 'Dummy', 'frames': 1024, 'sample_rate': 48000,
                                                'source_peak': source, 'gain_peak': gain, 'expected_ratio': 0.5}
        finally:
            host.engine_close()
    (root / 'standalone-probe.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(results, indent=2))


if __name__ == '__main__':
    main()
