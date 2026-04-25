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

    // Verify CRC. The CRC field sits right after status_flags + seq_num,
    // so its offset in the wire frame is exactly the start of the
    // serialised header through seq_num. With the v2.0 status frame
    // (header 2 + digital_in 2 + digital_out 2 + adc[12] = 24 + flags 1
    // + seq 1 = 32) that's bytes 0..32.
    let crc_offset = std::mem::offset_of!(StatusMessage, crc);
    let calculated_crc = crc::crc16_ccitt_table(&data[..crc_offset]);
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
        // Create a valid status message. The Status frame layout in
        // v2.0 is { header, digital_in, digital_out, adc[12],
        // status_flags, seq_num, crc, ... }, so CRC sits at offset 32
        // and is computed over bytes 0..32.
        let crc_off = std::mem::offset_of!(StatusMessage, crc);
        let mut data = [0u8; 64];
        data[0] = 0xAA; // STATUS_HEADER low
        data[1] = 0x55; // STATUS_HEADER high
        data[2] = 0xFF; // digital_in low
        data[3] = 0x00; // digital_in high

        let crc = crc::crc16_ccitt_table(&data[..crc_off]);
        data[crc_off] = (crc & 0xFF) as u8;
        data[crc_off + 1] = (crc >> 8) as u8;

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
        let crc_off = std::mem::offset_of!(StatusMessage, crc);
        let mut data = [0u8; 64];
        data[0] = 0xAA; // STATUS_HEADER
        data[1] = 0x55;
        data[crc_off] = 0xFF; // Wrong CRC
        data[crc_off + 1] = 0xFF;

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
        // Use offset_of! so the test follows the wire layout, not a
        // hand-counted set of magic numbers — when adc[] grows again
        // the test keeps working.
        let off_status_flags = std::mem::offset_of!(StatusMessage, status_flags);
        let off_seq_num = std::mem::offset_of!(StatusMessage, seq_num);
        let off_crc = std::mem::offset_of!(StatusMessage, crc);
        let off_loop_time = std::mem::offset_of!(StatusMessage, loop_time_us);
        let off_uptime = std::mem::offset_of!(StatusMessage, uptime_ms);
        let off_err = std::mem::offset_of!(StatusMessage, error_count);

        let mut data = [0u8; 64];

        // Header
        data[0] = 0xAA;
        data[1] = 0x55;

        // Digital I/O
        data[2] = 0x12;
        data[3] = 0x34;
        data[4] = 0x56;
        data[5] = 0x78;

        // ADC values (12 channels). adc[] starts at offset 6.
        for i in 0..12 {
            let val: u16 = 100 + (i as u16) * 100;
            data[6 + i * 2] = (val & 0xFF) as u8;
            data[6 + i * 2 + 1] = (val >> 8) as u8;
        }

        data[off_status_flags] = 0b00111111;
        data[off_seq_num] = 99;

        // Calculate and set CRC over bytes [0, off_crc).
        let crc = crc::crc16_ccitt_table(&data[..off_crc]);
        data[off_crc] = (crc & 0xFF) as u8;
        data[off_crc + 1] = (crc >> 8) as u8;

        // Loop time
        data[off_loop_time] = 200u8;
        data[off_loop_time + 1] = 0u8;

        // Uptime
        let uptime: u32 = 123456;
        data[off_uptime] = (uptime & 0xFF) as u8;
        data[off_uptime + 1] = ((uptime >> 8) & 0xFF) as u8;
        data[off_uptime + 2] = ((uptime >> 16) & 0xFF) as u8;
        data[off_uptime + 3] = ((uptime >> 24) & 0xFF) as u8;

        // Error count
        data[off_err] = 5;
        data[off_err + 1] = 0;

        let status = decode_status(&data).unwrap();

        assert_eq!(status.digital_in, 0x3412);
        assert_eq!(status.digital_out, 0x7856);
        assert_eq!(status.adc[0], 100);
        assert_eq!(status.adc[1], 200);
        assert_eq!(status.adc[11], 1200);
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
