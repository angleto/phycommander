//! Command staging buffer shared between user API writers and the
//! hard-paced RT scheduler.
//!
//! Writers ([`CommandStaging::set_dac0`], [`CommandStaging::set_digital_out_bit`],
//! ...) compose the *next* frame; the scheduler calls
//! [`CommandStaging::take_snapshot`] at each tick, performs the USB
//! exchange, and then calls [`CommandStaging::mark_sent`] to release
//! any [`WriteMode::BlockUntilSent`] waiters.
//!
//! ## Three write modes
//!
//! * [`WriteMode::Coalesce`] — lock-free-ish overwrite. Multiple writes
//!   to the same field between ticks all collapse to the latest value.
//!   Ideal for dashboards, setpoint tracking, telemetry inputs.
//!
//! * [`WriteMode::BlockUntilSent`] (default) — if the field you are
//!   writing is already marked dirty (a previous value is pending
//!   transmission), the caller blocks on a condition variable until
//!   the RT scheduler has transmitted that pending value. Each field
//!   has its own logical wait: two writers setting *different* fields
//!   never block each other, two writers setting *the same* field
//!   serialise at the tick rate.
//!
//! * [`WriteMode::ErrorOnConflict`] — like [`WriteMode::BlockUntilSent`]
//!   but returns [`StagingError::WouldOverwrite`] instead of blocking.
//!   Primarily a debug / safety-critical mode that surfaces "writer is
//!   faster than the tick rate" as an explicit error.
//!
//! ## Per-field granularity
//!
//! The dirty bitmask is fine-grained: each of the 16 bits of
//! [`Command::digital_out`] has its own dirty bit, so two threads
//! toggling different GPIO pins never block each other even when one
//! is in BlockUntilSent mode. The DAC, PWM and flags fields each have
//! a single dirty bit.
//!
//! Internally everything is protected by a single `parking_lot::Mutex`
//! on the inner state, which keeps the snapshot path cheap (one lock
//! acquire per tick) and avoids any lock-order concerns. The
//! "per-field" aspect is expressed through a `wait_while` predicate
//! that checks only the bitmask bits relevant to the caller.

use crate::protocol::{Command, CommandFlags};
use parking_lot::{Condvar, Mutex};
use std::sync::atomic::{AtomicU8, Ordering};
use thiserror::Error;

// -------------------------------------------------------------------------
//   Dirty bitmask layout (u32)
// -------------------------------------------------------------------------
//   bits  0..15 = digital_out bits 0..15 (one dirty bit per GPIO)
//   bit   16    = dac0
//   bit   17    = dac1
//   bit   18    = pwm0
//   bit   19    = pwm1
//   bit   20    = flags
//   bits 21..31 = reserved for future fields
// -------------------------------------------------------------------------

/// Mask covering all 16 `digital_out` bits.
pub const DIRTY_DIGITAL_OUT_ALL: u32 = 0x0000_FFFF;
pub const DIRTY_DAC0: u32 = 1 << 16;
pub const DIRTY_DAC1: u32 = 1 << 17;
pub const DIRTY_PWM0: u32 = 1 << 18;
pub const DIRTY_PWM1: u32 = 1 << 19;
pub const DIRTY_FLAGS: u32 = 1 << 20;

/// Mask for a single `digital_out` bit (0..16).
#[inline]
pub const fn dirty_digital_out_bit(bit: u8) -> u32 {
    1u32 << (bit as u32)
}

// -------------------------------------------------------------------------
//   Write modes
// -------------------------------------------------------------------------

/// Semantics applied when a writer tries to set a field that is
/// already pending transmission.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WriteMode {
    /// Overwrite the pending value with the new one. Never blocks.
    /// Multiple writes to the same field between ticks all collapse
    /// to the latest value.
    Coalesce = 0,
    /// Block the caller until the pending value for this field has
    /// been transmitted on the bus. Default.
    BlockUntilSent = 1,
    /// Return [`StagingError::WouldOverwrite`] if the field is
    /// already dirty. Primarily a debug / safety-critical mode.
    ErrorOnConflict = 2,
}

impl Default for WriteMode {
    fn default() -> Self {
        WriteMode::BlockUntilSent
    }
}

impl WriteMode {
    #[inline]
    fn from_u8(v: u8) -> Self {
        match v {
            0 => WriteMode::Coalesce,
            1 => WriteMode::BlockUntilSent,
            2 => WriteMode::ErrorOnConflict,
            _ => WriteMode::BlockUntilSent,
        }
    }
}

// -------------------------------------------------------------------------
//   Errors
// -------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StagingError {
    #[error("field is already pending transmission (dirty); use Coalesce or BlockUntilSent to overwrite")]
    WouldOverwrite,

    #[error("invalid digital_out bit index: {0} (must be 0..16)")]
    InvalidBit(u8),
}

// -------------------------------------------------------------------------
//   Staging buffer
// -------------------------------------------------------------------------

/// Shared staging buffer for the "next command frame" being composed.
///
/// See the module-level documentation for the data flow between
/// writers and the RT scheduler.
pub struct CommandStaging {
    inner: Mutex<StagingInner>,
    cv: Condvar,
    /// Process-global default `WriteMode`. Can be changed at runtime
    /// via [`set_default_mode`]. Individual writes can still pass an
    /// explicit mode via the `*_with` variants of every setter.
    default_mode: AtomicU8,
}

impl std::fmt::Debug for CommandStaging {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Take a short lock to display current state.
        let g = self.inner.lock();
        f.debug_struct("CommandStaging")
            .field("default_mode", &self.default_mode())
            .field("write_generation", &g.write_generation)
            .field("sent_generation", &g.sent_generation)
            .field("dirty_bits", &format_args!("0x{:08x}", g.dirty_bits))
            .finish()
    }
}

#[derive(Debug)]
struct StagingInner {
    /// The frame currently being composed. Cloned into the snapshot
    /// passed to the scheduler at each tick.
    frame: Command,
    /// Per-field dirty bitmask. A bit set here means "a writer has
    /// set this field since the last `mark_sent`, and the value has
    /// not yet gone on the wire".
    dirty_bits: u32,
    /// Monotonic counter bumped on every successful write.
    write_generation: u64,
    /// Monotonic counter bumped on every `mark_sent`. Currently
    /// informational (used by tests and the RT stats module).
    sent_generation: u64,
}

impl CommandStaging {
    /// Create a new staging buffer with a default frame and the
    /// given global [`WriteMode`].
    pub fn new(default_mode: WriteMode) -> Self {
        Self {
            inner: Mutex::new(StagingInner {
                frame: Command::default(),
                dirty_bits: 0,
                write_generation: 0,
                sent_generation: 0,
            }),
            cv: Condvar::new(),
            default_mode: AtomicU8::new(default_mode as u8),
        }
    }

    /// Return the current process-global default [`WriteMode`].
    pub fn default_mode(&self) -> WriteMode {
        WriteMode::from_u8(self.default_mode.load(Ordering::Relaxed))
    }

    /// Change the process-global default [`WriteMode`]. Does not
    /// affect writes currently in progress.
    pub fn set_default_mode(&self, m: WriteMode) {
        self.default_mode.store(m as u8, Ordering::Relaxed);
    }

    // ---------------------------------------------------------------
    //   Generic write helper — implements the three modes once and
    //   for all. All setters go through this.
    // ---------------------------------------------------------------

    fn do_write<F>(&self, mode: WriteMode, mask: u32, apply: F) -> Result<(), StagingError>
    where
        F: FnOnce(&mut Command),
    {
        let mut g = self.inner.lock();
        match mode {
            WriteMode::Coalesce => {
                // Overwrite; no waiting.
            }
            WriteMode::BlockUntilSent => {
                // Wait until *our* specific dirty bits are clean.
                // `wait_while` handles the thundering-herd case: all
                // waiters are woken by `notify_all` from `mark_sent`,
                // each re-checks its own predicate under the lock.
                self.cv.wait_while(&mut g, |s| (s.dirty_bits & mask) != 0);
            }
            WriteMode::ErrorOnConflict => {
                if (g.dirty_bits & mask) != 0 {
                    return Err(StagingError::WouldOverwrite);
                }
            }
        }
        apply(&mut g.frame);
        g.dirty_bits |= mask;
        g.write_generation = g.write_generation.wrapping_add(1);
        Ok(())
    }

    // ---------------------------------------------------------------
    //   Analog outputs — DAC0 / DAC1 (12-bit, 0..=4095)
    // ---------------------------------------------------------------

    pub fn set_dac0(&self, value: u16) -> Result<(), StagingError> {
        self.set_dac0_with(self.default_mode(), value)
    }
    pub fn set_dac0_with(&self, mode: WriteMode, value: u16) -> Result<(), StagingError> {
        self.do_write(mode, DIRTY_DAC0, |c| c.dac[0] = value)
    }

    pub fn set_dac1(&self, value: u16) -> Result<(), StagingError> {
        self.set_dac1_with(self.default_mode(), value)
    }
    pub fn set_dac1_with(&self, mode: WriteMode, value: u16) -> Result<(), StagingError> {
        self.do_write(mode, DIRTY_DAC1, |c| c.dac[1] = value)
    }

    // ---------------------------------------------------------------
    //   PWM outputs — PWM0 / PWM1 (16-bit, 0..=65535)
    // ---------------------------------------------------------------

    pub fn set_pwm0(&self, value: u16) -> Result<(), StagingError> {
        self.set_pwm0_with(self.default_mode(), value)
    }
    pub fn set_pwm0_with(&self, mode: WriteMode, value: u16) -> Result<(), StagingError> {
        self.do_write(mode, DIRTY_PWM0, |c| c.pwm[0] = value)
    }

    pub fn set_pwm1(&self, value: u16) -> Result<(), StagingError> {
        self.set_pwm1_with(self.default_mode(), value)
    }
    pub fn set_pwm1_with(&self, mode: WriteMode, value: u16) -> Result<(), StagingError> {
        self.do_write(mode, DIRTY_PWM1, |c| c.pwm[1] = value)
    }

    // ---------------------------------------------------------------
    //   Command flags (adc/dac/pwm enable, reset_seq, watchdog)
    // ---------------------------------------------------------------

    pub fn set_flags(&self, flags: CommandFlags) -> Result<(), StagingError> {
        self.set_flags_with(self.default_mode(), flags)
    }
    pub fn set_flags_with(&self, mode: WriteMode, flags: CommandFlags) -> Result<(), StagingError> {
        self.do_write(mode, DIRTY_FLAGS, |c| c.flags = flags)
    }

    // ---------------------------------------------------------------
    //   Digital output — 16-bit word
    //
    //   Two APIs:
    //
    //     * `set_digital_out`       — overwrite the entire 16-bit
    //                                 word in one go. Marks *all*
    //                                 16 dirty bits. In BlockUntilSent
    //                                 mode blocks until the whole word
    //                                 is clean (i.e. no other writer
    //                                 has a pending bit).
    //
    //     * `set_digital_out_bit`   — toggle one specific bit.
    //                                 Marks only that bit dirty.
    //                                 Two writers on different bits
    //                                 never block each other.
    // ---------------------------------------------------------------

    pub fn set_digital_out(&self, mask: u16) -> Result<(), StagingError> {
        self.set_digital_out_with(self.default_mode(), mask)
    }
    pub fn set_digital_out_with(&self, mode: WriteMode, mask: u16) -> Result<(), StagingError> {
        self.do_write(mode, DIRTY_DIGITAL_OUT_ALL, |c| c.digital_out = mask)
    }

    pub fn set_digital_out_bit(&self, bit: u8, value: bool) -> Result<(), StagingError> {
        self.set_digital_out_bit_with(self.default_mode(), bit, value)
    }
    pub fn set_digital_out_bit_with(
        &self,
        mode: WriteMode,
        bit: u8,
        value: bool,
    ) -> Result<(), StagingError> {
        if bit >= 16 {
            return Err(StagingError::InvalidBit(bit));
        }
        let mask = dirty_digital_out_bit(bit);
        self.do_write(mode, mask, |c| {
            if value {
                c.digital_out |= 1u16 << bit;
            } else {
                c.digital_out &= !(1u16 << bit);
            }
        })
    }

    // ---------------------------------------------------------------
    //   Scheduler-side API
    // ---------------------------------------------------------------

    /// Take a snapshot of the current frame for transmission.
    ///
    /// The returned `write_generation` is the value observed under
    /// the same critical section as the frame copy; pass it back to
    /// [`mark_sent`] after the USB exchange has completed.
    ///
    /// This does *not* clear the dirty bitmask: dirty bits are
    /// cleared only by [`mark_sent`] so that any BlockUntilSent
    /// waiters remain blocked while the I/O is still in flight.
    pub fn take_snapshot(&self) -> (Command, u64) {
        let g = self.inner.lock();
        (g.frame.clone(), g.write_generation)
    }

    /// Signal that the frame whose `write_generation` was passed to
    /// the scheduler has now been written to the bus.
    ///
    /// Clears the dirty bitmask and wakes up any BlockUntilSent
    /// waiters. `write_generation` is currently informational; it is
    /// accepted to preserve API symmetry with [`take_snapshot`] and
    /// allow future extensions (e.g. detecting out-of-order mark_sent
    /// calls in a multi-scheduler configuration).
    pub fn mark_sent(&self, _write_generation: u64) {
        let mut g = self.inner.lock();
        g.dirty_bits = 0;
        g.sent_generation = g.sent_generation.wrapping_add(1);
        drop(g);
        self.cv.notify_all();
    }

    // ---------------------------------------------------------------
    //   Introspection (tests, telemetry, /api/status)
    // ---------------------------------------------------------------

    pub fn write_generation(&self) -> u64 {
        self.inner.lock().write_generation
    }
    pub fn sent_generation(&self) -> u64 {
        self.inner.lock().sent_generation
    }
    pub fn dirty_bits(&self) -> u32 {
        self.inner.lock().dirty_bits
    }
    /// Return a clone of the current staged frame without taking a
    /// snapshot. Useful for diagnostics; does not affect dirty state.
    pub fn peek_frame(&self) -> Command {
        self.inner.lock().frame.clone()
    }
}

// =========================================================================
//   Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn new_defaults_to_block_until_sent() {
        let s = CommandStaging::new(WriteMode::default());
        assert_eq!(s.default_mode(), WriteMode::BlockUntilSent);
    }

    #[test]
    fn set_default_mode_is_honored_by_next_write() {
        let s = CommandStaging::new(WriteMode::BlockUntilSent);
        s.set_default_mode(WriteMode::Coalesce);
        // Write in default (Coalesce) mode twice on same field:
        // no block, second overwrites first.
        s.set_dac0(100).unwrap();
        s.set_dac0(200).unwrap();
        let (frame, _gen) = s.take_snapshot();
        assert_eq!(frame.dac[0], 200);
    }

    #[test]
    fn coalesce_overwrites_without_blocking() {
        let s = CommandStaging::new(WriteMode::Coalesce);
        for v in 0..1000u16 {
            s.set_dac0(v).unwrap();
        }
        let (frame, _) = s.take_snapshot();
        assert_eq!(frame.dac[0], 999);
        // Still dirty — not yet sent.
        assert_ne!(s.dirty_bits() & DIRTY_DAC0, 0);
    }

    #[test]
    fn error_on_conflict_returns_error_when_dirty() {
        let s = CommandStaging::new(WriteMode::ErrorOnConflict);
        assert!(s.set_dac0(100).is_ok());
        let err = s.set_dac0(200).unwrap_err();
        assert_eq!(err, StagingError::WouldOverwrite);
        // Nothing was overwritten
        let (frame, _) = s.take_snapshot();
        assert_eq!(frame.dac[0], 100);
    }

    #[test]
    fn error_on_conflict_passes_once_mark_sent_is_called() {
        let s = CommandStaging::new(WriteMode::ErrorOnConflict);
        s.set_dac0(100).unwrap();
        let (_, gen) = s.take_snapshot();
        s.mark_sent(gen);
        assert!(s.set_dac0(200).is_ok());
    }

    #[test]
    fn take_snapshot_does_not_clear_dirty() {
        let s = CommandStaging::new(WriteMode::BlockUntilSent);
        s.set_dac0(100).unwrap();
        let before = s.dirty_bits();
        let (_frame, _) = s.take_snapshot();
        let after = s.dirty_bits();
        assert_eq!(before, after);
        assert_ne!(after & DIRTY_DAC0, 0);
    }

    #[test]
    fn mark_sent_clears_dirty_bits() {
        let s = CommandStaging::new(WriteMode::BlockUntilSent);
        s.set_dac0(100).unwrap();
        s.set_pwm1(42).unwrap();
        let (_, gen) = s.take_snapshot();
        assert_ne!(s.dirty_bits(), 0);
        s.mark_sent(gen);
        assert_eq!(s.dirty_bits(), 0);
    }

    #[test]
    fn block_until_sent_waits_same_field() {
        // Thread A sets dac0. No mark_sent. Thread B sets dac0 in
        // BlockUntilSent mode. Thread B should be blocked.
        let s = Arc::new(CommandStaging::new(WriteMode::BlockUntilSent));
        s.set_dac0(100).unwrap();

        let s2 = Arc::clone(&s);
        let handle = thread::spawn(move || {
            let t0 = Instant::now();
            s2.set_dac0(200).unwrap();
            t0.elapsed()
        });

        // Give the second thread some time to actually block inside
        // wait_while. 50 ms is plenty.
        thread::sleep(Duration::from_millis(50));
        assert!(!handle.is_finished(), "writer should still be blocked before mark_sent");

        // Now release the waiter.
        let (_, gen) = s.take_snapshot();
        s.mark_sent(gen);

        let elapsed = handle.join().unwrap();
        // The thread should have been blocked for ~50 ms, not
        // ~0 ms and not ~forever.
        assert!(
            elapsed >= Duration::from_millis(40),
            "writer finished too fast, didn't actually block ({}ms)",
            elapsed.as_millis()
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "writer blocked too long ({}ms) — is notify_all broken?",
            elapsed.as_millis()
        );

        let (frame, _) = s.take_snapshot();
        assert_eq!(frame.dac[0], 200);
    }

    #[test]
    fn block_until_sent_different_fields_do_not_block_each_other() {
        // Writer A dirties dac0; writer B in BlockUntilSent sets pwm0.
        // Writer B should NOT block — different field, different mask.
        let s = Arc::new(CommandStaging::new(WriteMode::BlockUntilSent));
        s.set_dac0(100).unwrap();

        let s2 = Arc::clone(&s);
        let handle = thread::spawn(move || {
            let t0 = Instant::now();
            s2.set_pwm0(42).unwrap();
            t0.elapsed()
        });
        let elapsed = handle.join().unwrap();
        assert!(
            elapsed < Duration::from_millis(50),
            "different-field write should not block ({}ms)",
            elapsed.as_millis()
        );

        let (frame, _) = s.take_snapshot();
        assert_eq!(frame.dac[0], 100);
        assert_eq!(frame.pwm[0], 42);
    }

    #[test]
    fn digital_out_bit_fine_grained_no_cross_blocking() {
        // Writer A dirties bit 3; writer B in BlockUntilSent sets
        // bit 7. Must not block.
        let s = Arc::new(CommandStaging::new(WriteMode::BlockUntilSent));
        s.set_digital_out_bit(3, true).unwrap();

        let s2 = Arc::clone(&s);
        let handle = thread::spawn(move || {
            let t0 = Instant::now();
            s2.set_digital_out_bit(7, true).unwrap();
            t0.elapsed()
        });
        let elapsed = handle.join().unwrap();
        assert!(
            elapsed < Duration::from_millis(50),
            "different-bit write should not block ({}ms)",
            elapsed.as_millis()
        );

        let (frame, _) = s.take_snapshot();
        assert_eq!(frame.digital_out & ((1 << 3) | (1 << 7)), (1 << 3) | (1 << 7));
    }

    #[test]
    fn digital_out_bit_same_bit_blocks() {
        // Writer A dirties bit 3; writer B in BlockUntilSent sets
        // bit 3. Must block until mark_sent.
        let s = Arc::new(CommandStaging::new(WriteMode::BlockUntilSent));
        s.set_digital_out_bit(3, true).unwrap();

        let s2 = Arc::clone(&s);
        let handle = thread::spawn(move || {
            let t0 = Instant::now();
            s2.set_digital_out_bit(3, false).unwrap();
            t0.elapsed()
        });
        thread::sleep(Duration::from_millis(50));
        assert!(!handle.is_finished(), "same-bit writer should block");

        let (_, gen) = s.take_snapshot();
        s.mark_sent(gen);
        let elapsed = handle.join().unwrap();
        assert!(elapsed >= Duration::from_millis(40));
    }

    #[test]
    fn mass_digital_out_blocks_on_any_dirty_bit() {
        // Writer A dirties bit 3; writer B in BlockUntilSent calls
        // set_digital_out (mass set). Must block because *some* bit
        // of the digital_out word is dirty.
        let s = Arc::new(CommandStaging::new(WriteMode::BlockUntilSent));
        s.set_digital_out_bit(3, true).unwrap();

        let s2 = Arc::clone(&s);
        let handle = thread::spawn(move || {
            let t0 = Instant::now();
            s2.set_digital_out(0xA5A5).unwrap();
            t0.elapsed()
        });
        thread::sleep(Duration::from_millis(50));
        assert!(!handle.is_finished(), "mass set should block");

        let (_, gen) = s.take_snapshot();
        s.mark_sent(gen);
        let elapsed = handle.join().unwrap();
        assert!(elapsed >= Duration::from_millis(40));

        let (frame, _) = s.take_snapshot();
        assert_eq!(frame.digital_out, 0xA5A5);
    }

    #[test]
    fn per_call_mode_override_beats_default() {
        // Default is BlockUntilSent; an explicit Coalesce override
        // on a dirty field should NOT block.
        let s = Arc::new(CommandStaging::new(WriteMode::BlockUntilSent));
        s.set_dac0(100).unwrap();

        let s2 = Arc::clone(&s);
        let handle = thread::spawn(move || {
            let t0 = Instant::now();
            s2.set_dac0_with(WriteMode::Coalesce, 200).unwrap();
            t0.elapsed()
        });
        let elapsed = handle.join().unwrap();
        assert!(
            elapsed < Duration::from_millis(50),
            "Coalesce override should not block ({}ms)",
            elapsed.as_millis()
        );

        let (frame, _) = s.take_snapshot();
        assert_eq!(frame.dac[0], 200);
    }

    #[test]
    fn per_call_mode_override_error_on_conflict() {
        let s = CommandStaging::new(WriteMode::Coalesce);
        s.set_dac0(100).unwrap();
        let err = s.set_dac0_with(WriteMode::ErrorOnConflict, 200).unwrap_err();
        assert_eq!(err, StagingError::WouldOverwrite);
    }

    #[test]
    fn invalid_digital_out_bit_rejected() {
        let s = CommandStaging::new(WriteMode::Coalesce);
        let err = s.set_digital_out_bit(16, true).unwrap_err();
        assert_eq!(err, StagingError::InvalidBit(16));
        let err = s.set_digital_out_bit(200, true).unwrap_err();
        assert_eq!(err, StagingError::InvalidBit(200));
    }

    #[test]
    fn write_generation_monotonic() {
        let s = CommandStaging::new(WriteMode::Coalesce);
        let g0 = s.write_generation();
        s.set_dac0(1).unwrap();
        let g1 = s.write_generation();
        s.set_pwm0(2).unwrap();
        let g2 = s.write_generation();
        assert!(g1 > g0);
        assert!(g2 > g1);
    }

    #[test]
    fn sent_generation_monotonic() {
        let s = CommandStaging::new(WriteMode::Coalesce);
        let g0 = s.sent_generation();
        s.set_dac0(1).unwrap();
        let (_, gen) = s.take_snapshot();
        s.mark_sent(gen);
        let g1 = s.sent_generation();
        assert_eq!(g1, g0 + 1);
    }

    #[test]
    fn snapshot_reflects_per_field_state() {
        let s = CommandStaging::new(WriteMode::Coalesce);
        s.set_dac0(100).unwrap();
        s.set_dac1(200).unwrap();
        s.set_pwm0(300).unwrap();
        s.set_pwm1(400).unwrap();
        s.set_digital_out(0xBEEF).unwrap();
        let mut cf = CommandFlags::default();
        cf.adc_enable = true;
        cf.dac_enable = true;
        s.set_flags(cf).unwrap();

        let (frame, _) = s.take_snapshot();
        assert_eq!(frame.dac[0], 100);
        assert_eq!(frame.dac[1], 200);
        assert_eq!(frame.pwm[0], 300);
        assert_eq!(frame.pwm[1], 400);
        assert_eq!(frame.digital_out, 0xBEEF);
        assert!(frame.flags.adc_enable);
        assert!(frame.flags.dac_enable);
    }
}
