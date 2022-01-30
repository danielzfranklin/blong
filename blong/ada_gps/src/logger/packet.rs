use crate::UtcDateTime;
use defmt::Format;

#[derive(Format, Debug, Clone, Hash, Eq, PartialEq)]
pub enum Fix {
    /// Fix not available.
    ///
    /// Corresponds to NMEA GGA quality 0.
    No,
    /// Normal GPS fix
    ///
    /// Corresponds to NMEA GGA quality 1.
    GpsFix,
    /// Differential GPS fix (enhanced quality).
    ///
    /// Corresponds to NMEA GGA quality 2.
    DGpsFix,
    /// Dead reckoning.
    ///
    /// Corresponds to NMEA GGA quality 6.
    DeadReckoning,
}

#[derive(Clone, PartialEq, Format, Debug)]
pub struct Packet {
    pub time: Option<UtcDateTime>,
    pub fix: Option<Fix>,
    pub lat: Option<f32>,
    pub lon: Option<f32>,
    pub height: Option<i16>,
    pub speed: Option<i16>,
    /// In degrees
    pub heading: Option<u16>,
    pub hdop: Option<u16>,
    pub num_sat: Option<u8>,
}

impl Default for Packet {
    fn default() -> Self {
        Self {
            time: None,
            fix: None,
            lat: None,
            lon: None,
            height: None,
            speed: None,
            heading: None,
            hdop: None,
            num_sat: None,
        }
    }
}
