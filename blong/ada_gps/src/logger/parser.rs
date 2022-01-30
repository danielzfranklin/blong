use core::ops::BitXor;

use bitflags::bitflags;
use defmt::Format;

use super::{Fix, Packet};
use crate::{warn, UtcDateTime};

// TODO NOTE: We're just guessing this is little-endian, as that's more common
// half the checksums pass either way

const MAX_HEADER2_BIT_NUM: u32 = 7;
const HEADER_SIZE: usize = 64;
const HEADER1_SIZE: usize = 16;
const HEADER1_CS_BUF_SIZE: usize = 14;
const HEADER2_SIZE: usize = 44;
const DATA_SIZE: usize = 4032;
const DATA_CHECKSUM_SIZE: usize = 1;
const SECTOR_SIZE: usize = 4096;

#[derive(Format, Debug)]
pub(crate) struct Parser<F> {
    on_packet: F,
    active_sector: Option<SectorHeader>,
    pub(crate) stats: Stats,
}

#[derive(Format, Debug, Clone)]
pub(crate) struct Stats {
    sector_count: usize,
    invalid_sectors: usize,
    empty_sectors: usize,
    invalid_packets: usize,
    packets_parsed: usize,
    invalid_fields: usize,
}

impl<F> Parser<F>
where
    F: FnMut(Packet),
{
    pub(crate) fn new(on_packet: F) -> Self {
        Self {
            on_packet,
            active_sector: None,
            stats: Stats {
                sector_count: 0,
                empty_sectors: 0,
                invalid_sectors: 0,
                invalid_packets: 0,
                packets_parsed: 0,
                invalid_fields: 0,
            },
        }
    }

    fn on_packet(&mut self, packet: Packet) {
        (self.on_packet)(packet)
    }

    // TODO: Make this streaming
    pub(crate) fn parse(&mut self, data: &[u8]) {
        let mut temp_packet_count = 0;

        let sector_count = data.len() / SECTOR_SIZE;
        self.stats.sector_count = sector_count;
        for sector_i in 0..sector_count {
            let data_i = sector_i * SECTOR_SIZE;
            let sector = &data[data_i..data_i + SECTOR_SIZE];
            self.parse_sector(sector);
        }
    }

    fn parse_sector(&mut self, sector: &[u8]) {
        let header = &sector[..HEADER_SIZE];
        let header = match SectorHeader::parse(header) {
            Some(header) => header,
            None => {
                self.stats.invalid_sectors += 1;
                return;
            }
        };

        if header.packet_count == 0 {
            self.stats.empty_sectors += 1;
            return;
        }

        // This includes the checksum
        let packet_size = header.packet_size as usize;

        self.active_sector = Some(header);

        for packet_i in 0..header.packet_count as usize {
            let offset = packet_i * packet_size;
            let start = HEADER_SIZE + offset;
            let end = start + packet_size;
            let packet = &sector[start..end];
            self.parse_packet(packet);
        }
        self.active_sector = None;
    }

    fn parse_packet(&mut self, data: &[u8]) {
        let header = self.active_sector.expect("not in sector");
        let content_flags = header.content_flags;

        let checksum = data[data.len() - 1];
        let data = &data[..data.len() - 1];

        if u8_checksum_for(data) != checksum {
            self.stats.invalid_packets += 1;
            return;
        }

        let mut addr = 0;
        let mut packet = Packet::default();

        if content_flags.contains(ContentFlags::UTC) {
            let time = read_u32_at(data, addr) as i64;
            if let Some(time) = UtcDateTime::from_unix(time) {
                packet.time = Some(time);
            } else {
                self.stats.invalid_fields += 1;
            }
            addr += 4;
        }

        if content_flags.contains(ContentFlags::VALID) {
            let value = data[addr];
            if value & 0x04 == 0x04 {
                packet.fix = Some(Fix::DGpsFix)
            } else if value & 0x02 == 0x02 {
                packet.fix = Some(Fix::GpsFix)
            } else if value & 0x40 == 0x40 {
                packet.fix = Some(Fix::DeadReckoning)
            } else if value == 0x00 {
                packet.fix = Some(Fix::No)
            } else {
                self.stats.invalid_fields += 1;
            };
            addr += 1;
        }

        if content_flags.contains(ContentFlags::LAT) {
            let lat = read_f32_at(data, addr);
            if lat <= 90_f32 && lat >= -90_f32 {
                packet.lat = Some(lat);
            } else {
                self.stats.invalid_fields += 1;
            }
            addr += 4;
        }

        if content_flags.contains(ContentFlags::LON) {
            let lon = read_f32_at(data, addr);
            if lon <= 180_f32 && lon >= -180_f32 {
                packet.lon = Some(lon);
            } else {
                self.stats.invalid_fields += 1;
            }
            addr += 4;
        }

        if content_flags.contains(ContentFlags::HEIGHT) {
            packet.height = Some(read_i16_at(data, addr));
            addr += 2;
        }

        if content_flags.contains(ContentFlags::SPEED) {
            packet.speed = Some(read_i16_at(data, addr));
            addr += 2;
        }

        if content_flags.contains(ContentFlags::TRK) {
            packet.heading = Some(read_u16_at(data, addr));
            addr += 2;
        }

        if content_flags.contains(ContentFlags::HDOP) {
            packet.hdop = Some(read_u16_at(data, addr));
            addr += 2;
        }

        if content_flags.contains(ContentFlags::NUM_SAT) {
            packet.num_sat = Some(data[addr]);
            addr += 1;
        }

        self.stats.packets_parsed += 1;
        self.on_packet(packet);
    }
}

#[derive(Debug, Format, Copy, Clone)]
struct SectorHeader {
    content_flags: ContentFlags,
    packet_size: u32,
    packet_count: u32,
}

bitflags! {
    #[derive(Format)]
    struct ContentFlags: u32 {
        const UTC = 1<<0;
        const VALID = 1<<1;
        const LAT = 1<<2;
        const LON = 1<<3;
        const HEIGHT = 1<<4;
        const SPEED = 1<<5;
        const TRK = 1<<6; // Heading
        const HDOP = 1<<10;
        const NUM_SAT = 1<<12;
    }
}

impl SectorHeader {
    fn parse(header: &[u8]) -> Option<Self> {
        let expected_checksum = read_u16_at(header, HEADER1_CS_BUF_SIZE);
        let checksum = u16_checksum_for(&header[..HEADER1_CS_BUF_SIZE]);
        if checksum != expected_checksum {
            return None;
        }

        // `content is `u4Content` in reference.
        // The reference also parses out a u16 called `u2Serial`, but never
        // uses it.
        let content_flags = read_u32_at(header, 4);
        let content_flags = ContentFlags::from_bits_truncate(content_flags);
        let packet_size = packet_size(content_flags);

        let packet_count = packet_count(header);

        Some(Self {
            content_flags,
            packet_size,
            packet_count,
        })
    }
}

// Locus_Find_BitMap in reference
fn packet_count(header: &[u8]) -> u32 {
    let mut data = 0;

    let mut i: i32 = (HEADER2_SIZE as i32) - 1;
    while i >= 0 {
        let addr = HEADER1_SIZE as i32 + i;
        data = header[addr as usize];
        if data != 0xFF {
            break;
        }
        i -= 1;
    }
    if i < 0 {
        return 0;
    }
    let i = i as u32;

    let mut j = 0;
    while j <= MAX_HEADER2_BIT_NUM {
        if (data >> j) == 0 {
            break;
        }
        j += 1;
    }

    let mut num_byte: u32 = i;
    let mut num_bit: u32 = 0;
    if j == 0 {
        num_byte += 1;
        num_bit = 0;
    } else {
        num_bit = MAX_HEADER2_BIT_NUM + 1 - j;
    }

    num_byte * 8 + num_bit
}

// uCalculateSize in reference
fn packet_size(content: ContentFlags) -> u32 {
    let mut size = 0;

    if content.contains(ContentFlags::UTC) {
        size += 4;
    }
    if content.contains(ContentFlags::VALID) {
        size += 1;
    }
    if content.contains(ContentFlags::LAT) {
        size += 4;
    }
    if content.contains(ContentFlags::LON) {
        size += 4;
    }
    if content.contains(ContentFlags::HEIGHT) {
        size += 2;
    }
    if content.contains(ContentFlags::TRK) {
        size += 2;
    }
    if content.contains(ContentFlags::SPEED) {
        size += 2;
    }
    if content.contains(ContentFlags::HDOP) {
        size += 2;
    }
    if content.contains(ContentFlags::NUM_SAT) {
        size += 1;
    }

    size += 1;

    size
}

/// `u1Locus_Gen_Checksum` in reference.
fn u8_checksum_for(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0_u8, BitXor::bitxor)
}

/// Interprets the byte slice as a slice of `u16`s, and computes their checksum.
/// `u2Locus_Gen_Checksum` in reference.
fn u16_checksum_for(bytes: &[u8]) -> u16 {
    assert!(bytes.len() % 2 == 0);

    bytes
        .chunks_exact(2)
        .map(pair_as_u16)
        .fold(0_u16, BitXor::bitxor)
}

fn pair_as_u16(pair: &[u8]) -> u16 {
    assert!(pair.len() == 2);
    read_u16_at(pair, 0)
}

fn read_u32_at(buf: &[u8], start: usize) -> u32 {
    u32::from_le_bytes([buf[start], buf[start + 1], buf[start + 2], buf[start + 3]])
}

fn read_f32_at(buf: &[u8], start: usize) -> f32 {
    f32::from_le_bytes([buf[start], buf[start + 1], buf[start + 2], buf[start + 3]])
}

fn read_u16_at(buf: &[u8], start: usize) -> u16 {
    u16::from_le_bytes([buf[start], buf[start + 1]])
}

fn read_i16_at(buf: &[u8], start: usize) -> i16 {
    i16::from_le_bytes([buf[start], buf[start + 1]])
}

#[cfg(all(test, feature = "host-test"))]
mod tests {
    use super::*;
    use insta::*;

    #[test]
    fn parses_large_sample_dump() {
        let sample = include_bytes!("../../test_assets/3819_log_records.bin");

        let mut packets = Vec::new();
        let mut parser = Parser::new(|packet| {
            packets.push(packet);
        });
        parser.parse(sample);

        assert_debug_snapshot!(parser.stats);
        assert_debug_snapshot!(packets);
    }
}
