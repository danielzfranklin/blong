#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

#[rtic::app(device = rp_pico::hal::pac, peripherals = true)]
mod app {
    use embedded_hal::digital::v2::OutputPin;
    use embedded_time::duration::Extensions;
    use rp_pico as bsp;

    #[allow(unused)]
    use defmt::{debug, error, info, panic, trace, warn};

    use bsp::{
        hal::{self, clocks::init_clocks_and_plls, watchdog::Watchdog, Sio},
        XOSC_CRYSTAL_FREQ,
    };

    #[shared]
    struct Shared {
        indicator: hal::gpio::Pin<hal::gpio::pin::bank0::Gpio25, hal::gpio::PushPullOutput>,
    }

    #[local]
    struct Local {}

    #[init]
    fn init(c: init::Context) -> (Shared, Local, init::Monotonics) {
        info!("init");

        let mut resets = c.device.RESETS;
        let mut watchdog = Watchdog::new(c.device.WATCHDOG);
        let _clocks = init_clocks_and_plls(
            XOSC_CRYSTAL_FREQ,
            c.device.XOSC,
            c.device.CLOCKS,
            c.device.PLL_SYS,
            c.device.PLL_USB,
            &mut resets,
            &mut watchdog,
        )
        .ok()
        .unwrap();

        let sio = Sio::new(c.device.SIO);
        let pins = rp_pico::Pins::new(
            c.device.IO_BANK0,
            c.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );

        let mut indicator = pins.led.into_push_pull_output();
        indicator.set_low().unwrap();

    }

    #[task(
        binds = TIMER_IRQ_0,
        priority = 1,
        shared = [timer, alarm, led],
        local = [tog: bool = true],
    )]
    fn timer_irq(mut c: timer_irq::Context) {
        if *c.local.tog {
            c.shared.led.lock(|l| l.set_high().unwrap());
            debug!("Toggled on");
        } else {
            c.shared.led.lock(|l| l.set_low().unwrap());
            debug!("Toggled off");
        }
        *c.local.tog = !*c.local.tog;

        let timer = c.shared.timer;
        let alarm = c.shared.alarm;
        (timer, alarm).lock(|t, a| {
            a.clear_interrupt(t);
            let _ = a.schedule(SCAN_TIME_US.microseconds());
        });
        (Shared { indicator }, Local {}, init::Monotonics())
    }
}
