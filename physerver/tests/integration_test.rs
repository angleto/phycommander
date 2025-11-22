/// Integration tests for physerver
///
/// These tests verify the complete system integration

use physerver::{Command, CommandFlags, Status, StatusFlags};
use physerver::protocol::{encode_command, decode_status};

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
    // Create a mock status message
    let mut data = [0u8; 64];

    // Header
    data[0] = 0xAA;
    data[1] = 0x55;

    // Digital I/O
    data[2] = 0xFF;
    data[3] = 0x0F;
    data[4] = 0x00;
    data[5] = 0xF0;

    // ADC values
    for i in 0..8 {
        let val = (i + 1) * 500;
        data[6 + i * 2] = (val & 0xFF) as u8;
        data[7 + i * 2] = (val >> 8) as u8;
    }

    // Flags
    data[22] = 0x07; // ADC, DAC, PWM active

    // Seq num
    data[23] = 42;

    // Calculate CRC
    let crc = physerver::protocol::crc16_ccitt_table(&data[0..24]);
    data[24] = (crc & 0xFF) as u8;
    data[25] = (crc >> 8) as u8;

    // Loop time
    data[26] = 150;
    data[27] = 0;

    // Uptime
    data[28] = 0x00;
    data[29] = 0xE1;
    data[30] = 0xF5;
    data[31] = 0x05; // 100000000 ms

    // Decode
    let status = decode_status(&data).expect("Failed to decode status");

    assert_eq!(status.digital_in, 0x0FFF);
    assert_eq!(status.digital_out, 0xF000);
    assert_eq!(status.adc[0], 500);
    assert_eq!(status.adc[7], 4000);
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
    // Create valid status
    let mut data = [0u8; 64];
    data[0] = 0xAA;
    data[1] = 0x55;

    let crc = physerver::protocol::crc16_ccitt_table(&data[0..24]);
    data[24] = (crc & 0xFF) as u8;
    data[25] = (crc >> 8) as u8;

    // Valid decode
    assert!(decode_status(&data).is_ok());

    // Corrupt a byte in the middle
    data[10] = 0xFF;

    // Should fail CRC check
    assert!(decode_status(&data).is_err());
}
