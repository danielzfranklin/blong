use super::Checksum;
use alloc::vec::Vec;

pub(crate) fn serialize<'i, 'o>(name: &'i [u8], fields: &'i [&'i [u8]], out: &'o mut Vec<u8>) {
    out.push(b'$');

    // Name
    for &byte in name {
        out.push(byte);
    }

    // Fields
    for &field in fields {
        out.push(b',');
        for &byte in field {
            out.push(byte);
        }
    }

    out.push(b'*');

    // Checksum
    let line = &out[1..out.len() - 1]; // between $ and *
    let checksum = Checksum::compute_for(line);
    for byte in checksum.to_ascii() {
        out.push(byte);
    }

    out.extend_from_slice(b"\r\n");
}
