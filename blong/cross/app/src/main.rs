#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

#[rtic::app(device = rp_pico::hal::pac, peripherals = true, dispatchers = [DMA_IRQ_0])]
mod app {
    #[allow(unused)]
    pub use defmt::{debug, error, info, trace, warn};

    use board::{Board, StatusLed};
    use embedded_hal::digital::v2::OutputPin;

    #[monotonic(binds = TIMER_IRQ_0)]
    type AppMono = rp2040_monotonic::Rp2040Monotonic;

    const STATUS_BLINK_CYCLES: u32 = 20_000_000;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        status_led: StatusLed,
    }

    #[init]
    fn init(c: init::Context) -> (Shared, Local, init::Monotonics) {
        info!("init");
        defmt::timestamp!("{=u64:us}", monotonics::AppMono::now().ticks());
        let board = Board::init(c.device);

        (
            Shared {},
            Local {
                status_led: board.status_led,
            },
            init::Monotonics(board.mono),
        )
    }

    #[idle(local = [status_led])]
    fn idle(c: idle::Context) -> ! {
        let status_led = c.local.status_led;

        for _ in 0..2 {
            blink_status_led(status_led);
            cortex_m::asm::delay(STATUS_BLINK_CYCLES);
        }

        loop {
            cortex_m::asm::wfe();
            blink_status_led(status_led);
        }
    }

    fn blink_status_led(led: &mut StatusLed) {
        led.set_high().unwrap();
        cortex_m::asm::delay(STATUS_BLINK_CYCLES);
        led.set_low().unwrap();
    }
}
