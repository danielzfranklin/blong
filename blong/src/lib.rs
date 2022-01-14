#![no_std]
#![no_main]

pub mod timer;

use defmt_rtt as _;
use nrf52840_hal as _;
use panic_probe as _;

#[allow(unused_imports)]
use defmt::{debug, error, info, warn, Format};

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
