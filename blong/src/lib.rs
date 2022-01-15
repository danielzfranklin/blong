#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

pub mod attrs;
// pub mod log_to_defmt;
pub mod timer;

pub use nrf52840_hal as hal;

use defmt_rtt as _;
use panic_probe as _;

#[allow(unused_imports)]
use defmt::{debug, error, info, warn, Format};

use alloc_cortex_m::CortexMHeap;

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! {
    panic!("OOM");
}

/// # Safety
/// Must be called zero or once
pub unsafe fn init_allocator() {
    ALLOCATOR.init(
        cortex_m_rt::heap_start() as usize,
        1024, // in bytes
    )
}

// See https://crates.io/crates/defmt-test/0.3.0 for more documentation (e.g. about the 'state'
// feature)
//
// Version 0.3.0 of defmt_test supports only one unit test module
#[defmt_test::tests]
mod tests {
    use defmt::assert;

    #[test]
    fn it_works() {
        assert!(true);
    }
}
