use physerver::protocol::{crc::crc16_ccitt_table, decode_status, encode_command};
/// Integration tests for physerver
///
/// These tests verify the complete system integration
use physerver::{Command, CommandFlags, StatusFlags};

#[test]
fn test_protocol_integration() {
    // Create a command
    let cmd = Command {
        digital_out: 0xFFFF,
        dac: [2047, 4095],
        pwm: [32768, 0],
        flags: CommandFlags {
            adc_enable: true,
            dac_enable: true,
            pwm_enable: false,
            reset_seq: false,
            watchdog_disable: false,
        },
        seq_num: 42,
    };

    // Encode it
    let bytes = encode_command(&cmd);

    // Verify basic structure
    assert_eq!(bytes.len(), 64);
    assert_eq!(bytes[0], 0x55); // Command header low
    assert_eq!(bytes[1], 0xAA); // Command header high
}

#[test]
fn test_status_decode_integration() {
    // Build a mock status frame using offset_of! so the test follows
    // the wire layout instead of hand-counted offsets — adc[] grew
    // from 8 to 12 in v2.0 and earlier hard-coded offsets are stale.
    use physerver::protocol::types::StatusMessage;
    let off_status_flags = std::mem::offset_of!(StatusMessage, status_flags);
    let off_seq_num = std::mem::offset_of!(StatusMessage, seq_num);
    let off_crc = std::mem::offset_of!(StatusMessage, crc);
    let off_loop_time = std::mem::offset_of!(StatusMessage, loop_time_us);
    let off_uptime = std::mem::offset_of!(StatusMessage, uptime_ms);

    let mut data = [0u8; 64];

    // Header
    data[0] = 0xAA;
    data[1] = 0x55;

    // Digital I/O
    data[2] = 0xFF;
    data[3] = 0x0F;
    data[4] = 0x00;
    data[5] = 0xF0;

    // ADC values (12 channels). adc[] starts at offset 6.
    for i in 0..12u16 {
        let val = (i + 1) * 500;
        data[6 + (i as usize) * 2] = (val & 0xFF) as u8;
        data[6 + (i as usize) * 2 + 1] = (val >> 8) as u8;
    }

    // Flags
    data[off_status_flags] = 0x07; // ADC, DAC, PWM active

    // Seq num
    data[off_seq_num] = 42;

    // Calculate CRC over [0, off_crc)
    let crc = crc16_ccitt_table(&data[..off_crc]);
    data[off_crc] = (crc & 0xFF) as u8;
    data[off_crc + 1] = (crc >> 8) as u8;

    // Loop time
    data[off_loop_time] = 150;
    data[off_loop_time + 1] = 0;

    // Uptime: 100000000 ms (= 0x05F5E100)
    data[off_uptime] = 0x00;
    data[off_uptime + 1] = 0xE1;
    data[off_uptime + 2] = 0xF5;
    data[off_uptime + 3] = 0x05;

    // Decode
    let status = decode_status(&data).expect("Failed to decode status");

    assert_eq!(status.digital_in, 0x0FFF);
    assert_eq!(status.digital_out, 0xF000);
    assert_eq!(status.adc[0], 500);
    assert_eq!(status.adc[7], 4000);
    assert_eq!(status.adc[11], 6000);
    assert_eq!(status.seq_num, 42);
    assert_eq!(status.loop_time_us, 150);
}

#[test]
fn test_command_flags_integration() {
    let flags = CommandFlags {
        adc_enable: true,
        dac_enable: true,
        pwm_enable: true,
        reset_seq: false,
        watchdog_disable: true,
    };

    let byte = flags.to_byte();
    let recovered = CommandFlags::from_byte(byte);

    assert_eq!(flags.adc_enable, recovered.adc_enable);
    assert_eq!(flags.dac_enable, recovered.dac_enable);
    assert_eq!(flags.pwm_enable, recovered.pwm_enable);
    assert_eq!(flags.reset_seq, recovered.reset_seq);
    assert_eq!(flags.watchdog_disable, recovered.watchdog_disable);
}

#[test]
fn test_status_flags_integration() {
    let flags = StatusFlags {
        adc_active: true,
        dac_active: false,
        pwm_active: true,
        error: false,
        watchdog_triggered: false,
        usb_configured: true,
        overrun: false,
    };

    let byte = flags.to_byte();
    let recovered = StatusFlags::from_byte(byte);

    assert_eq!(flags.adc_active, recovered.adc_active);
    assert_eq!(flags.dac_active, recovered.dac_active);
    assert_eq!(flags.pwm_active, recovered.pwm_active);
    assert_eq!(flags.error, recovered.error);
    assert_eq!(flags.watchdog_triggered, recovered.watchdog_triggered);
    assert_eq!(flags.usb_configured, recovered.usb_configured);
    assert_eq!(flags.overrun, recovered.overrun);
}

#[test]
fn test_multiple_command_encode() {
    // Test that we can encode multiple commands without issues
    for seq in 0..255 {
        let cmd = Command {
            digital_out: seq as u16,
            dac: [seq as u16 * 10, 4095 - seq as u16 * 10],
            pwm: [0, 0],
            flags: CommandFlags::default(),
            seq_num: seq,
        };

        let bytes = encode_command(&cmd);
        assert_eq!(bytes.len(), 64);
        assert_eq!(bytes[13], seq);
    }
}

#[test]
fn test_error_detection() {
    use physerver::protocol::types::StatusMessage;
    let off_crc = std::mem::offset_of!(StatusMessage, crc);

    // Create valid status
    let mut data = [0u8; 64];
    data[0] = 0xAA;
    data[1] = 0x55;

    let crc = crc16_ccitt_table(&data[..off_crc]);
    data[off_crc] = (crc & 0xFF) as u8;
    data[off_crc + 1] = (crc >> 8) as u8;

    // Valid decode
    assert!(decode_status(&data).is_ok());

    // Corrupt a byte in the middle
    data[10] = 0xFF;

    // Should fail CRC check
    assert!(decode_status(&data).is_err());
}
