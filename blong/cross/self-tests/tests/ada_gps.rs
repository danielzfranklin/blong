#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

#[defmt_test::tests]
mod tests {
    use ada_gps::{IntegerPercent, LoggerStatus};
    use board::Board;

    #[init]
    fn init() -> Board {
        let device = rp_pico::pac::Peripherals::take().unwrap();
        let core = cortex_m::Peripherals::take().unwrap();
        Board::init(core, device)
    }

    #[test]
    fn test_logs(board: &mut Board) {
        let gps = &mut board.gps;
        gps.stop_logging().unwrap();
        gps.erase_logs().unwrap();

        gps.configure_logger_interval(60 * 30).unwrap();
        assert_eq!(
            gps.logger_status().unwrap(),
            LoggerStatus {
                interval: 60 * 30,
                is_on: false,
                record_count: 0,
                percent_full: IntegerPercent::zero(),
            }
        );

        gps.configure_logger_interval(1).unwrap();
        assert_eq!(
            gps.logger_status().unwrap(),
            LoggerStatus {
                interval: 1,
                is_on: false,
                record_count: 0,
                percent_full: IntegerPercent::zero(),
            }
        );

        gps.start_logging().unwrap();
        assert_eq!(
            gps.logger_status().unwrap(),
            LoggerStatus {
                interval: 1,
                is_on: true,
                record_count: 0,
                percent_full: IntegerPercent::zero(),
            }
        );

        board.delay.delay_ms(2_100); // 2.1 secs

        let status_after_delay = gps.logger_status().unwrap();
        assert_eq!(status_after_delay.interval, 1);
        assert!(status_after_delay.is_on);
        assert_eq!(
            status_after_delay.record_count,
            2,
            "If this fails you probably didn't have a fix. This test presums a fix before you run it."
        );
        assert_eq!(status_after_delay.percent_full, 0);

        gps.read_logs().unwrap();
        // TODO: Clear
        // TODO: Check on, storage empty
        // TODO: Turn off
        // TODO: Check off
    }

    // NOTE: This test is commented out because it causes us to lose our fix,
    //   which breaks other tests.
    // #[test]
    // fn test_restarts(board: &mut Board) {
    //     let gps = &mut board.gps;

    //     for _ in 0..10 {
    //         gps.factory_reset().unwrap();
    //     }

    //     for _ in 0..10 {
    //         gps.cold_restart().unwrap();
    //     }

    //     for _ in 0..10 {
    //         gps.warm_restart().unwrap();
    //     }

    //     for _ in 0..10 {
    //         gps.hot_restart().unwrap();
    //     }
    // }

    // NOTE: This test is commented out as it produces a lot of logs
    // #[test]
    // fn test_sending_pmtk_commands_race_condition(board: &mut Board) {
    //     // This tries to check we properly retry on the random errors we should
    //     //   expect.
    //     for boot in 0..40 {
    //         if let Err(err) = board.gps.hot_restart() {
    //             panic!("Failed to factory reset on boot {} with {:?}", boot, err);
    //         }

    //         for rep in 0..60 {
    //             if let Err(err) = board.gps.configure_logger_interval(10) {
    //                 panic!("Failed boot {} rep {} with {:?}", boot, rep, err)
    //             }
    //         }
    //     }
    // }
}
