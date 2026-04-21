//! Built-in periodic waveform generator, evaluated per microframe.
//!
//! When iso mode is active, the [`crate::transport::IsoTransport`]
//! consults a [`WaveformBank`] in its OUT callback. For every channel
//! whose [`WaveformSpec::enabled`] flag is set, the per-packet command
//! has its corresponding field overwritten with the waveform sample,
//! ignoring whatever value [`crate::staging::CommandStaging`] carried
//! at that instant. Sampling rate equals the iso microframe rate
//! (8 kHz on HS), which is also the maximum waveform Nyquist frequency
//! of 4 kHz — well above what the SAM3X DAC can faithfully reproduce
//! anyway (its slew rate caps somewhere below 1 MHz, but the practical
//! ceiling on a 12-bit DAC at 8 kSPS is a few hundred Hz of clean sine).
//!
//! ## Interaction with the on-chip firmware function generator
//!
//! The firmware exposes a **second** waveform generator via vendor
//! SETUP requests (see `waveform.c` and the `/api/fngen/*` REST routes).
//! That one lives entirely on the SAM3X: PDC + TC triggers DAC
//! conversions at up to 1 MSPS, independently of USB traffic.
//!
//! If **both** generators target the same DAC channel, the firmware
//! wins: `apply_command_frame` checks `waveform_dac_is_generating(idx)`
//! before writing the Command frame's `dacN` field, and skips the
//! write when a firmware generator is running. Any per-microframe
//! value this `WaveformBank` computes is silently ignored for that
//! channel until the firmware generator is stopped (via
//! `/api/fngen/stop/dacN`).
//!
//! This is by design — two generators fighting on the same physical
//! DAC would alias unpredictably. The dashboard can mask host-side
//! UI on a channel that is currently firmware-driven; not enforced
//! in core, only documented here.
//!
//! All fields are `Copy` and stored behind small read-locks so the
//! HTTP layer can update them at any time without disturbing the iso
//! callback's hot path beyond a single uncontended `read()`.
//!
//! Unit-conversion contract:
//!   * `amplitude` and `offset` are in raw device units
//!     (DAC: 0–4095, PWM: 0–65535).
//!   * The generated waveform is centered on `offset` and swings
//!     ±`amplitude / 2`, then clamped to the full DAC/PWM range.
//!     A pure DC level is `{ amplitude: 0, offset: <value> }`.
//!   * `freq_hz` is the fundamental in Hz.
//!   * `duty` is the high-fraction for `Square` (ignored otherwise).

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Waveform shape selector. Serialised as the lowercase variant name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaveformShape {
    /// Constant level at `offset` (amplitude is ignored).
    Dc,
    /// `offset + (amplitude / 2) * sin(2π · freq · t)` clamped to the
    /// channel range.
    Sine,
    /// Square at `freq`, high during the first `duty` fraction of each
    /// period. Default `duty = 0.5` gives a 50% duty cycle.
    Square,
    /// Symmetric triangle (peak at mid-period).
    Triangle,
    /// Rising sawtooth, resets to minimum at the period boundary.
    Sawtooth,
}

impl Default for WaveformShape {
    fn default() -> Self {
        WaveformShape::Dc
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct WaveformSpec {
    /// Master enable. When `false` the generator yields its channel
    /// back to [`crate::staging::CommandStaging`].
    pub enabled: bool,
    pub shape: WaveformShape,
    pub freq_hz: f32,
    /// Peak-to-peak swing in raw device units.
    pub amplitude: u32,
    /// Mid-point of the swing in raw device units.
    pub offset: u32,
    /// High-fraction for `Square` (0.0–1.0). Ignored for other shapes.
    pub duty: f32,
    /// Maximum legal output value for the channel (4095 for DAC,
    /// 65535 for PWM). Set by the constructor that knows the channel
    /// kind; the API never lets the client override it.
    #[serde(skip)]
    pub max_value: u32,
}

impl Default for WaveformSpec {
    fn default() -> Self {
        Self {
            enabled: false,
            shape: WaveformShape::Dc,
            freq_hz: 1.0,
            amplitude: 0,
            offset: 0,
            duty: 0.5,
            max_value: 4095,
        }
    }
}

impl WaveformSpec {
    /// Compute the device-units sample for a given packet index `t`
    /// at sampling rate `sr_hz`.
    #[inline]
    pub fn sample(&self, t: u64, sr_hz: f32) -> u16 {
        if !self.enabled {
            return 0;
        }
        let phase = ((t as f64) * (self.freq_hz as f64) / (sr_hz as f64)).rem_euclid(1.0) as f32;
        let unit = match self.shape {
            WaveformShape::Dc => 0.0,
            WaveformShape::Sine => (2.0 * std::f32::consts::PI * phase).sin(),
            WaveformShape::Square => {
                if phase < self.duty.clamp(0.0, 1.0) {
                    1.0
                } else {
                    -1.0
                }
            }
            // Triangle: rises 0→1 in [0, 0.5], falls 1→-1 in [0.5, 1].
            // Re-centred to ±1 around 0.
            WaveformShape::Triangle => {
                if phase < 0.5 {
                    4.0 * phase - 1.0
                } else {
                    3.0 - 4.0 * phase
                }
            }
            // Sawtooth: -1 at phase=0, +1 at phase→1.
            WaveformShape::Sawtooth => 2.0 * phase - 1.0,
        };
        let value = (self.offset as f32) + (self.amplitude as f32) * 0.5 * unit;
        value.clamp(0.0, self.max_value as f32) as u16
    }
}

/// One independent waveform per controllable channel. Each spec lives
/// behind its own [`RwLock`] so the HTTP layer can update one channel
/// without contending with the iso callback's read of the other three.
#[derive(Default)]
pub struct WaveformBank {
    pub dac0: RwLock<WaveformSpec>,
    pub dac1: RwLock<WaveformSpec>,
    pub pwm0: RwLock<WaveformSpec>,
    pub pwm1: RwLock<WaveformSpec>,
    /// Monotonic packet counter shared with the iso loop. Public so
    /// that consumers can compute consistent `t` values across calls.
    pub packet_counter: AtomicU64,
}

impl WaveformBank {
    pub fn new() -> Self {
        let mut bank = Self::default();
        // Set the per-channel max_value defaults.
        bank.dac0.get_mut().max_value = 4095;
        bank.dac1.get_mut().max_value = 4095;
        bank.pwm0.get_mut().max_value = 65535;
        bank.pwm1.get_mut().max_value = 65535;
        bank
    }

    /// Channel selector by string name (matches the REST URL slug).
    pub fn channel(&self, name: &str) -> Option<&RwLock<WaveformSpec>> {
        match name {
            "dac0" => Some(&self.dac0),
            "dac1" => Some(&self.dac1),
            "pwm0" => Some(&self.pwm0),
            "pwm1" => Some(&self.pwm1),
            _ => None,
        }
    }

    /// Apply the spec from the API to the named channel, preserving
    /// the channel's `max_value` (the client must not override the
    /// hardware range).
    pub fn set(&self, name: &str, spec: WaveformSpec) -> Result<(), &'static str> {
        let lock = self.channel(name).ok_or("unknown channel")?;
        let mut g = lock.write();
        let max_value = g.max_value;
        *g = WaveformSpec { max_value, ..spec };
        Ok(())
    }

    pub fn next_packet_idx(&self, n: u64) -> u64 {
        self.packet_counter.fetch_add(n, Ordering::Relaxed)
    }

    /// Snapshot used by GET /api/waveform to render the current state.
    pub fn snapshot(&self) -> WaveformBankSnapshot {
        WaveformBankSnapshot {
            dac0: *self.dac0.read(),
            dac1: *self.dac1.read(),
            pwm0: *self.pwm0.read(),
            pwm1: *self.pwm1.read(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WaveformBankSnapshot {
    pub dac0: WaveformSpec,
    pub dac1: WaveformSpec,
    pub pwm0: WaveformSpec,
    pub pwm1: WaveformSpec,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_returns_offset_when_enabled() {
        let spec = WaveformSpec {
            enabled: true,
            shape: WaveformShape::Dc,
            offset: 2048,
            max_value: 4095,
            ..Default::default()
        };
        assert_eq!(spec.sample(0, 8000.0), 2048);
        assert_eq!(spec.sample(12345, 8000.0), 2048);
    }

    #[test]
    fn disabled_returns_zero() {
        let spec = WaveformSpec { enabled: false, offset: 1000, ..Default::default() };
        assert_eq!(spec.sample(0, 8000.0), 0);
    }

    #[test]
    fn sine_centered_on_offset() {
        let spec = WaveformSpec {
            enabled: true,
            shape: WaveformShape::Sine,
            freq_hz: 1.0,
            amplitude: 4000,
            offset: 2048,
            max_value: 4095,
            ..Default::default()
        };
        // At t=0 phase=0 sin=0 → offset
        assert_eq!(spec.sample(0, 8000.0), 2048);
        // At t=2000 (1/4 of 1Hz at 8kHz) phase=0.25 sin=1 → offset + amp/2 = 4048
        let s = spec.sample(2000, 8000.0);
        assert!((s as i32 - 4048).abs() <= 1, "got {s}");
        // At t=6000 phase=0.75 sin=-1 → offset - amp/2 = 48
        let s = spec.sample(6000, 8000.0);
        assert!((s as i32 - 48).abs() <= 1, "got {s}");
    }

    #[test]
    fn square_50pct_high_then_low() {
        let spec = WaveformSpec {
            enabled: true,
            shape: WaveformShape::Square,
            freq_hz: 1000.0,
            amplitude: 4000,
            offset: 2048,
            duty: 0.5,
            max_value: 4095,
        };
        // At t=0..3 high half: offset + amp/2 = 4048
        assert_eq!(spec.sample(0, 8000.0), 4048);
        assert_eq!(spec.sample(2, 8000.0), 4048);
        // At t=4..7 low half: offset - amp/2 = 48
        assert_eq!(spec.sample(4, 8000.0), 48);
        assert_eq!(spec.sample(7, 8000.0), 48);
    }

    #[test]
    fn clamps_to_max_value() {
        let spec = WaveformSpec {
            enabled: true,
            shape: WaveformShape::Dc,
            offset: 99999,
            max_value: 4095,
            ..Default::default()
        };
        assert_eq!(spec.sample(0, 8000.0), 4095);
    }
}
