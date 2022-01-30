#![no_std]
#![no_main]

#[rtic::app(device = rp_pico::hal::pac, peripherals = true, dispatchers = [DMA_IRQ_0])]
mod app {
    #[allow(unused)]
    pub use defmt::{debug, error, info, trace, warn};

    use ada_gps::Gps;
    use bbqueue::BBBuffer;
    use board::{
        cortex_m,
        cortex_m::prelude::*,
        embedded_hal::digital::v2::OutputPin,
        nb, rp2040_monotonic,
        rp_pico::{self, hal::Watchdog, pac::Interrupt},
        Board, GpsDelay, GpsUartReader, GpsUartWriter, StatusLed,
    };

    #[monotonic(binds = TIMER_IRQ_0)]
    type AppMono = rp2040_monotonic::Rp2040Monotonic;

    const STATUS_BLINK_CYCLES: u32 = 5_000_000;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        gps: Gps<'static, GpsUartWriter, GpsDelay>,
        watchdog: Watchdog,
        status_led: StatusLed,
        gps_uart_reader: GpsUartReader,
        gps_rx_producer: ada_gps::RxProducer<'static>,
    }

    #[init(
        local = [
            gps_rx_queue: ada_gps::RxBuf = BBBuffer::new(),
        ]
    )]
    fn init(c: init::Context) -> (Shared, Local, init::Monotonics) {
        info!("Initializing");

        defmt::timestamp!("{=u64:us}", monotonics::AppMono::now().ticks());

        let Board {
            delay: _delay,
            watchdog,
            status_led,
            gps_uart_reader,
            gps_uart_writer,
            gps_delay,
            mono,
        } = Board::init(c.core, c.device);

        let (gps_rx_producer, gps_rx_consumer) = c.local.gps_rx_queue.try_split().unwrap();
        let gps = Gps::new(gps_rx_consumer, gps_uart_writer, gps_delay, false);

        (
            Shared {},
            Local {
                gps,
                watchdog,
                status_led,
                gps_uart_reader,
                gps_rx_producer,
            },
            init::Monotonics(mono),
        )
    }

    #[idle(local = [watchdog, status_led, gps])]
    fn idle(c: idle::Context) -> ! {
        let idle::LocalResources {
            gps,
            watchdog,
            status_led,
        } = c.local;

        // gps.hot_restart().unwrap();

        info!("Ready");
        blink_status_led_for(status_led, 100_000_000);
        cortex_m::asm::delay(50_000_000);

        gps.logger_status().unwrap();
        // gps.read_logs(|count_estimate, i, point| {
        //     // info!("Got point {}, expecting {}", point, count_estimate)
        //     let percent = i as f32 / count_estimate as f32 * 100_f32;
        //     info!("{}% ({}/{})", percent, i, count_estimate);
        // })
        // .unwrap();

        loop {
            cortex_m::asm::wfe();
            watchdog.feed();
            info!("Woke up");

            // TODO: This is where we actually do things

            gps.flush_rx_queue();
            // NOTE: watchdog hasn't actually been tested, because of a cargo-flash
            // bug. As such, I'm unsure if the watchdog ticks while we're asleep
            watchdog.feed();
            blink_status_led(status_led);
            watchdog.feed();
        }
    }

    #[task(binds = UART0_IRQ, local=[gps_uart_reader, gps_rx_producer])]
    fn uart0(c: uart0::Context) {
        const MAX_BYTES_PER_INTERRUPT: usize = 1024;

        let uart0::LocalResources {
            gps_uart_reader: reader,
            gps_rx_producer: producer,
        } = c.local;

        let mut grant = match producer.grant_max_remaining(MAX_BYTES_PER_INTERRUPT) {
            Ok(grant) => grant,
            Err(_) => {
                // This means the queue is totally full. Nothing we can do here.
                // When we catch up later we'll just need to retry.
                Board::unpend(Interrupt::UART0_IRQ);
                return;
            }
        };

        match reader.read_raw(grant.buf()) {
            Ok(count) => {
                // We successfully read `count` bytes
                grant.commit(count)
            }
            Err(nb::Error::WouldBlock) => {
                // Spurious wake, nothing read
                grant.commit(0)
            }
            Err(nb::Error::Other(_)) => {
                // Error reading. Doing anything that takes time (like logging)
                // could compound the issue, so we just ignore it.
                //
                // This will probably cause a corrupted packet, which ada_gps
                // will detect and address at a higher level.
                grant.commit(0)
            }
        }

        Board::unpend(Interrupt::UART0_IRQ);
    }

    fn blink_status_led(led: &mut StatusLed) {
        blink_status_led_for(led, STATUS_BLINK_CYCLES);
    }

    fn blink_status_led_for(led: &mut StatusLed, cycles: u32) {
        led.set_high().unwrap();
        cortex_m::asm::delay(cycles);
        led.set_low().unwrap();
    }
}
