use alloc::vec::Vec;
use defmt::{Debug2Format, Format};

use super::Checksum;
use crate::{debug, IntegerPercent};

/// Returns a tuple of (name, fields)
pub(crate) fn parse(cmd: &[u8]) -> Result<(Vec<u8>, Vec<Vec<u8>>), Error> {
    let mut raw = cmd.iter().peekable();
    let mut name = Vec::new();
    let mut fields = Vec::new();

    // Prefix
    if raw.next().ok_or(Error::ExpectedPrefix)? != &b'$' {
        debug!("expected prefix, got different character");
        return Err(Error::ExpectedPrefix);
    }

    // Name
    loop {
        match raw.peek() {
            Some(b',' | b'*') => {
                if name.len() == 0 {
                    debug!("got name of length zero");
                    return Err(Error::ExpectedName);
                } else {
                    break;
                }
            }
            _ => (),
        }
        let char = *raw.next().ok_or(Error::ExpectedName)?;
        name.push(char);
    }

    // Fields
    while raw.peek() != Some(&&b'*') {
        let char = raw.next().ok_or(Error::ExpectedField)?;
        if char == &b',' {
            fields.push(Vec::new());
        } else {
            let field = fields.last_mut().unwrap();
            field.push(*char);
        }
    }

    // Checksum
    let _ = raw.next(); // We already checked this is b'*'
    let checksum = [
        *raw.next().ok_or(Error::ExpectedChecksum)?,
        *raw.next().ok_or(Error::ExpectedChecksum)?,
    ];
    let checksum = Checksum::parse(&checksum).map_err(|_| Error::ChecksumParse)?;

    // Suffix
    if raw.next().ok_or(Error::ExpectedSuffix)? != &b'\r' {
        debug!("expected carriage return, got different character");
        return Err(Error::ExpectedSuffix);
    }
    if raw.next().ok_or(Error::ExpectedSuffix)? != &b'\n' {
        debug!("expected newline, got different character");
        return Err(Error::ExpectedSuffix);
    }

    // End
    if raw.next().is_some() {
        debug!("expected end");
        return Err(Error::ExpectedEnd);
    }

    // Check checksum
    let line = &cmd[1..cmd.len() - 5]; // between $ and *
    if checksum != Checksum::compute_for(&line) {
        debug!("wrong checksum");
        return Err(Error::WrongChecksum);
    }

    Ok((name, fields))
}

pub(crate) fn integer_field(val: &[u8]) -> Result<u32, Error> {
    lexical_core::parse(val).map_err(|err| {
        debug!(
            "Failed to parse field {=[u8]:a} as u32: {:?}",
            val,
            Debug2Format(&err),
        );
        Error::ParseField
    })
}

pub(crate) fn integer_percent_field(val: &[u8]) -> Result<IntegerPercent, Error> {
    let val = lexical_core::parse::<u8>(val).map_err(|err| {
        debug!(
            "Failed to parse field {=[u8]:a} as u8 (expecting integer percent): {:?}",
            val,
            Debug2Format(&err),
        );
        Error::ParseField
    })?;

    if val > 100 {
        debug!("Expected integer percent, but got val > 100: {}", val);
        return Err(Error::ParseField);
    }

    Ok(IntegerPercent::new(val))
}

pub(crate) fn bool_field(val: &[u8], truthy: &[u8], falsy: &[u8]) -> Result<bool, Error> {
    if val == truthy {
        Ok(true)
    } else if val == falsy {
        Ok(false)
    } else {
        debug!(
            "Failed to parse bool, expected {=[u8]:a} for truthy or {=[u8]:a} for falsy, got {=[u8]:a}",
            truthy, falsy, val
        );
        Err(Error::ParseField)
    }
}

#[derive(Format, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Error {
    ExpectedPrefix,
    ExpectedName,
    ExpectedField,
    ExpectedChecksum,
    ChecksumParse,
    ExpectedSuffix,
    ExpectedEnd,
    WrongChecksum,
    ParseField,
}

#[cfg(all(test, feature = "host-test"))]
mod tests {
    use super::*;

    #[test]
    fn test_valid() {
        let (actual_name, actual_fields) =
            parse_cmd(b"$PMTK314,1,10,1,1,1,5,0,0,0,0,0,0,0,0,0,0,0,0,0*1C\r\n").unwrap();

        let expected_name = b"PMTK314";
        assert_eq!(actual_name, expected_name);

        let expected_fields: Vec<&[u8]> = vec![
            b"1", b"10", b"1", b"1", b"1", b"5", b"0", b"0", b"0", b"0", b"0", b"0", b"0", b"0",
            b"0", b"0", b"0", b"0", b"0",
        ];
        assert_eq!(actual_fields, expected_fields);

        // Test parsing no fields
        let (actual_name, actual_fields) = parse_cmd(b"$PMTK183*38\r\n").unwrap();

        let expected_name = b"PMTK183";
        assert_eq!(actual_name, expected_name);

        let expected_fields: Vec<Vec<u8>> = vec![];
        assert_eq!(actual_fields, expected_fields);
    }

    #[test]
    fn test_invalid() {
        assert_eq!(parse_cmd(b""), Err(Error::ExpectedPrefix));
        assert_eq!(parse_cmd(b"foo"), Err(Error::ExpectedPrefix));

        assert_eq!(parse_cmd(b"$"), Err(Error::ExpectedName));
        assert_eq!(parse_cmd(b"$*"), Err(Error::ExpectedName));
        assert_eq!(parse_cmd(b"$NAME"), Err(Error::ExpectedName));

        assert_eq!(parse_cmd(b"$NAME,"), Err(Error::ExpectedField));
        assert_eq!(parse_cmd(b"$NAME,\r\n"), Err(Error::ExpectedField));

        assert_eq!(parse_cmd(b"$NAME,*"), Err(Error::ExpectedChecksum));
        assert_eq!(parse_cmd(b"$NAME,*0"), Err(Error::ExpectedChecksum));

        assert_eq!(parse_cmd(b"$NAME,*zz"), Err(Error::ChecksumParse));

        assert_eq!(parse_cmd(b"$NAME,*0f"), Err(Error::ExpectedSuffix));
        assert_eq!(parse_cmd(b"$NAME,*0f\r"), Err(Error::ExpectedSuffix));

        assert_eq!(parse_cmd(b"$NAME,*0f\r\n"), Err(Error::WrongChecksum));
    }
}
