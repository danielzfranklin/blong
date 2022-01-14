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

    use nrf52840_hal as hal;

    use hal::gpiote::Gpiote;
    use hal::pac::TIMER0;

    // A monotonic timer to enable scheduling in RTIC
    #[monotonic(binds = TIMER0, default = true)]
    type MonoDefault = MonoTimer<TIMER0>;

    #[shared]
    struct Shared {}

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

        let port0 = hal::gpio::p0::Parts::new(cx.device.P0);
        let gpiote = Gpiote::new(cx.device.GPIOTE);

        // Setup button
        gpiote
            .channel0()
            .input_pin(&port0.p0_29.into_pullup_input().degrade())
            .hi_to_lo()
            .enable_interrupt();

        // Spawn task, runs right after init finishes
        startup::spawn().unwrap();

        (Shared {}, Local { gpiote }, init::Monotonics(mono_default))
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
    #[task]
    fn startup(_cx: startup::Context) {
        info!("Starting up");
    }

    #[task(binds = GPIOTE, local = [gpiote])]
    fn on_gpiote(cx: on_gpiote::Context) {
        let gpiote = cx.local.gpiote;

        if gpiote.channel0().is_event_triggered() {
            debug!("Button press");
        }

        gpiote.reset_events();
    }
}
