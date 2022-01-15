#![no_std]
#![no_main]

use blong as _; // global logger + panicking-behavior + memory layout

// This logs decimals seconds
defmt::timestamp!("{=u32:us}", app::monotonics::MonoDefault::now().ticks());

#[rtic::app(
    device = hal::pac,
    peripherals = true,
    dispatchers = [TIMER1]
)]
mod app {
    use blong::timer::MonoTimer;
    #[allow(unused_imports)]
    use defmt::{debug, error, info, warn, Format};

    use apa102_spi::Apa102;
    use hal::gpio::Level;
    use hal::spi::{self, Spi};
    use nrf52840_hal as hal;
    use smart_leds_trait::{SmartLedsWrite, RGB8};

    use hal::gpiote::Gpiote;
    use hal::pac::{spi0, SPI0, TIMER0};

    // A monotonic timer to enable scheduling in RTIC
    #[monotonic(binds = TIMER0, default = true)]
    type MonoDefault = MonoTimer<TIMER0>;

    type DotStar = Apa102<Spi<SPI0>>;

    #[shared]
    struct Shared {
        dotstar: DotStar,
    }

    #[local]
    struct Local {
        gpiote: Gpiote,
    }

    #[init]
    fn init(mut cx: init::Context) -> (Shared, Local, init::Monotonics) {
        // Setup timers
        let mono_default = MonoTimer::new(cx.device.TIMER0);

        // Setup sleep
        //   Set the ARM SLEEPONEXIT bit to go to sleep after handling interrupts.
        //   See https://developer.arm.com/docs/100737/0100/power-management/sleep-mode/sleep-on-exit-bit
        cx.core.SCB.set_sleepdeep();

        let gpiote = Gpiote::new(cx.device.GPIOTE);
        let port0 = hal::gpio::p0::Parts::new(cx.device.P0);
        let port1 = hal::gpio::p1::Parts::new(cx.device.P1);

        // Setup builtin dotstar led
        let dotstar_pins = spi::Pins {
            // Pin numbers from <https://blog.adafruit.com/2021/05/26/pin-reference-adafruit-itsybitsy-nrf52840-prettypins/>
            sck: port1.p1_09.into_push_pull_output(Level::High).degrade(),
            mosi: Some(port0.p0_08.into_push_pull_output(Level::High).degrade()),
            miso: None,
        };

        let dotstar_spi = Spi::new(
            cx.device.SPI0,
            dotstar_pins,
            spi0::frequency::FREQUENCY_A::K125,
            apa102_spi::MODE,
        );
        // Not inverting end frame b/c adafruit does something similar in their bsp for other boards
        // See <https://github.com/atsamd-rs/atsamd/blob/9495af975d6a35ae8bb455fae29ad0356fe20e09/boards/trinket_m0/src/lib.rs#L155>
        let dotstar = Apa102::new_with_custom_postamble(dotstar_spi, 4, false);

        // Setup builtin button
        gpiote
            .channel0()
            .input_pin(&port0.p0_29.into_pullup_input().degrade())
            .hi_to_lo()
            .enable_interrupt();

        // Spawn task, runs right after init finishes
        startup::spawn().unwrap();

        (
            Shared { dotstar },
            Local { gpiote },
            init::Monotonics(mono_default),
        )
    }

    // Background task, runs whenever no other tasks are running
    #[idle]
    fn idle(_: idle::Context) -> ! {
        loop {
            // Wait For Interrupt is used instead of a busy-wait loop
            // to allow MCU to sleep between interrupts, since we set
            // SLEEPONEXIT in init.
            // https://developer.arm.com/documentation/ddi0406/c/Application-Level-Architecture/Instruction-Details/Alphabetical-list-of-instructions/WFI
            rtic::export::wfi();
        }
    }

    // Software task, also not bound to a hardware interrupt
    #[task(shared = [dotstar])]
    fn startup(mut cx: startup::Context) {
        info!("Starting up");
        cx.shared.dotstar.lock(|dotstar| {
            dotstar.write([RGB8::new(64, 64, 64)].into_iter()).unwrap();
        });
    }

    #[task(binds = GPIOTE, local = [gpiote], shared = [dotstar])]
    fn on_gpiote(mut cx: on_gpiote::Context) {
        let gpiote = cx.local.gpiote;

        if gpiote.channel0().is_event_triggered() {
            debug!("Button press");

            cx.shared.dotstar.lock(|dotstar| {
                dotstar.write([RGB8::new(0, 0, 64)].into_iter()).unwrap();
            });
        }

        gpiote.reset_events();
    }
}
