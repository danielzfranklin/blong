use defmt::{Display2Format, Format};

use crate::error;

// Note we only handle "Basic" mode, i.e. table row A on page 11 of
// GTop_LOCUS_Library_User_Manual-v13.pdf.
//
// See also Locus_Sample_Code/LocusParser.cpp,
// <https://github.com/don/locus/blob/master/locus.py>, and
// <https://github.com/land-boards/lb-Arduino-Code/blob/master/Host%20code/parseLOCUS/parseLOCUS.cpp>

#[derive(Format, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LoggedPoint {
    pub temp_checksum: u8,
}

// TODO: All this header and sector shit I see in the sample cpp
// Lets stream the whole complete bytes to our laptop over uart, then we can compile the
// sample code and see if it works.

pub(crate) fn parse_data_fields<Fields, Field, OnPoint>(
    fields: Fields,
    mut on_point: OnPoint,
) -> Result<(), Error>
where
    OnPoint: FnMut(LoggedPoint),
    Fields: AsRef<[Field]>,
    Field: AsRef<[u8]>,
{
    let fields = fields.as_ref();

    if fields.len() % 4 != 0 {
        error!(
            "Chunk count not divisible by 4, got {} chunks",
            fields.len()
        );
        return Err(Error::InvalidFieldCount);
    }

    let mut data = [0u8; 16];
    for data_chunks in fields.chunks_exact(4) {
        // Decode the chunks into `data`
        for n in 0..4 {
            let chunk = data_chunks[n].as_ref();

            if chunk.len() != 8 {
                error!("Invalid chunk length: {}", chunk.len(),);
                return Err(Error::InvalidFieldLength);
            }

            let out_start = n * 4;
            hex::decode_to_slice(chunk, &mut data[out_start..out_start + 4]).map_err(|err| {
                error!(
                    "Failed to decode chunk {=[u8]:a} as hex: {}",
                    chunk,
                    Display2Format(&err)
                );
                Error::HexDecode
            })?;
        }

        let point = parse_point(&data)?;
        on_point(point);
    }

    Ok(())
}

pub fn parse_point(bytes: &[u8; 16]) -> Result<LoggedPoint, Error> {
    let timestamp = &bytes[0..4];
    let fix = &bytes[4];
    let latitude = &bytes[5..9];
    let longitude = &bytes[9..13];
    let height = &bytes[13..15];

    let mut checksum = 0;
    for byte in bytes {
        checksum ^= byte;
    }
    // if checksum != 0 {
    //     error!("Wrong checksum for bytes {=[u8; 16]:a}", bytes);
    //     return Err(Error::WrongChecksum);
    // }

    Ok(LoggedPoint {
        temp_checksum: checksum,
    }) // TODO
}

#[derive(Format, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Error {
    /// The fields 8-bytes of ascii hex data. We only handle basic mode, where
    /// each point is 16 bytes. As such, the number of chunks must be divisible
    /// by 4.
    InvalidFieldCount,
    /// A chunk must be 8 bytes of ascii hex
    InvalidFieldLength,
    WrongChecksum,
    HexDecode,
}
