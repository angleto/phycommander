pub mod codec;
pub mod crc;
pub mod types;
pub mod wave_types;

pub use codec::{decode_status, encode_command};
pub use crc::crc16_ccitt;
use thiserror::Error;
pub use types::*;
pub use wave_types::{
    channel_id, channel_id_from_name, Capabilities, ChannelKind, ChannelState, InputSrc, PulseEdge,
    WaveArbHeader, WaveBuiltinSpec, WaveLutSpec, WavePidSpec, WavePulseSpec, WaveShape,
    WaveThresholdSpec,
};

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Invalid header: expected {expected:#06x}, got {got:#06x}")]
    InvalidHeader { expected: u16, got: u16 },

    #[error("CRC mismatch: expected {expected:#06x}, got {got:#06x}")]
    CrcMismatch { expected: u16, got: u16 },

    #[error("Invalid message length: expected {expected}, got {got}")]
    InvalidLength { expected: usize, got: usize },

    #[error("Sequence number mismatch: expected {expected}, got {got}")]
    SequenceMismatch { expected: u8, got: u8 },

    #[error("Invalid field value: {field} = {value}")]
    InvalidField { field: String, value: u16 },
}

pub type Result<T> = std::result::Result<T, ProtocolError>;

/// Protocol constants
pub const MESSAGE_SIZE: usize = 64;
pub const COMMAND_HEADER: u16 = 0xAA55;
pub const STATUS_HEADER: u16 = 0x55AA;

/// DAC and ADC resolution
pub const DAC_MAX: u16 = 4095; // 12-bit
pub const ADC_MAX: u16 = 4095; // 12-bit
pub const PWM_MAX: u16 = 65535; // 16-bit

/// Command flags
pub const FLAG_ADC_ENABLE: u8 = 0b00000001;
pub const FLAG_DAC_ENABLE: u8 = 0b00000010;
pub const FLAG_PWM_ENABLE: u8 = 0b00000100;
pub const FLAG_RESET_SEQ: u8 = 0b00001000;
pub const FLAG_WATCHDOG_DISABLE: u8 = 0b00010000;

/// Status flags
pub const STATUS_ADC_ACTIVE: u8 = 0b00000001;
pub const STATUS_DAC_ACTIVE: u8 = 0b00000010;
pub const STATUS_PWM_ACTIVE: u8 = 0b00000100;
pub const STATUS_ERROR_FLAG: u8 = 0b00001000;
pub const STATUS_WATCHDOG_TRIGGERED: u8 = 0b00010000;
pub const STATUS_USB_CONFIGURED: u8 = 0b00100000;
pub const STATUS_OVERRUN: u8 = 0b01000000;
