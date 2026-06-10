//! CXX bridge for the C++ MidiPort object and bridge handles.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/MidiPortCxxBridge.h");

        type MidiPort;
        type MidiPortBridgeStrong;
        type MidiPortBridgeWeak;

        fn downgrade(self: &MidiPortBridgeStrong) -> UniquePtr<MidiPortBridgeWeak>;
        fn upgrade(self: &MidiPortBridgeWeak) -> UniquePtr<MidiPortBridgeStrong>;
        fn clone(self: &MidiPortBridgeWeak) -> UniquePtr<MidiPortBridgeWeak>;
        fn valid(self: &MidiPortBridgeStrong) -> bool;
        fn get_ref(self: &MidiPortBridgeStrong) -> &MidiPort;
        unsafe fn get_pin_mut(self: Pin<&mut MidiPortBridgeStrong>) -> Pin<&mut MidiPort>;

        fn midi_port_prepare(port: Pin<&mut MidiPort>, nframes: u32);
        fn midi_port_process(port: Pin<&mut MidiPort>, nframes: u32);
        fn midi_port_close(port: Pin<&mut MidiPort>);
        fn midi_port_get_read_output_data_buffer(port: Pin<&mut MidiPort>, nframes: u32) -> usize;
        fn midi_port_get_write_data_into_port_buffer(
            port: Pin<&mut MidiPort>,
            nframes: u32,
        ) -> usize;
        fn midi_readable_buffer_n_events(buffer_ptr: usize) -> u32;
        unsafe fn midi_readable_buffer_get_event(
            buffer_ptr: usize,
            idx: u32,
            out_time: &mut u32,
            out_size: &mut u16,
            out_data: *mut u8,
        ) -> bool;
        unsafe fn midi_writeable_buffer_write_event(
            buffer_ptr: usize,
            time: u32,
            size: u16,
            data: *const u8,
        ) -> bool;
    }
}
