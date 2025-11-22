use super::*;
use std::mem;

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
    let bytes = unsafe {
        std::slice::from_raw_parts(
            &msg as *const _ as *const u8,
            14,
        )
    };
    msg.crc = crc::crc16_ccitt_table(bytes);

    // Convert to byte array
    unsafe { mem::transmute(msg) }
}

/// Decode a StatusMessage from raw bytes
pub fn decode_status(data: &[u8]) -> Result<Status> {
    if data.len() != MESSAGE_SIZE {
        return Err(ProtocolError::InvalidLength {
            expected: MESSAGE_SIZE,
            got: data.len(),
        });
    }

    // Parse message
    let msg: StatusMessage = unsafe {
        std::ptr::read_unaligned(data.as_ptr() as *const _)
    };

    // Verify header
    if msg.header != STATUS_HEADER {
        return Err(ProtocolError::InvalidHeader {
            expected: STATUS_HEADER,
            got: msg.header,
        });
    }

    // Verify CRC (over bytes 0-23)
    let calculated_crc = crc::crc16_ccitt_table(&data[0..24]);
    if msg.crc != calculated_crc {
        return Err(ProtocolError::CrcMismatch {
            expected: calculated_crc,
            got: msg.crc,
        });
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
        assert_eq!(bytes[13], 42);   // seq_num
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
}
