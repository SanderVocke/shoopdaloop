#![no_std]
use core::sync::atomic::{AtomicUsize, Ordering};
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ffi::c_void,
    fmt::Write,
};
use linked_list_allocator::Heap;

extern "C" {
    static mut __rust_heap: u8;
    static mut __rust_heap_end: u8;
    fn shoop_allocator_lock();
    fn shoop_allocator_unlock();
    fn shoop_in_render() -> i32;
    fn shoop_log(bytes: *const u8, len: usize);
    fn shoop_halt() -> !;
}
struct Allocator(UnsafeCell<Heap>);
unsafe impl Sync for Allocator {}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        assert_eq!(shoop_in_render(), 0, "render allocation");
        shoop_allocator_lock();
        let result = (*self.0.get())
            .allocate_first_fit(layout)
            .map_or(core::ptr::null_mut(), |p| p.as_ptr());
        PEAK.fetch_max((*self.0.get()).used(), Ordering::Relaxed);
        shoop_allocator_unlock();
        result
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        assert_eq!(shoop_in_render(), 0, "render deallocation");
        shoop_allocator_lock();
        (*self.0.get()).deallocate(core::ptr::NonNull::new_unchecked(ptr), layout);
        shoop_allocator_unlock();
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator(UnsafeCell::new(Heap::empty()));
static PEAK: AtomicUsize = AtomicUsize::new(0);
struct Serial;
impl core::fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        unsafe {
            shoop_log(s.as_ptr(), s.len());
        }
        Ok(())
    }
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    let _ = writeln!(Serial, "FATAL Rust: {info}");
    unsafe { shoop_halt() }
}
#[no_mangle]
pub unsafe extern "C" fn shoop_rust_main() {
    let start = core::ptr::addr_of_mut!(__rust_heap);
    let size = core::ptr::addr_of_mut!(__rust_heap_end).addr() - start.addr();
    (*ALLOCATOR.0.get()).init(start, size);
    #[cfg(feature = "fixture")]
    {
        let baseline = (*ALLOCATOR.0.get()).used();
        for alignment in [8, 32, 4096] {
            let layout = Layout::from_size_align(513, alignment).unwrap();
            let allocation = core::ptr::NonNull::new(ALLOCATOR.alloc(layout))
                .expect("allocator alignment probe allocation");
            let buffer = allocation.as_ptr();
            assert!(buffer.addr().is_multiple_of(alignment));
            buffer.write_volatile(0x5a);
            buffer.add(512).write_volatile(0xa5);
            assert_eq!(buffer.read_volatile(), 0x5a);
            ALLOCATOR.dealloc(buffer, layout);
        }
        assert_eq!((*ALLOCATOR.0.get()).used(), baseline);
        let atomic = core::sync::atomic::AtomicU64::new(0xffff_ffff);
        assert_eq!(atomic.fetch_add(2, Ordering::SeqCst), 0xffff_ffff);
        assert_eq!(atomic.load(Ordering::SeqCst), 0x1_0000_0001);
        let _ = writeln!(Serial, "SHOOP_ALLOC_ATOMIC_PASS");
    }
    shoop_host_freertos::run();
}
#[no_mangle]
pub unsafe extern "C" fn shoop_render_step(owner: *mut c_void, frames: usize) -> i32 {
    shoop_host_freertos::render_step(owner, frames)
}

#[no_mangle]
pub unsafe extern "C" fn shoop_render_xruns(owner: *mut c_void, count: u32) {
    shoop_host_freertos::render_xruns(owner, count);
}

#[no_mangle]
pub unsafe extern "C" fn shoop_heap_stats(used: *mut usize, peak: *mut usize) {
    shoop_allocator_lock();
    *used = (*ALLOCATOR.0.get()).used();
    *peak = PEAK.load(Ordering::Relaxed);
    shoop_allocator_unlock();
}

#[cfg(feature = "fixture")]
#[no_mangle]
pub unsafe extern "C" fn shoop_fixture_step(
    owner: *mut c_void,
    frames: usize,
    input: u32,
    midi: u32,
) -> i32 {
    shoop_host_freertos::fixture_step(owner, frames, input, midi)
}
#[cfg(feature = "fixture")]
#[no_mangle]
pub unsafe extern "C" fn shoop_fixture_report(
    frame: *mut u32,
    hash: *mut u32,
    midi: *mut u32,
    on: *mut u32,
    off: *mut u32,
    bad: *mut u32,
) {
    let stats = shoop_host_freertos::fixture_stats();
    *frame = stats[0];
    *hash = stats[1];
    *midi = stats[2];
    *on = stats[3];
    *off = stats[4];
    *bad = stats[5];
}
