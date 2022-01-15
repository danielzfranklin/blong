#![no_std]
#![no_main]

use blong as _; // global logger + panicking-behavior + memory layout

// This logs decimals seconds
defmt::timestamp!("{=u32:us}", app::monotonics::MonoDefault::now().ticks());

#[rtic::app(
    device = hal::pac,
    peripherals = true,
    dispatchers = [TIMER1, TIMER3]
)]
mod app {
    use blong::timer::MonoTimer;
    use blong::{attrs, hal};

    use defmt::Display2Format;
    #[allow(unused_imports)]
    use defmt::{debug, error, info, warn, Format};

    use hal::{
        gpio::{Level, Output, Pin},
        gpiote::Gpiote,
        pac::TIMER0,
        prelude::*,
    };
    use nrf52840_hal::gpio::PushPull;
    use rubble::{
        l2cap::{BleChannelMap, L2CAPState},
        link::{
            ad_structure::{AdStructure, Flags},
            queue::{PacketQueue, SimpleQueue},
            LinkLayer, Responder, MIN_PDU_BUF,
        },
        security::NoSecurity,
        time::Timer,
    };
    use rubble_nrf5x::{
        radio::{BleRadio, PacketBuffer},
        timer::BleTimer,
        utils::get_device_address,
    };

    // A monotonic timer to enable scheduling in RTIC
    #[monotonic(binds = TIMER0, default = true)]
    type MonoDefault = MonoTimer<TIMER0>;

    pub enum BleAppConfig {}

    impl rubble::config::Config for BleAppConfig {
        type Timer = BleTimer<hal::pac::TIMER2>;
        type Transmitter = BleRadio;
        type ChannelMapper = BleChannelMap<attrs::DemoAttrs, NoSecurity>;
        type PacketQueue = &'static mut SimpleQueue;
    }

    #[shared]
    struct Shared {
        indicator_led: Pin<Output<PushPull>>,
        ble_ll: LinkLayer<BleAppConfig>,
        ble_r: Responder<BleAppConfig>,
        radio: BleRadio,
    }

    #[local]
    struct Local {
        gpiote: Gpiote,
    }

    #[init(
        local = [
            ble_tx_buf: PacketBuffer = [0; MIN_PDU_BUF],
            ble_rx_buf: PacketBuffer = [0; MIN_PDU_BUF],
            tx_queue: SimpleQueue = SimpleQueue::new(),
            rx_queue: SimpleQueue = SimpleQueue::new(),
        ]
    )]
    fn init(mut cx: init::Context) -> (Shared, Local, init::Monotonics) {
        unsafe {
            // Safety: We only call once, per contract
            // TODO blong::init_allocator();
            // TODO blong::log_to_defmt::init();
        }

        // Setup timer
        let mono_default = MonoTimer::new(cx.device.TIMER0);

        // Setup sleep
        //   Set the ARM SLEEPONEXIT bit to go to sleep after handling interrupts.
        //   See https://developer.arm.com/docs/100737/0100/power-management/sleep-mode/sleep-on-exit-bit
        cx.core.SCB.set_sleepdeep();

        let gpiote = Gpiote::new(cx.device.GPIOTE);
        let p0 = hal::gpio::p0::Parts::new(cx.device.P0);

        // Setup builtin button
        gpiote
            .channel0()
            .input_pin(&p0.p0_29.into_pullup_input().degrade())
            .hi_to_lo()
            .enable_interrupt();

        // Setup builtin indicator
        let indicator_led = p0.p0_06.into_push_pull_output(Level::Low).degrade();

        // Setup BLE

        // On reset, the internal high frequency clock is already used, but we
        // also need to switch to the external HF oscillator. This is needed
        // for Bluetooth to work.
        let _clocks = hal::clocks::Clocks::new(cx.device.CLOCK).enable_ext_hfosc();

        let ble_timer = BleTimer::init(cx.device.TIMER2);

        // Determine device address
        let device_address = get_device_address();
        info!("Device address: {}", Display2Format(&device_address));

        let mut radio = BleRadio::new(
            cx.device.RADIO,
            &cx.device.FICR,
            cx.local.ble_tx_buf,
            cx.local.ble_rx_buf,
        );

        // Create TX/RX queues
        let (tx, tx_cons) = cx.local.tx_queue.split();
        let (rx_prod, rx) = cx.local.rx_queue.split();

        // Create the actual BLE stack objects
        let mut ble_ll = LinkLayer::<BleAppConfig>::new(device_address, ble_timer);

        let ble_indicator_led = p0.p0_04.into_push_pull_output(Level::High);

        let ble_r = Responder::new(
            tx,
            rx,
            L2CAPState::new(BleChannelMap::with_attributes(attrs::DemoAttrs::new(
                ble_indicator_led.degrade(),
            ))),
        );

        // Send advertisement and set up regular interrupt
        let next_update = ble_ll
            .start_advertise(
                rubble::time::Duration::from_millis(200),
                &[
                    // TODO: AdStructure::Flags(Flags::discoverable()),
                    AdStructure::CompleteLocalName("Blong"),
                ],
                &mut radio,
                tx_cons,
                rx_prod,
            )
            .unwrap();

        ble_ll.timer().configure_interrupt(next_update);

        // Spawn task, runs right after init finishes
        startup::spawn().unwrap();

        (
            Shared {
                indicator_led,
                ble_ll,
                ble_r,
                radio,
            },
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
            // TODO: rtic::export::wfi();
            continue;
        }
    }

    // Software task, also not bound to a hardware interrupt
    #[task]
    fn startup(_cx: startup::Context) {
        info!("Starting up");
    }

    #[task(
        binds = GPIOTE,
        shared = [indicator_led],
        local = [gpiote, btn_toggled_to: bool = false]
    )]
    fn on_gpiote(cx: on_gpiote::Context) {
        let gpiote = cx.local.gpiote;
        let btn_toggled_to = cx.local.btn_toggled_to;
        let mut indicator_led = cx.shared.indicator_led;

        if gpiote.channel0().is_event_triggered() {
            debug!("Button press");
            *btn_toggled_to = !*btn_toggled_to;
            indicator_led.lock(|indicator_led| {
                if *btn_toggled_to {
                    indicator_led.set_high().unwrap();
                } else {
                    indicator_led.set_low().unwrap();
                }
            });
        }

        gpiote.reset_events();
    }

    #[task(binds = RADIO, shared = [radio, ble_ll], priority = 3)]
    fn radio(cx: radio::Context) {
        (cx.shared.radio, cx.shared.ble_ll).lock(|radio, ble_ll| {
            if let Some(cmd) = radio.recv_interrupt(ble_ll.timer().now(), ble_ll) {
                radio.configure_receiver(cmd.radio);
                ble_ll.timer().configure_interrupt(cmd.next_update);

                if cmd.queued_work {
                    // If there's any lower-priority work to be done, ensure that happens.
                    // If we fail to spawn the task, it's already scheduled.
                    ble_worker::spawn().ok();
                }
            }
        });
    }

    #[task(binds = TIMER2, shared = [radio, ble_ll], priority = 3)]
    fn timer2(cx: timer2::Context) {
        (cx.shared.radio, cx.shared.ble_ll).lock(|radio, ble_ll| {
            let timer = ble_ll.timer();
            if !timer.is_interrupt_pending() {
                return;
            }
            timer.clear_interrupt();

            let cmd = ble_ll.update_timer(radio);
            radio.configure_receiver(cmd.radio);

            ble_ll.timer().configure_interrupt(cmd.next_update);

            if cmd.queued_work {
                // If there's any lower-priority work to be done, ensure that happens.
                // If we fail to spawn the task, it's already scheduled.
                ble_worker::spawn().ok();
            }
        });
    }

    #[task(shared = [ble_r], priority = 2)]
    fn ble_worker(mut cx: ble_worker::Context) {
        // Fully drain the packet queue
        cx.shared.ble_r.lock(|ble_r| {
            while ble_r.has_work() {
                debug!("ble_r has work");
                ble_r.process_one().unwrap();
            }
        })
    }
}
