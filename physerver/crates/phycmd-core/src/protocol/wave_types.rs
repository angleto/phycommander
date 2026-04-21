//! On-chip function generator wire types.
//!
//! Byte-identical mirror of the firmware definitions in
//! `ATSAM3X8E_FW/src/waveform.h`. Both sides assert struct sizes at
//! compile time; if either side drifts, the build breaks immediately.
//!
//! See `docs/firmware/PROTOCOL.md` for full semantics.

use serde::{Deserialize, Serialize};

// ---- Vendor SETUP request opcodes ----------------------------------------

pub const VREQ_GEN_GET_CAPS: u8 = 0x10;
pub const VREQ_GEN_GET_STATE: u8 = 0x11;
pub const VREQ_GEN_PLAY_BUILTIN: u8 = 0x12;
pub const VREQ_GEN_PLAY_ARBITRARY: u8 = 0x13;
pub const VREQ_GEN_STOP: u8 = 0x14;
pub const VREQ_DAC_SET_CLOCK: u8 = 0x18;
pub const VREQ_DAC_GET_CLOCK: u8 = 0x19;
pub const VREQ_ADC_SET_RATE: u8 = 0x20;
pub const VREQ_ADC_GET_RATE: u8 = 0x21;
pub const VREQ_GEN_PLAY_LUT: u8 = 0x30;
pub const VREQ_GEN_PLAY_THRESHOLD: u8 = 0x31;
pub const VREQ_GEN_PLAY_PULSE_TRIG: u8 = 0x32;
pub const VREQ_GEN_PLAY_PID: u8 = 0x38;

// ---- Wave shape & channel kind enums (single byte each on the wire) -----

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaveShape {
    Off = 0,
    Dc = 1,
    Sine = 2,
    Square = 3,
    Triangle = 4,
    Sawtooth = 5,
    Arbitrary = 6,
    Lut = 16,
    Threshold = 17,
    PulseTrig = 18,
    Pid = 19,
}

impl WaveShape {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Off),
            1 => Some(Self::Dc),
            2 => Some(Self::Sine),
            3 => Some(Self::Square),
            4 => Some(Self::Triangle),
            5 => Some(Self::Sawtooth),
            6 => Some(Self::Arbitrary),
            16 => Some(Self::Lut),
            17 => Some(Self::Threshold),
            18 => Some(Self::PulseTrig),
            19 => Some(Self::Pid),
            _ => None,
        }
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    Dac = 0,
    Pwm = 1,
    Dout = 2,
    Din = 3,
    Adc = 4,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputSrc {
    None = 0,
    Adc = 1,
    DinMask = 2,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PulseEdge {
    Rising = 1,
    Falling = 2,
    Any = 3,
}

// ---- Mode bitmask (uint8 in Capabilities) -------------------------------

pub const MODE_MANUAL: u8 = 1 << 0;
pub const MODE_BUILTIN: u8 = 1 << 1;
pub const MODE_ARBITRARY: u8 = 1 << 2;
pub const MODE_LUT: u8 = 1 << 3;
pub const MODE_THRESHOLD: u8 = 1 << 4;
pub const MODE_PULSE_TRIG: u8 = 1 << 5;
pub const MODE_PID: u8 = 1 << 6;

// ---- ChannelState.flags bits ------------------------------------------
//
// Mirror of `CHAN_STATE_FLAG_*` in `waveform.h`.

/// The channel ID is defined in the protocol but not backed by hardware
/// on this firmware revision. Currently set for PWM 4..7 on Arduino
/// Due: the IDs exist so hosts can iterate 0..num_pwm uniformly, but
/// BUILTIN / ARBITRARY requests on them return STALL and
/// GEN_GET_STATE flags this bit. Clients should render such channels
/// as unavailable.
pub const CHAN_STATE_FLAG_RESERVED: u8 = 1 << 0;

// ---- Wire-format structs (must stay byte-identical with waveform.h) -----

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub struct Capabilities {
    pub protocol_version: u8,
    pub _reserved0: u8,
    pub firmware_minor: u16,
    pub firmware_major: u16,
    pub _reserved1: u16,
    pub num_dac: u8,
    pub num_pwm: u8,
    pub num_dout: u8,
    pub num_din: u8,
    pub num_adc: u8,
    pub _reserved2: [u8; 3],
    pub modes_dac: u8,
    pub modes_pwm: u8,
    pub modes_dout: u8,
    pub modes_din: u8,
    pub modes_adc: u8,
    pub _reserved3: [u8; 3],
    pub max_dac_sample_rate_hz: u32,
    pub max_arb_buffer_samples: u32,
}
const _: () = assert!(std::mem::size_of::<Capabilities>() == 32);

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub struct WaveBuiltinSpec {
    pub shape: u8,
    pub flags: u8,
    pub duty_x10: u16,
    pub amplitude: u16,
    pub offset: u16,
    pub freq_mhz: u32,
    pub phase_offset_x16: u16,
    pub _reserved1: u16,
}
const _: () = assert!(std::mem::size_of::<WaveBuiltinSpec>() == 16);

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub struct WaveArbHeader {
    pub n_samples: u16,
    pub loop_count: u16,
    pub sample_rate_hz: u32,
    // followed by int16_t samples[n_samples]
}
const _: () = assert!(std::mem::size_of::<WaveArbHeader>() == 8);

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub struct WaveLutSpec {
    pub input_src: u8,
    pub _reserved0: u8,
    pub input_arg: u16,
    pub n_entries: u16,
    pub output_mask: u16,
    pub _reserved1: u32,
    // followed by int16_t entries[n_entries]
}
const _: () = assert!(std::mem::size_of::<WaveLutSpec>() == 12);

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub struct WaveThresholdSpec {
    pub input_src: u8,
    pub _reserved0: u8,
    pub input_arg: u16,
    pub thr_high: u16,
    pub thr_low: u16,
    pub val_high: u16,
    pub val_low: u16,
    pub _reserved1: u32,
}
const _: () = assert!(std::mem::size_of::<WaveThresholdSpec>() == 16);

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub struct WavePulseSpec {
    pub input_din_bit: u8,
    pub edge: u8,
    pub active_level: u8,
    pub _reserved0: u8,
    pub duration_us: u32,
    pub cooldown_us: u32,
}
const _: () = assert!(std::mem::size_of::<WavePulseSpec>() == 12);

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub struct WavePidSpec {
    pub input_src: u8,
    pub _reserved0: u8,
    pub input_arg: u16,
    pub sample_rate_hz: u32,
    pub setpoint: i32,
    pub kp_q16_16: i32,
    pub ki_q16_16: i32,
    pub kd_q16_16: i32,
    pub out_min: u16,
    pub out_max: u16,
    pub integral_clamp: i32,
}
const _: () = assert!(std::mem::size_of::<WavePidSpec>() == 32);

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub struct ChannelState {
    pub channel_kind: u8,
    pub channel_index: u8,
    pub shape: u8,
    pub flags: u8,
    pub freq_mhz: u32,
    pub duty_x10: u16,
    pub amplitude: u16,
    pub offset: u16,
    pub phase_offset_x16: u16,
    pub arb_n_samples: u16,
    pub arb_loops_remaining: u16,
    pub arb_sample_rate_hz: u32,
    pub cur_phase_q24_8: u32,
    pub _reserved: u32,
}
const _: () = assert!(std::mem::size_of::<ChannelState>() == 32);

// ---- Channel ID flat encoding (matches firmware's channel_decode) -------

pub const CH_NUM_DAC: u16 = 2;
pub const CH_NUM_PWM: u16 = 8;
pub const CH_NUM_DOUT: u16 = 16;
pub const CH_NUM_DIN: u16 = 16;
pub const CH_NUM_ADC: u16 = 8;

pub fn channel_id(kind: ChannelKind, index: u8) -> u16 {
    let base = match kind {
        ChannelKind::Dac => 0,
        ChannelKind::Pwm => CH_NUM_DAC,
        ChannelKind::Dout => CH_NUM_DAC + CH_NUM_PWM,
        ChannelKind::Din => CH_NUM_DAC + CH_NUM_PWM + CH_NUM_DOUT,
        ChannelKind::Adc => CH_NUM_DAC + CH_NUM_PWM + CH_NUM_DOUT + CH_NUM_DIN,
    };
    base + index as u16
}

/// Parse "dac0", "dout5", etc. into a flat channel id.
pub fn channel_id_from_name(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    for (prefix, kind, count) in &[
        ("dac", ChannelKind::Dac, CH_NUM_DAC),
        ("pwm", ChannelKind::Pwm, CH_NUM_PWM),
        ("dout", ChannelKind::Dout, CH_NUM_DOUT),
        ("din", ChannelKind::Din, CH_NUM_DIN),
        ("adc", ChannelKind::Adc, CH_NUM_ADC),
    ] {
        if let Some(idx_str) = lower.strip_prefix(prefix) {
            if let Ok(idx) = idx_str.parse::<u16>() {
                if idx < *count {
                    return Some(channel_id(*kind, idx as u8));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn id_round_trip() {
        assert_eq!(channel_id_from_name("dac0"), Some(0));
        assert_eq!(channel_id_from_name("dac1"), Some(1));
        assert_eq!(channel_id_from_name("pwm0"), Some(2));
        assert_eq!(channel_id_from_name("dout0"), Some(10));
        assert_eq!(channel_id_from_name("dout15"), Some(25));
        assert_eq!(channel_id_from_name("din0"), Some(26));
        assert_eq!(channel_id_from_name("adc7"), Some(49));
        assert_eq!(channel_id_from_name("dac9"), None); // out of range
        assert_eq!(channel_id_from_name("xyz"), None);
    }
}
