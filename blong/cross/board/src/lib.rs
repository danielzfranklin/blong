#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;
use core::alloc::Layout;

use alloc_cortex_m::CortexMHeap;
use embedded_hal::digital::v2::OutputPin;
use rp2040_monotonic::Rp2040Monotonic;
use rp_pico::{
    hal::{
        clocks::init_clocks_and_plls,
        gpio::{bank0::Gpio25, Pin, PushPullOutput},
        uart::{self, UartPeripheral},
        Clock, Sio, Watchdog,
    },
    pac::{self, UART0},
    Gp16Uart0Tx, Gp17Uart0Rx, XOSC_CRYSTAL_FREQ,
};

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[alloc_error_handler]
fn oom(_: Layout) -> ! {
    panic!("oom")
}

/// # Safety
/// This function must be called exactly once.
unsafe fn init_allocator() {
    crate::ALLOCATOR.init(
        cortex_m_rt::heap_start() as usize,
        2_usize.pow(18), // about 10% of the total memory
    );
}

pub type StatusLed = Pin<Gpio25, PushPullOutput>;
pub type GpsUart = UartPeripheral<uart::Enabled, UART0, (Gp16Uart0Tx, Gp17Uart0Rx)>;

pub struct Board {
    pub status_led: StatusLed,
    pub gps_uart: GpsUart,
    pub mono: Rp2040Monotonic,
}

impl Board {
    pub fn init(device: pac::Peripherals) -> Self {
        unsafe {
            init_allocator();
        }

        // Causes all interrupts to fire an event, allowing us to use wfe (wait for event) in our
        // idle loop. Our idle loop is simple enough this isn't technically necessary (we could just)
        // use `wfi` (wait for interrupt), but this is a "good habit";
        device.PPB.scr.modify(|_r, w| w.sevonpend().set_bit());

        let mut resets = device.RESETS;
        let mut watchdog = Watchdog::new(device.WATCHDOG);

        let clocks = init_clocks_and_plls(
            XOSC_CRYSTAL_FREQ,
            device.XOSC,
            device.CLOCKS,
            device.PLL_SYS,
            device.PLL_USB,
            &mut resets,
            &mut watchdog,
        )
        .ok()
        .unwrap();

        // Causes all interrupts to fire an event, allowing us to use wfe (wait for event) in our
        // idle loop. Our idle loop is simple enough this isn't technically necessary (we could just)
        // use `wfi` (wait for interrupt), but this is a "good habit";
        device.PPB.scr.modify(|_r, w| w.sevonpend().set_bit());

        let sio = Sio::new(device.SIO);
        let pins = rp_pico::Pins::new(
            device.IO_BANK0,
            device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );

        let mut status_led = pins.led.into_push_pull_output();
        status_led.set_low().unwrap();

        let gps_uart = UartPeripheral::new(
            device.UART0,
            (pins.gpio16.into_mode(), pins.gpio17.into_mode()),
            &mut resets,
        )
        .enable(
            uart::common_configs::_9600_8_N_1,
            clocks.peripheral_clock.freq(),
        )
        .unwrap();

        let mono = Rp2040Monotonic::new(device.TIMER);

        Self {
            status_led,
            gps_uart,
            mono,
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
