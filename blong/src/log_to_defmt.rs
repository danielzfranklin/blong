use core::fmt::Write;

use alloc::string::String;
use defmt::Display2Format;
use log::{Level, LevelFilter, Metadata, Record};

static mut LOGGER: LogToDefmt = LogToDefmt {
    rubble_log_level: Level::Error,
};

/// # Safety
/// Must be called zero or one times
pub unsafe fn init() {
    LOGGER.rubble_log_level = get_rubble_log_level();

    if let Ok(()) = log::set_logger(&LOGGER) {
        log::set_max_level(LevelFilter::Debug)
    }
}

struct LogToDefmt {
    rubble_log_level: Level,
}

impl log::Log for LogToDefmt {
    fn enabled(&self, meta: &Metadata) -> bool {
        if meta.target().starts_with("rubble::") {
            meta.level() <= self.rubble_log_level
        } else {
            true
        }
    }

    fn log(&self, record: &Record) {
        let meta = record.metadata();

        if self.enabled(meta) {
            let mut msg = String::new();
            if write!(msg, "{} {}", meta.target(), record.args()).is_ok() {
                let msg = Display2Format(&msg);

                match record.level() {
                    Level::Error => defmt::error!("{}", msg),
                    Level::Warn => defmt::warn!("{}", msg),
                    Level::Info => defmt::info!("{}", msg),
                    Level::Debug => defmt::debug!("{}", msg),
                    Level::Trace => defmt::trace!("{}", msg),
                }
            }
        }
    }

    fn flush(&self) {
        defmt::flush()
    }
}
fn get_rubble_log_level() -> Level {
    match option_env!("RUBBLE_LOG_LEVEL").unwrap_or("info") {
        "trace" => Level::Trace,
        "debug" => Level::Debug,
        "info" => Level::Info,
        "warn" => Level::Warn,
        "error" => Level::Error,
        level => panic!("Unexpected RUBBLE_LOG_LEVEL: {}", level),
    }
}
