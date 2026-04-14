use serde::{Deserialize, Serialize};

/// Command message sent from host to device (64 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CommandMessage {
    pub header: u16,        // 0xAA55
    pub digital_out: u16,   // GPIO outputs (16 bits)
    pub dac0: u16,          // DAC channel 0 (0-4095)
    pub dac1: u16,          // DAC channel 1 (0-4095)
    pub pwm0: u16,          // PWM channel 0 duty (0-65535)
    pub pwm1: u16,          // PWM channel 1 duty (0-65535)
    pub flags: u8,          // Control flags
    pub seq_num: u8,        // Sequence number
    pub crc: u16,           // CRC-16-CCITT
    pub reserved: [u8; 48], // Reserved for future use
}

/// Status message sent from device to host (64 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct StatusMessage {
    pub header: u16,        // 0x55AA
    pub digital_in: u16,    // GPIO inputs (16 bits)
    pub digital_out: u16,   // GPIO outputs echo
    pub adc: [u16; 8],      // ADC channels 0-7
    pub status_flags: u8,   // Status flags
    pub seq_num: u8,        // Sequence number echo
    pub crc: u16,           // CRC-16-CCITT
    pub loop_time_us: u16,  // Main loop time (µs)
    pub uptime_ms: u32,     // System uptime (ms)
    pub error_count: u16,   // Total error count
    pub reserved: [u8; 30], // Reserved
}

/// High-level command structure (safe, validated)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub digital_out: u16,
    pub dac: [u16; 2],
    pub pwm: [u16; 2],
    pub flags: CommandFlags,
    pub seq_num: u8,
}

/// High-level status structure (safe, validated)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub digital_in: u16,
    pub digital_out: u16,
    pub adc: [u16; 8],
    pub flags: StatusFlags,
    pub seq_num: u8,
    pub loop_time_us: u16,
    pub uptime_ms: u32,
    pub error_count: u16,
}

/// Command flags (high-level)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CommandFlags {
    pub adc_enable: bool,
    pub dac_enable: bool,
    pub pwm_enable: bool,
    pub reset_seq: bool,
    pub watchdog_disable: bool,
}

/// Status flags (high-level)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StatusFlags {
    pub adc_active: bool,
    pub dac_active: bool,
    pub pwm_active: bool,
    pub error: bool,
    pub watchdog_triggered: bool,
    pub usb_configured: bool,
    pub overrun: bool,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            digital_out: 0,
            dac: [0, 0],
            pwm: [0, 0],
            flags: CommandFlags::default(),
            seq_num: 0,
        }
    }
}

impl Default for Status {
    fn default() -> Self {
        Self {
            digital_in: 0,
            digital_out: 0,
            adc: [0; 8],
            flags: StatusFlags::default(),
            seq_num: 0,
            loop_time_us: 0,
            uptime_ms: 0,
            error_count: 0,
        }
    }
}

impl CommandFlags {
    pub fn to_byte(&self) -> u8 {
        let mut byte = 0u8;
        if self.adc_enable {
            byte |= super::FLAG_ADC_ENABLE;
        }
        if self.dac_enable {
            byte |= super::FLAG_DAC_ENABLE;
        }
        if self.pwm_enable {
            byte |= super::FLAG_PWM_ENABLE;
        }
        if self.reset_seq {
            byte |= super::FLAG_RESET_SEQ;
        }
        if self.watchdog_disable {
            byte |= super::FLAG_WATCHDOG_DISABLE;
        }
        byte
    }

    pub fn from_byte(byte: u8) -> Self {
        Self {
            adc_enable: byte & super::FLAG_ADC_ENABLE != 0,
            dac_enable: byte & super::FLAG_DAC_ENABLE != 0,
            pwm_enable: byte & super::FLAG_PWM_ENABLE != 0,
            reset_seq: byte & super::FLAG_RESET_SEQ != 0,
            watchdog_disable: byte & super::FLAG_WATCHDOG_DISABLE != 0,
        }
    }
}

impl StatusFlags {
    pub fn from_byte(byte: u8) -> Self {
        Self {
            adc_active: byte & super::STATUS_ADC_ACTIVE != 0,
            dac_active: byte & super::STATUS_DAC_ACTIVE != 0,
            pwm_active: byte & super::STATUS_PWM_ACTIVE != 0,
            error: byte & super::STATUS_ERROR_FLAG != 0,
            watchdog_triggered: byte & super::STATUS_WATCHDOG_TRIGGERED != 0,
            usb_configured: byte & super::STATUS_USB_CONFIGURED != 0,
            overrun: byte & super::STATUS_OVERRUN != 0,
        }
    }

    pub fn to_byte(&self) -> u8 {
        let mut byte = 0u8;
        if self.adc_active {
            byte |= super::STATUS_ADC_ACTIVE;
        }
        if self.dac_active {
            byte |= super::STATUS_DAC_ACTIVE;
        }
        if self.pwm_active {
            byte |= super::STATUS_PWM_ACTIVE;
        }
        if self.error {
            byte |= super::STATUS_ERROR_FLAG;
        }
        if self.watchdog_triggered {
            byte |= super::STATUS_WATCHDOG_TRIGGERED;
        }
        if self.usb_configured {
            byte |= super::STATUS_USB_CONFIGURED;
        }
        if self.overrun {
            byte |= super::STATUS_OVERRUN;
        }
        byte
    }
}

// Ensure sizes are correct
const _: () = assert!(std::mem::size_of::<CommandMessage>() == 64);
const _: () = assert!(std::mem::size_of::<StatusMessage>() == 64);
