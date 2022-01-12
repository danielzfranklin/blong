#![no_main]
#![no_std]

// use panic_halt as _;
use panic_probe as _;

use defmt_rtt as _;
use defmt::{debug, info, warn, error};

use nrf52840_hal as hal;

use hal::pac::{CorePeripherals, Peripherals};
use hal::prelude::*;
use hal::gpio::Level;
use hal::delay::Delay;

use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    
    let p = Peripherals::take().unwrap();
    let core = CorePeripherals::take().unwrap();

    let port0 = hal::gpio::p0::Parts::new(p.P0);
    let mut led = port0.p0_06.into_push_pull_output(Level::Low);
    
    let mut delay = Delay::new(core.SYST);

    warn!("Starting");
    
    loop {       
        info!("Loop top");

        debug!("Setting led high");
        led.set_high().unwrap();
        delay.delay_ms(2000_u32);

        debug!("Setting led low");
        led.set_low().unwrap();
        delay.delay_ms(500_u32);
    }
}
