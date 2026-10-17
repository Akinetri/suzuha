/*
 * the smallest png that works: rgba rows in uncompressed deflate blocks;
 * tray icons are a few kilobytes, so compressing them isn't worth a crate
 */
pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);

    for row in rgba.chunks(width as usize * 4) {
        // filter type 0, the row as it is
        raw.push(0);
        raw.extend_from_slice(row);
    }

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

    let mut header = Vec::new();

    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());

    // 8 bits per channel, rgba, deflate, no filter choice, no interlace
    header.extend_from_slice(&[8, 6, 0, 0, 0]);

    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib(&raw));
    chunk(&mut png, b"IEND", &[]);

    png
}

fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());

    let start = png.len();

    png.extend_from_slice(kind);
    png.extend_from_slice(data);

    let checksum = crc32(&png[start..]);

    png.extend_from_slice(&checksum.to_be_bytes());
}

// stored blocks of at most 65535 bytes, with zlib's header and adler checksum around them
fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];

    let blocks: Vec<&[u8]> = data.chunks(65_535).collect();

    if blocks.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }

    for (index, block) in blocks.iter().enumerate() {
        let last = u8::from(index == blocks.len() - 1);

        let length = block.len() as u16;

        out.push(last);
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&(!length).to_le_bytes());
        out.extend_from_slice(block);
    }

    out.extend_from_slice(&adler32(data).to_be_bytes());

    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;

    for byte in data {
        crc ^= u32::from(*byte);

        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();

            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }

    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut first, mut second) = (1_u32, 0_u32);

    for byte in data {
        first = (first + u32::from(*byte)) % 65_521;
        second = (second + first) % 65_521;
    }

    (second << 16) | first
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }
}
