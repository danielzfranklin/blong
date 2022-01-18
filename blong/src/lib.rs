#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;
use core::alloc::Layout;

use defmt_rtt as _;
use panic_probe as _;

pub mod gps;
pub mod prelude;

use alloc_cortex_m::CortexMHeap;

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[alloc_error_handler]
fn oom(_: Layout) -> ! {
    panic!("oom")
}

/// # Safety
/// This function must be called exactly once.
pub unsafe fn init_allocator() {
    crate::ALLOCATOR.init(
        cortex_m_rt::heap_start() as usize,
        2_usize.pow(18), // about 10% of the total memory
    );
}
