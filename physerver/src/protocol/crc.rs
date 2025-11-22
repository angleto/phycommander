/// CRC-16-CCITT calculation
/// Polynomial: 0x1021
/// Initial value: 0xFFFF
/// No final XOR
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;

    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }

    crc
}

/// Faster table-based CRC-16-CCITT calculation
pub fn crc16_ccitt_table(data: &[u8]) -> u16 {
    const CRC_TABLE: [u16; 256] = generate_crc_table();

    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        let index = ((crc >> 8) ^ (byte as u16)) as u8;
        crc = (crc << 8) ^ CRC_TABLE[index as usize];
    }
    crc
}

/// Generate CRC table at compile time
const fn generate_crc_table() -> [u16; 256] {
    let mut table = [0u16; 256];
    let mut i = 0;

    while i < 256 {
        let mut crc = (i as u16) << 8;
        let mut j = 0;

        while j < 8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
            j += 1;
        }

        table[i] = crc;
        i += 1;
    }

    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc_empty() {
        let data = [];
        assert_eq!(crc16_ccitt(&data), 0xFFFF);
    }

    #[test]
    fn test_crc_simple() {
        let data = b"123456789";
        let crc = crc16_ccitt(data);
        assert_eq!(crc, 0x29B1); // Known value for "123456789"
    }

    #[test]
    fn test_crc_table_matches() {
        let data = b"Hello, World!";
        assert_eq!(crc16_ccitt(data), crc16_ccitt_table(data));
    }
}
