use crate::IntegerPercent;
use defmt::Format;

#[derive(Format, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Status {
    pub interval: u32,
    pub is_on: bool,
    pub record_count: u32,
    pub percent_full: IntegerPercent,
}
