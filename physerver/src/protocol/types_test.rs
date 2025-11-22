use super::*;

#[cfg(test)]
mod protocol_tests {
    use super::*;

    #[test]
    fn test_command_default() {
        let cmd = Command::default();
        assert_eq!(cmd.digital_out, 0);
        assert_eq!(cmd.dac, [0, 0]);
        assert_eq!(cmd.pwm, [0, 0]);
        assert_eq!(cmd.seq_num, 0);
    }

    #[test]
    fn test_status_default() {
        let status = Status::default();
        assert_eq!(status.digital_in, 0);
        assert_eq!(status.digital_out, 0);
        assert_eq!(status.adc, [0; 8]);
        assert_eq!(status.seq_num, 0);
        assert_eq!(status.loop_time_us, 0);
        assert_eq!(status.uptime_ms, 0);
        assert_eq!(status.error_count, 0);
    }

    #[test]
    fn test_command_flags_to_byte() {
        let flags = CommandFlags {
            adc_enable: true,
            dac_enable: true,
            pwm_enable: false,
            reset_seq: false,
            watchdog_disable: false,
        };
        assert_eq!(flags.to_byte(), 0b00000011);

        let flags_all = CommandFlags {
            adc_enable: true,
            dac_enable: true,
            pwm_enable: true,
            reset_seq: true,
            watchdog_disable: true,
        };
        assert_eq!(flags_all.to_byte(), 0b00011111);
    }

    #[test]
    fn test_command_flags_from_byte() {
        let flags = CommandFlags::from_byte(0b00000011);
        assert!(flags.adc_enable);
        assert!(flags.dac_enable);
        assert!(!flags.pwm_enable);
        assert!(!flags.reset_seq);
        assert!(!flags.watchdog_disable);
    }

    #[test]
    fn test_status_flags_to_byte() {
        let flags = StatusFlags {
            adc_active: true,
            dac_active: true,
            pwm_active: false,
            error: false,
            watchdog_triggered: false,
            usb_configured: true,
            overrun: false,
        };
        assert_eq!(flags.to_byte(), 0b00100011);
    }

    #[test]
    fn test_status_flags_from_byte() {
        let flags = StatusFlags::from_byte(0b01001111);
        assert!(flags.adc_active);
        assert!(flags.dac_active);
        assert!(flags.pwm_active);
        assert!(flags.error);
        assert!(!flags.watchdog_triggered);
        assert!(!flags.usb_configured);
        assert!(flags.overrun);
    }

    #[test]
    fn test_flags_roundtrip() {
        let original = CommandFlags {
            adc_enable: true,
            dac_enable: false,
            pwm_enable: true,
            reset_seq: false,
            watchdog_disable: true,
        };
        let byte = original.to_byte();
        let recovered = CommandFlags::from_byte(byte);

        assert_eq!(original.adc_enable, recovered.adc_enable);
        assert_eq!(original.dac_enable, recovered.dac_enable);
        assert_eq!(original.pwm_enable, recovered.pwm_enable);
        assert_eq!(original.reset_seq, recovered.reset_seq);
        assert_eq!(original.watchdog_disable, recovered.watchdog_disable);
    }
}
