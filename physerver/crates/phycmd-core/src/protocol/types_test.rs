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
        assert_eq!(status.adc, [0; 12]);
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

    #[test]
    fn test_status_flags_roundtrip() {
        let original = StatusFlags {
            adc_active: true,
            dac_active: false,
            pwm_active: true,
            error: true,
            watchdog_triggered: false,
            usb_configured: true,
            overrun: false,
        };
        let byte = original.to_byte();
        let recovered = StatusFlags::from_byte(byte);

        assert_eq!(original.adc_active, recovered.adc_active);
        assert_eq!(original.dac_active, recovered.dac_active);
        assert_eq!(original.pwm_active, recovered.pwm_active);
        assert_eq!(original.error, recovered.error);
        assert_eq!(original.watchdog_triggered, recovered.watchdog_triggered);
        assert_eq!(original.usb_configured, recovered.usb_configured);
        assert_eq!(original.overrun, recovered.overrun);
    }

    #[test]
    fn test_command_clone() {
        let cmd1 = Command {
            digital_out: 0xFF,
            dac: [100, 200],
            pwm: [300, 400],
            flags: CommandFlags {
                adc_enable: true,
                dac_enable: true,
                pwm_enable: false,
                reset_seq: false,
                watchdog_disable: false,
            },
            seq_num: 5,
        };

        let cmd2 = cmd1.clone();
        assert_eq!(cmd1.digital_out, cmd2.digital_out);
        assert_eq!(cmd1.dac, cmd2.dac);
        assert_eq!(cmd1.pwm, cmd2.pwm);
        assert_eq!(cmd1.seq_num, cmd2.seq_num);
    }

    #[test]
    fn test_status_clone() {
        let status1 = Status {
            digital_in: 0xFF,
            digital_out: 0xAA,
            adc: [100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1100, 1200],
            flags: StatusFlags::default(),
            seq_num: 10,
            loop_time_us: 150,
            uptime_ms: 100000,
            error_count: 5,
        };

        let status2 = status1.clone();
        assert_eq!(status1.digital_in, status2.digital_in);
        assert_eq!(status1.digital_out, status2.digital_out);
        assert_eq!(status1.adc, status2.adc);
        assert_eq!(status1.seq_num, status2.seq_num);
        assert_eq!(status1.loop_time_us, status2.loop_time_us);
        assert_eq!(status1.uptime_ms, status2.uptime_ms);
        assert_eq!(status1.error_count, status2.error_count);
    }

    #[test]
    fn test_command_flags_all_combinations() {
        // Test all possible combinations of flags
        for i in 0..32u8 {
            let flags = CommandFlags::from_byte(i);
            let byte = flags.to_byte();
            assert_eq!(byte, i);
        }
    }

    #[test]
    fn test_status_flags_all_combinations() {
        // Test all possible combinations of flags
        for i in 0..128u8 {
            let flags = StatusFlags::from_byte(i);
            let byte = flags.to_byte();
            assert_eq!(byte, i);
        }
    }

    #[test]
    fn test_command_with_max_values() {
        let cmd = Command {
            digital_out: 0xFFFF,
            dac: [4095, 4095],
            pwm: [65535, 65535],
            flags: CommandFlags {
                adc_enable: true,
                dac_enable: true,
                pwm_enable: true,
                reset_seq: true,
                watchdog_disable: true,
            },
            seq_num: 255,
        };

        assert_eq!(cmd.digital_out, 0xFFFF);
        assert_eq!(cmd.dac[0], 4095);
        assert_eq!(cmd.dac[1], 4095);
        assert_eq!(cmd.pwm[0], 65535);
        assert_eq!(cmd.pwm[1], 65535);
        assert_eq!(cmd.seq_num, 255);
    }

    #[test]
    fn test_status_with_max_values() {
        let status = Status {
            digital_in: 0xFFFF,
            digital_out: 0xFFFF,
            adc: [4095; 12],
            flags: StatusFlags {
                adc_active: true,
                dac_active: true,
                pwm_active: true,
                error: true,
                watchdog_triggered: true,
                usb_configured: true,
                overrun: true,
            },
            seq_num: 255,
            loop_time_us: 65535,
            uptime_ms: 0xFFFFFFFF,
            error_count: 65535,
        };

        assert_eq!(status.digital_in, 0xFFFF);
        assert_eq!(status.digital_out, 0xFFFF);
        for &adc_val in &status.adc {
            assert_eq!(adc_val, 4095);
        }
        assert_eq!(status.seq_num, 255);
        assert_eq!(status.loop_time_us, 65535);
        assert_eq!(status.uptime_ms, 0xFFFFFFFF);
        assert_eq!(status.error_count, 65535);
    }

    #[test]
    fn test_message_sizes() {
        assert_eq!(std::mem::size_of::<CommandMessage>(), 64);
        assert_eq!(std::mem::size_of::<StatusMessage>(), 64);
    }
}
