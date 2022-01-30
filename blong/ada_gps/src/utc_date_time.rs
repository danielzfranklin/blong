#[derive(Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct UtcDateTime(time::OffsetDateTime);

impl UtcDateTime {
    pub fn from_unix(timestamp: i64) -> Option<Self> {
        time::OffsetDateTime::from_unix_timestamp(timestamp)
            .map(Self)
            .ok()
    }
}

impl defmt::Format for UtcDateTime {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "{=i32:04}-{=u8:02}-{=u8:02} {=u8:02}:{=u8:02}:{=u8:02}.{=u32} UTC",
            self.0.year(),
            self.0.month() as u8,
            self.0.day(),
            self.0.hour(),
            self.0.minute(),
            self.0.second(),
            self.0.microsecond()
        )
    }
}

impl core::fmt::Debug for UtcDateTime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self, f)
    }
}

impl core::fmt::Display for UtcDateTime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{} UTC",
            self.0.year(),
            self.0.month() as u8,
            self.0.day(),
            self.0.hour(),
            self.0.minute(),
            self.0.second(),
            self.0.microsecond(),
        )
    }
}
