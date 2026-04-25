use std::mem;

use super::*;

/// Encode a Command into a CommandMessage (raw bytes)
pub fn encode_command(cmd: &Command) -> [u8; MESSAGE_SIZE] {
    let mut msg = CommandMessage {
        header: COMMAND_HEADER,
        digital_out: cmd.digital_out,
        dac0: cmd.dac[0].min(DAC_MAX),
        dac1: cmd.dac[1].min(DAC_MAX),
        pwm0: cmd.pwm[0],
        pwm1: cmd.pwm[1],
        flags: cmd.flags.to_byte(),
        seq_num: cmd.seq_num,
        crc: 0, // Calculated below
        reserved: [0; 48],
    };

    // Calculate CRC over first 14 bytes (header through seq_num)
    let bytes = unsafe { std::slice::from_raw_parts(&msg as *const _ as *const u8, 14) };
    msg.crc = crc::crc16_ccitt_table(bytes);

    // Convert to byte array
    unsafe { mem::transmute(msg) }
}

/// Decode a StatusMessage from raw bytes
pub fn decode_status(data: &[u8]) -> Result<Status> {
    if data.len() != MESSAGE_SIZE {
        return Err(ProtocolError::InvalidLength { expected: MESSAGE_SIZE, got: data.len() });
    }

    // Parse message
    let msg: StatusMessage = unsafe { std::ptr::read_unaligned(data.as_ptr() as *const _) };

    // Verify header
    if msg.header != STATUS_HEADER {
        return Err(ProtocolError::InvalidHeader { expected: STATUS_HEADER, got: msg.header });
    }

    // Verify CRC (over bytes 0-23)
    let calculated_crc = crc::crc16_ccitt_table(&data[0..24]);
    if msg.crc != calculated_crc {
        return Err(ProtocolError::CrcMismatch { expected: calculated_crc, got: msg.crc });
    }

    // Convert to high-level Status
    Ok(Status {
        digital_in: msg.digital_in,
        digital_out: msg.digital_out,
        adc: msg.adc,
        flags: StatusFlags::from_byte(msg.status_flags),
        seq_num: msg.seq_num,
        loop_time_us: msg.loop_time_us,
        uptime_ms: msg.uptime_ms,
        error_count: msg.error_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_command() {
        let cmd = Command {
            digital_out: 0x0F00,
            dac: [2047, 4095],
            pwm: [32768, 65535],
            flags: CommandFlags {
                adc_enable: true,
                dac_enable: true,
                pwm_enable: true,
                reset_seq: false,
                watchdog_disable: false,
            },
            seq_num: 42,
        };

        let bytes = encode_command(&cmd);

        assert_eq!(bytes.len(), 64);
        assert_eq!(bytes[0], 0x55); // COMMAND_HEADER low byte
        assert_eq!(bytes[1], 0xAA); // COMMAND_HEADER high byte
        assert_eq!(bytes[12], 0x07); // flags
        assert_eq!(bytes[13], 42); // seq_num
    }

    #[test]
    fn test_decode_status() {
        // Create a valid status message
        let mut data = [0u8; 64];
        data[0] = 0xAA; // STATUS_HEADER low
        data[1] = 0x55; // STATUS_HEADER high
        data[2] = 0xFF; // digital_in low
        data[3] = 0x00; // digital_in high

        // Calculate CRC for bytes 0-23
        let crc = crc::crc16_ccitt_table(&data[0..24]);
        data[24] = (crc & 0xFF) as u8;
        data[25] = (crc >> 8) as u8;

        let status = decode_status(&data).unwrap();
        assert_eq!(status.digital_in, 0x00FF);
    }

    #[test]
    fn test_invalid_header() {
        let mut data = [0u8; 64];
        data[0] = 0x00; // Wrong header
        data[1] = 0x00;

        let result = decode_status(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_crc_mismatch() {
        let mut data = [0u8; 64];
        data[0] = 0xAA; // STATUS_HEADER
        data[1] = 0x55;
        data[24] = 0xFF; // Wrong CRC
        data[25] = 0xFF;

        let result = decode_status(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        // Encode a command
        let cmd = Command {
            digital_out: 0xABCD,
            dac: [1234, 2345],
            pwm: [12345, 54321],
            flags: CommandFlags {
                adc_enable: true,
                dac_enable: false,
                pwm_enable: true,
                reset_seq: false,
                watchdog_disable: false,
            },
            seq_num: 123,
        };

        let bytes = encode_command(&cmd);

        // Verify header
        assert_eq!(bytes[0], 0x55);
        assert_eq!(bytes[1], 0xAA);

        // Verify CRC is calculated
        let crc = crc::crc16_ccitt_table(&bytes[0..14]);
        assert_eq!(bytes[14], (crc & 0xFF) as u8);
        assert_eq!(bytes[15], (crc >> 8) as u8);
    }

    #[test]
    fn test_decode_full_status() {
        let mut data = [0u8; 64];

        // Header
        data[0] = 0xAA;
        data[1] = 0x55;

        // Digital I/O
        data[2] = 0x12;
        data[3] = 0x34;
        data[4] = 0x56;
        data[5] = 0x78;

        // ADC values (8 channels)
        for i in 0..8 {
            let val = 100 + i * 100;
            data[6 + i * 2] = (val & 0xFF) as u8;
            data[7 + i * 2] = (val >> 8) as u8;
        }

        // Status flags
        data[22] = 0b00111111;

        // Sequence number
        data[23] = 99;

        // Calculate and set CRC
        let crc = crc::crc16_ccitt_table(&data[0..24]);
        data[24] = (crc & 0xFF) as u8;
        data[25] = (crc >> 8) as u8;

        // Loop time
        data[26] = 200 & 0xFF;
        data[27] = (200 >> 8) as u8;

        // Uptime
        let uptime = 123456u32;
        data[28] = (uptime & 0xFF) as u8;
        data[29] = ((uptime >> 8) & 0xFF) as u8;
        data[30] = ((uptime >> 16) & 0xFF) as u8;
        data[31] = ((uptime >> 24) & 0xFF) as u8;

        // Error count
        data[32] = 5;
        data[33] = 0;

        let status = decode_status(&data).unwrap();

        assert_eq!(status.digital_in, 0x3412);
        assert_eq!(status.digital_out, 0x7856);
        assert_eq!(status.adc[0], 100);
        assert_eq!(status.adc[1], 200);
        assert_eq!(status.seq_num, 99);
        assert_eq!(status.loop_time_us, 200);
        assert_eq!(status.uptime_ms, 123456);
        assert_eq!(status.error_count, 5);
    }

    #[test]
    fn test_invalid_message_length() {
        let data = [0u8; 32]; // Too short
        let result = decode_status(&data);
        assert!(result.is_err());

        let data = [0u8; 128]; // Too long
        let result = decode_status(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_dac_clamping() {
        let cmd = Command {
            digital_out: 0,
            dac: [5000, 6000], // Over 4095 max
            pwm: [0, 0],
            flags: CommandFlags::default(),
            seq_num: 0,
        };

        let bytes = encode_command(&cmd);

        // DAC values should be clamped to 4095
        let dac0 = u16::from_le_bytes([bytes[4], bytes[5]]);
        let dac1 = u16::from_le_bytes([bytes[6], bytes[7]]);

        assert_eq!(dac0, 4095);
        assert_eq!(dac1, 4095);
    }
}
