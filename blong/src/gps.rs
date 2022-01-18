use crate::prelude::*;
use ascii::AsciiStr;
use core::fmt::Write;
use defmt::Debug2Format;
use nmea_parser::NmeaParser;

// NOTE: See PMTK_A11-datasheet.pdf

pub struct Gps {
    // Maximum packet length is 255 bytes
    unparsed: heapless::Vec<u8, 255>,
    parser: NmeaParser,
}

impl Gps {
    pub fn new() -> Self {
        Self {
            unparsed: Default::default(),
            parser: Default::default(),
        }
    }

    pub fn write_on_cmd(writer: impl Write) {
        // Turns on gga and rmc
        //   GGA: GPS fix data
        //   RMC: Recommended Minimum sentence C
        let on_cmd = AsciiStr::from_ascii("PMTK314,0,1,0,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0").unwrap();
        Self::write_command(writer, on_cmd)
    }

    /// Accept a byte of input, and do any processing that triggers.
    ///
    /// This function is potentially expensive. It may allocate buffers and
    /// write to storage.
    pub fn accept_byte(&mut self, byte: u8) {
        if self.unparsed.push(byte).is_err() {
            error!("Maximum packet size exceeded, clearing unparsed and retrying");
            self.unparsed.clear();
            return;
        }

        let len = self.unparsed.len();
        if byte == b'\n' && len >= 2 && self.unparsed[len - 2] == b'\r' {
            let sentence = match AsciiStr::from_ascii(&self.unparsed) {
                Ok(sentence) => sentence.as_str(),
                Err(_) => {
                    error!("Sentence not ascii, clearing unparsed and retrying");
                    self.unparsed.clear();
                    return;
                }
            };

            let sentence = self.parser.parse_sentence(sentence);
            debug!("Got: {}", Debug2Format(&sentence));

            self.unparsed.clear();
        }
    }

    fn write_command(mut writer: impl Write, cmd: &AsciiStr) {
        let mut checksum = 0;
        for char in cmd.as_bytes() {
            checksum ^= char;
        }
        write!(writer, "${}*{:02X}\r\n", cmd, checksum).unwrap();
    }
}

impl Default for Gps {
    fn default() -> Self {
        Self::new()
    }
}
