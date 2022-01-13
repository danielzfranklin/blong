#![no_std]
#![no_main]

use blong as _; // global logger + panicking-behavior + memory layout

use nrf52840_hal as hal;

use hal::delay::Delay;
use hal::gpio::Level;
use hal::pac::{CorePeripherals, Peripherals};
use hal::prelude::*;

use cortex_m_rt::entry;

#[allow(unused_imports)]
use defmt::{debug, error, info, warn, Format};

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let core = CorePeripherals::take().unwrap();

    let port0 = hal::gpio::p0::Parts::new(p.P0);
    let mut led = port0.p0_06.into_push_pull_output(Level::Low);

    let mut delay = Delay::new(core.SYST);

    info!("Starting");

    for _ in 0..5 {
        debug!("Setting led high");
        led.set_high().unwrap();
        delay.delay_ms(2000_u32);

        debug!("Setting led low");
        led.set_low().unwrap();
        delay.delay_ms(500_u32);
    }

    blong::exit();
}
