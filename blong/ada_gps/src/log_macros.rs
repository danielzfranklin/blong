#[macro_export]
macro_rules! debug {
    ($($arg:expr),+ $(,)?) => {
        #[cfg(not(target_os = "none"))]
        drop(($($arg),+));

        #[cfg(target_os = "none")]
        defmt::debug!($($arg),+);
    }
}

#[macro_export]
macro_rules! error {
    ($($arg:expr),+ $(,)?) => {
        #[cfg(not(target_os = "none"))]
        drop(($($arg),+));

        #[cfg(target_os = "none")]
        defmt::error!($($arg),+);
    }
}

#[macro_export]
macro_rules! info {
    ($($arg:expr),+ $(,)?) => {
        #[cfg(not(target_os = "none"))]
        drop(($($arg),+));

        #[cfg(target_os = "none")]
        defmt::info!($($arg),+);
    }
}

#[macro_export]
macro_rules! trace {
    ($($arg:expr),+ $(,)?) => {
        #[cfg(not(target_os = "none"))]
        drop(($($arg),+));

        #[cfg(target_os = "none")]
        defmt::trace!($($arg),+);
    }
}

#[macro_export]
macro_rules! warn {
    ($($arg:expr),+ $(,)?) => {
        #[cfg(not(target_os = "none"))]
        drop(($($arg),+));

        #[cfg(target_os = "none")]
        defmt::warn!($($arg),+);
    }
}
