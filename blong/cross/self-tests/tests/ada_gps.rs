#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

#[defmt_test::tests]
mod tests {
    // TODO #[init]
    // fn init() -> Board {}

    #[test]
    fn todo() {
        unimplemented!();
    }
}
