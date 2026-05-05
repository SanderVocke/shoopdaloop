#!/usr/bin/env python3
import os
import re

files_to_fix = [
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_composite_loop_backend_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_composite_loop_gui_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_file_io_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_fx_chain_backend_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_fx_chain_gui_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_global_utils_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_logger_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_loop_backend_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_loop_channel_backend_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_loop_channel_gui_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_loop_gui_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_lua_engine_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_midi_control_port_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_port_backend_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_port_gui_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_qmlengine_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_render_audio_waveform_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_session_control_handler_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_test_screen_grabber_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_tracing_capture_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/qobj_update_thread_bridge.rs',
    'src/rust/frontend/src/cxx_qt_shoop/rust/test/qobj_test_file_runner_bridge.rs',
]

for filepath in files_to_fix:
    with open(filepath, 'r') as f:
        content = f.read()
    
    # Skip if already has QObject declared via QtCore
    if 'include!(<QtCore/QObject>)' in content:
        print(f"Skip (has QObject): {filepath}")
        continue
    
    # Find the #[cxx_qt::bridge] followed by pub mod ffi { and unsafe extern "C++" {
    pattern = r'(#\[cxx_qt::bridge\]\npub mod ffi \{\n    unsafe extern "C++" \{\n)'
    match = re.search(pattern, content)
    
    if match:
        insert_content = '''#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!(<QtCore/QObject>);
        type QObject;

'''
        content = content[:match.start()] + insert_content + content[match.end():]
        with open(filepath, 'w') as f:
            f.write(content)
        print(f"Fixed: {filepath}")
    else:
        # Alternative: check for any extern "C++" right after #[cxx_qt::bridge]
        alt_pattern = r'(#\[cxx_qt::bridge\]\npub mod ffi \{\s*unsafe extern "C++" \{\s*)'
        match = re.search(alt_pattern, content)
        if match:
            insert_content = match.group(1) + '''include!(<QtCore/QObject>);
        type QObject;

'''
            content = content[:match.start()] + insert_content + content[match.end():]
            with open(filepath, 'w') as f:
                f.write(content)
            print(f"Fixed (alt): {filepath}")
        else:
            print(f"Could not match pattern in: {filepath}")
            # Show the relevant part for debugging
            start = content.find('#[cxx_qt::bridge]')
            if start >= 0:
                print(f"  Context: {content[start:start+200]}")

