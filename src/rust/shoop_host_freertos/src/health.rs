use profiling_protocol::{
    health::{Snapshot, Temperature},
    Name,
};

#[repr(C)]
#[derive(Default)]
struct Reading {
    current_millicelsius: i32,
    peak_millicelsius: i32,
    flags: u32,
}
const _: () = assert!(core::mem::size_of::<Reading>() == 12);

extern "C" {
    fn shoop_soc_temperature(reading: *mut Reading) -> i32;
}

pub(super) fn sample() -> Snapshot {
    let mut sample = Snapshot::default();
    let mut reading = Reading::default();
    if unsafe { shoop_soc_temperature(&mut reading) } != 0 {
        sample.temperatures[0] = Some(Temperature {
            name: Name::new("SoC die (PS/PL)").expect("static sensor name"),
            current_millicelsius: (reading.flags & 1 != 0).then_some(reading.current_millicelsius),
            peak_millicelsius: (reading.flags & 2 != 0).then_some(reading.peak_millicelsius),
        });
        sample.overtemperature = (reading.flags & 4 != 0).then_some(reading.flags & 8 != 0);
    }
    sample
}
