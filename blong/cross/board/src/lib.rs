#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;
use core::alloc::Layout;
use panic_probe as _;

pub use cortex_m;
pub use embedded_hal;
pub use nb;
pub use rp2040_monotonic;
pub use rp_pico;

use alloc_cortex_m::CortexMHeap;
use asm_delay::AsmDelay;
use cortex_m::{delay::Delay, peripheral::NVIC};
use embedded_hal::{digital::v2::OutputPin, watchdog::WatchdogEnable as _};
use embedded_time::{duration::Extensions as _, fixed_point::FixedPoint as _};
use rp2040_monotonic::Rp2040Monotonic;
use rp_pico::{
    hal::{
        clocks::init_clocks_and_plls,
        gpio::{bank0::Gpio25, Pin, PushPullOutput},
        uart::{self, UartPeripheral},
        Clock, Sio, Watchdog,
    },
    pac::{self, Interrupt, UART0},
    Gp16Uart0Tx, Gp17Uart0Rx, XOSC_CRYSTAL_FREQ,
};
use rtt_target::rtt_init;

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

// The pico has 264KB of SRAM

#[alloc_error_handler]
fn oom(_: Layout) -> ! {
    panic!("oom")
}

/// # Safety
/// This function must be called exactly once.
unsafe fn init_allocator() {
    crate::ALLOCATOR.init(
        cortex_m_rt::heap_start() as usize,
        2_usize.pow(15), // about 12% of the total memory
    );
}

pub type StatusLed = Pin<Gpio25, PushPullOutput>;
pub type GpsUartReader = uart::Reader<UART0, (Gp16Uart0Tx, Gp17Uart0Rx)>;
pub type GpsUartWriter = uart::Writer<UART0, (Gp16Uart0Tx, Gp17Uart0Rx)>;
pub type GpsDelay = AsmDelay;

pub struct Board {
    pub watchdog: Watchdog,
    pub delay: Delay,
    pub status_led: StatusLed,
    pub gps_uart_reader: GpsUartReader,
    pub gps_uart_writer: GpsUartWriter,
    pub gps_delay: AsmDelay,
    pub mono: Rp2040Monotonic,
}

impl Board {
    pub fn init(core: cortex_m::Peripherals, device: pac::Peripherals) -> Self {
        unsafe {
            init_allocator();
        }

        init_needed_rtt();

        // Causes all interrupts to fire an event, allowing us to use wfe (wait for event) in our
        // idle loop. Our idle loop is simple enough this isn't technically necessary (we could just)
        // use `wfi` (wait for interrupt), but this is a "good habit";
        device.PPB.scr.modify(|_r, w| w.sevonpend().set_bit());

        let mut resets = device.RESETS;

        let mut watchdog = Watchdog::new(device.WATCHDOG);
        // Set to watchdog to reset if it's not reloaded within 1.05 seconds
        watchdog.start(1_050_000u32.microseconds());

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

        // NOTE: I'm not sure this is the right frequency
        let cpu_freq_hz = clocks.system_clock.freq().integer();
        let delay = Delay::new(core.SYST, cpu_freq_hz);
        let gps_delay = AsmDelay::new(asm_delay::bitrate::Hertz(cpu_freq_hz));

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

        let (mut gps_uart_reader, gps_uart_writer) = UartPeripheral::new(
            device.UART0,
            (pins.gpio16.into_mode(), pins.gpio17.into_mode()),
            &mut resets,
        )
        .enable(
            uart::common_configs::_9600_8_N_1,
            clocks.peripheral_clock.freq(),
        )
        .unwrap()
        .split();
        gps_uart_reader.enable_rx_interrupt();

        let mono = Rp2040Monotonic::new(device.TIMER);

        Self {
            watchdog,
            delay,
            status_led,
            gps_uart_reader,
            gps_uart_writer,
            gps_delay,
            mono,
        }
    }

    pub fn unpend(interrupt: Interrupt) {
        NVIC::unpend(interrupt)
    }
}

#[cfg(not(feature = "rtt-print"))]
fn init_needed_rtt() {
    let channels = rtt_init! {
        up: {
            0: {
                size: 1024
                name: "defmt_rtt"
            }
        }
    };

    defmt_rtt_target::init(channels.up.0);
}

#[cfg(feature = "rtt-print")]
fn init_needed_rtt() {
    let channels = rtt_init! {
        up: {
            0: {
                size: 1024
                mode: NoBlockSkip
                name: "Defmt"
            }
            1: {
                // We block on buffer full with a massive buffer (2^15
                // bytes, 12% of total memory) because we use this for
                // dumping complete traffic, where partial data is useless.
                size: 32768
            }
        }
    };

    defmt_rtt_target::init(channels.up.0);
    rtt_target::set_print_channel(channels.up.1);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
