use defmt::Format;

#[derive(Format, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntegerPercent(u8);

impl IntegerPercent {
    /// Panics if `val > 100`
    pub fn new(val: u8) -> Self {
        assert!(val <= 100);
        Self(val)
    }

    pub fn zero() -> Self {
        Self(0)
    }

    /// Convert to an integer value (42% would be `42`).
    pub fn as_u8(self) -> u8 {
        self.0
    }
}

impl PartialEq<u8> for IntegerPercent {
    fn eq(&self, other: &u8) -> bool {
        self.0.eq(other)
    }
}
