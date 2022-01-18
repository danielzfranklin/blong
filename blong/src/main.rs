#![no_std]
#![no_main]

use blong as _;

defmt::timestamp!("{=u64:us}", app::monotonics::AppMono::now().ticks());

#[rtic::app(device = rp_pico::hal::pac, peripherals = true, dispatchers = [DMA_IRQ_0])]
mod app {
    use blong::{gps::Gps, prelude::*};
    use cortex_m::prelude::_embedded_hal_serial_Read;
    use embedded_hal::digital::v2::OutputPin;
    use rp2040_monotonic::Rp2040Monotonic;
    use rp_pico::{
        hal::{
            self,
            clocks::init_clocks_and_plls,
            uart::{self, ReadErrorType, UartPeripheral},
            watchdog::Watchdog,
            Clock, Sio,
        },
        pac::{Interrupt, UART0},
        Gp16Uart0Tx, Gp17Uart0Rx, XOSC_CRYSTAL_FREQ,
    };

    #[monotonic(binds = TIMER_IRQ_0)]
    type AppMono = Rp2040Monotonic;

    type ActivityIndicatorPin =
        hal::gpio::Pin<hal::gpio::pin::bank0::Gpio25, hal::gpio::PushPullOutput>;

    // About 8 maximum size packets
    const GPS_UART_INCOMING_SIZE: usize = 2048;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        activity_indicator: ActivityIndicatorPin,
        gps: Gps,
        gps_uart: UartPeripheral<uart::Enabled, UART0, (Gp16Uart0Tx, Gp17Uart0Rx)>,
        gps_uart_incoming_tx: heapless::spsc::Producer<'static, u8, GPS_UART_INCOMING_SIZE>,
        gps_uart_incoming_rx: heapless::spsc::Consumer<'static, u8, GPS_UART_INCOMING_SIZE>,
    }

    #[init(
        local = [
            gps_uart_incoming: heapless::spsc::Queue::<u8, GPS_UART_INCOMING_SIZE> = heapless::spsc::Queue::new()
        ]
    )]
    fn init(c: init::Context) -> (Shared, Local, init::Monotonics) {
        unsafe { blong::init_allocator() };

        info!("init");

        // Causes all interrupts to fire an event, allowing us to use wfe (wait for event) in our
        // idle loop. Our idle loop is simple enough this isn't technically necessary (we could just)
        // use `wfi` (wait for interrupt), but this is a "good habit";
        c.device.PPB.scr.modify(|_r, w| w.sevonpend().set_bit());

        let mut resets = c.device.RESETS;
        let mut watchdog = Watchdog::new(c.device.WATCHDOG);
        let clocks = init_clocks_and_plls(
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

        let app_mono = Rp2040Monotonic::new(c.device.TIMER);

        let sio = Sio::new(c.device.SIO);
        let pins = rp_pico::Pins::new(
            c.device.IO_BANK0,
            c.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );

        let mut activity_indicator = pins.led.into_push_pull_output();
        activity_indicator.set_low().unwrap();

        let mut gps_uart = UartPeripheral::new(
            c.device.UART0,
            (pins.gpio16.into_mode(), pins.gpio17.into_mode()),
            &mut resets,
        )
        .enable(
            uart::common_configs::_9600_8_N_1,
            clocks.peripheral_clock.freq(),
        )
        .unwrap();

        gps_uart.enable_rx_interrupt();
        Gps::write_on_cmd(&mut gps_uart);

        let (gps_uart_incoming_tx, gps_uart_incoming_rx) = c.local.gps_uart_incoming.split();

        (
            Shared {},
            Local {
                activity_indicator,
                gps: Gps::new(),
                gps_uart,
                gps_uart_incoming_rx,
                gps_uart_incoming_tx,
            },
            init::Monotonics(app_mono),
        )
    }

    #[idle(local = [activity_indicator, gps, gps_uart_incoming_rx])]
    fn idle(c: idle::Context) -> ! {
        let activity_indicator = c.local.activity_indicator;
        let gps = c.local.gps;
        let gps_uart_incoming_rx = c.local.gps_uart_incoming_rx;

        loop {
            cortex_m::asm::wfe();
            activity_indicator.set_high().unwrap();

            while let Some(byte) = gps_uart_incoming_rx.dequeue() {
                gps.accept_byte(byte);
            }

            cortex_m::asm::delay(100_000);
            activity_indicator.set_low().unwrap();
        }
    }

    #[task(binds = UART0_IRQ, local = [gps_uart, gps_uart_incoming_tx], priority = 2)]
    fn uart0(c: uart0::Context) {
        hal::pac::NVIC::unpend(Interrupt::UART0_IRQ);

        let uart = c.local.gps_uart;
        let incoming_tx = c.local.gps_uart_incoming_tx;

        // NOTE: Errors can be caused by things like starting the pico in the
        //   middle of a message.

        match uart.read() {
            Ok(byte) => match incoming_tx.enqueue(byte) {
                Ok(_) => (),
                Err(_) => {
                    error!("uart incoming out of space, dropping");
                }
            },
            Err(nb::Error::WouldBlock) => (),
            Err(nb::Error::Other(err)) => match err {
                ReadErrorType::Overrun => error!("Uart read failed: Overrun"),
                ReadErrorType::Break => warn!("Uart read failed: Break"),
                ReadErrorType::Parity => error!("Uart read failed: Parity"),
                ReadErrorType::Framing => error!("Uart read failed: Framing"),
            },
        };
    }
}
