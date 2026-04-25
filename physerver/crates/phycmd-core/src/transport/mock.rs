//! `MockTransport` — an in-memory [`Transport`] impl used by unit
//! tests to exercise the RT scheduler without actual USB hardware.
//!
//! The mock echoes command fields into the corresponding status
//! fields:
//!
//!   * `command.digital_out` → `status.digital_out` (echo)
//!   * `command.dac[0]` → `status.adc[0]` (simulates a loopback)
//!   * `command.dac[1]` → `status.adc[1]`
//!   * `command.seq_num` → `status.seq_num`
//!   * sets `status.flags.usb_configured = true`
//!
//! A few knobs are exposed to make testing of edge cases easier:
//!
//!   * `latency` — how long each `exchange` sleeps before returning. Lets a test verify tick
//!     slippage and missed-tick accounting with deterministic timing.
//!
//!   * `fail_every_nth` — if set to `Some(N)`, every Nth exchange returns `Err` instead of a valid
//!     status. Useful to exercise the scheduler's error path.
//!
//!   * `observed_commands` / `observed_responses` — recorded history for post-hoc assertions in
//!     tests.
//!
//! The mock is only compiled when `cfg(test)` is set for the
//! `phycmd-core` crate or when the downstream consumer opts in via
//! `features = ["test-mock"]` (so that integration tests in sibling
//! crates can reuse it). Both paths are wired up in `Cargo.toml`.

use std::{fmt, sync::Arc, thread::sleep, time::Duration};

use anyhow::Result;
use parking_lot::Mutex;

use super::traits::{Transport, TransportStats};
use crate::protocol::{Command, Status, StatusFlags};

/// Shared configuration and recorded state for [`MockTransport`].
///
/// Wrapped in `Arc<Mutex<...>>` inside the struct so tests can grab
/// a handle to the same inner state and inspect it while the
/// scheduler is running in another thread.
#[derive(Debug)]
pub struct MockState {
    /// How long each `exchange` should pretend to take.
    pub latency: Duration,
    /// If `Some(n)`, every n-th call to `exchange` fails.
    pub fail_every_nth: Option<u64>,
    /// Recorded commands (in order they were sent).
    pub observed_commands: Vec<Command>,
    /// Recorded responses (in order they were produced).
    pub observed_responses: Vec<Status>,
    /// Global call counter (incremented on every `exchange`, even
    /// failed ones).
    pub call_count: u64,
}

impl Default for MockState {
    fn default() -> Self {
        Self {
            latency: Duration::from_micros(50),
            fail_every_nth: None,
            observed_commands: Vec::new(),
            observed_responses: Vec::new(),
            call_count: 0,
        }
    }
}

/// In-memory [`Transport`] for unit tests.
pub struct MockTransport {
    state: Arc<Mutex<MockState>>,
    stats: TransportStats,
    device_id: String,
}

impl fmt::Debug for MockTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockTransport").field("device_id", &self.device_id).finish()
    }
}

impl MockTransport {
    /// Create a fresh mock with default state.
    pub fn new() -> Self {
        Self::with_state(Arc::new(Mutex::new(MockState::default())))
    }

    /// Create a mock that shares its state with a caller-supplied
    /// `Arc`. This is how tests inspect/modify the mock while it is
    /// being driven by the scheduler thread.
    pub fn with_state(state: Arc<Mutex<MockState>>) -> Self {
        Self { state, stats: TransportStats::default(), device_id: "mock".to_string() }
    }

    /// Handle to the shared mock state.
    pub fn state(&self) -> Arc<Mutex<MockState>> {
        Arc::clone(&self.state)
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for MockTransport {
    fn send_command(&mut self, cmd: &Command) -> Result<()> {
        // send_command alone is never used by the RT scheduler —
        // it goes through `exchange`. But the trait requires it,
        // so we implement it as "record without responding".
        let mut s = self.state.lock();
        s.observed_commands.push(cmd.clone());
        self.stats.messages_sent += 1;
        Ok(())
    }

    fn receive_status(&mut self) -> Result<Status> {
        // The scheduler uses `exchange` instead; if anything calls
        // this it gets a default status.
        let s = Status::default();
        self.stats.messages_received += 1;
        Ok(s)
    }

    fn exchange(&mut self, cmd: &Command) -> Result<Status> {
        // Capture the latency and fail config under the lock, then
        // release it before sleeping so tests that inspect the
        // state concurrently don't block on the sleep.
        let (latency, fail_every_nth, this_call) = {
            let mut s = self.state.lock();
            s.call_count += 1;
            (s.latency, s.fail_every_nth, s.call_count)
        };

        if latency > Duration::ZERO {
            sleep(latency);
        }

        // Check fail injection
        if let Some(n) = fail_every_nth {
            if n > 0 && this_call % n == 0 {
                self.stats.errors += 1;
                return Err(anyhow::anyhow!(
                    "MockTransport injected failure on call {}",
                    this_call
                ));
            }
        }

        // Build the response by echoing the command.
        let status = Status {
            digital_in: cmd.digital_out, // loopback
            digital_out: cmd.digital_out,
            adc: [cmd.dac[0], cmd.dac[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            flags: StatusFlags { usb_configured: true, ..Default::default() },
            seq_num: cmd.seq_num,
            loop_time_us: 10, // pretend the device loop is fast
            uptime_ms: 0,
            error_count: 0,
        };

        // Record and update stats.
        {
            let mut s = self.state.lock();
            s.observed_commands.push(cmd.clone());
            s.observed_responses.push(status.clone());
        }
        self.stats.messages_sent += 1;
        self.stats.messages_received += 1;

        Ok(status)
    }

    fn stats(&self) -> &TransportStats {
        &self.stats
    }

    fn reset_stats(&mut self) {
        self.stats = TransportStats::default();
    }

    fn name(&self) -> &str {
        "MockTransport"
    }

    fn device_id(&self) -> String {
        self.device_id.clone()
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn set_timeout(&mut self, _timeout: Duration) -> Result<()> {
        Ok(())
    }

    fn max_rate(&self) -> u32 {
        // For tests: pretend we can do 100 kHz so the scheduler
        // never vetoes the requested rate on mock inputs.
        100_000
    }

    fn typical_latency_us(&self) -> u32 {
        10
    }
}

// =========================================================================
//   MockPipelinedTransport — for testing the pipelined scheduler loop
// =========================================================================

/// A [`PipelinedTransport`] backed by the same mock echo logic as
/// [`MockTransport`]. `submit()` stores the command; `reap()` sleeps
/// for the configured latency and returns the echoed Status.
pub struct MockPipelinedTransport {
    state: Arc<Mutex<MockState>>,
    stats: TransportStats,
    pending_cmd: Option<Command>,
}

impl fmt::Debug for MockPipelinedTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockPipelinedTransport")
            .field("in_flight", &self.pending_cmd.is_some())
            .finish()
    }
}

impl MockPipelinedTransport {
    pub fn new() -> Self {
        Self::with_state(Arc::new(Mutex::new(MockState::default())))
    }

    pub fn with_state(state: Arc<Mutex<MockState>>) -> Self {
        Self { state, stats: TransportStats::default(), pending_cmd: None }
    }

    pub fn state(&self) -> Arc<Mutex<MockState>> {
        Arc::clone(&self.state)
    }
}

impl Default for MockPipelinedTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl super::traits::PipelinedTransport for MockPipelinedTransport {
    fn submit(&mut self, cmd: &Command) -> anyhow::Result<()> {
        if self.pending_cmd.is_some() {
            anyhow::bail!("submit with transfer already in flight");
        }
        self.pending_cmd = Some(cmd.clone());
        Ok(())
    }

    fn reap(&mut self, _timeout: Duration) -> anyhow::Result<Status> {
        let cmd = self
            .pending_cmd
            .take()
            .ok_or_else(|| anyhow::anyhow!("reap with no in-flight transfer"))?;

        // Simulate latency + failure injection (same logic as MockTransport)
        let (latency, fail_every_nth, call_count) = {
            let mut s = self.state.lock();
            s.call_count += 1;
            (s.latency, s.fail_every_nth, s.call_count)
        };

        if latency > Duration::ZERO {
            sleep(latency);
        }

        if let Some(n) = fail_every_nth {
            if n > 0 && call_count % n == 0 {
                self.stats.errors += 1;
                return Err(anyhow::anyhow!(
                    "MockPipelinedTransport injected failure on call {}",
                    call_count
                ));
            }
        }

        let status = Status {
            digital_in: cmd.digital_out,
            digital_out: cmd.digital_out,
            adc: [cmd.dac[0], cmd.dac[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            flags: StatusFlags { usb_configured: true, ..Default::default() },
            seq_num: cmd.seq_num,
            loop_time_us: 10,
            uptime_ms: 0,
            error_count: 0,
        };

        {
            let mut s = self.state.lock();
            s.observed_commands.push(cmd);
            s.observed_responses.push(status.clone());
        }
        self.stats.messages_sent += 1;
        self.stats.messages_received += 1;

        Ok(status)
    }

    fn drain(&mut self, timeout: Duration) -> anyhow::Result<Option<Status>> {
        if self.pending_cmd.is_none() {
            return Ok(None);
        }
        self.reap(timeout).map(Some)
    }

    fn stats(&self) -> &TransportStats {
        &self.stats
    }

    fn reset_stats(&mut self) {
        self.stats = TransportStats::default();
    }

    fn name(&self) -> &str {
        "MockPipelined"
    }

    fn device_id(&self) -> String {
        "mock-pipelined".to_string()
    }

    fn is_connected(&self) -> bool {
        true
    }
}

// =========================================================================
//   Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Command, CommandFlags};

    fn make_cmd(dac0: u16, dac1: u16, digital: u16, seq: u8) -> Command {
        Command {
            digital_out: digital,
            dac: [dac0, dac1],
            pwm: [0, 0],
            flags: CommandFlags::default(),
            seq_num: seq,
        }
    }

    #[test]
    fn echo_fields_as_expected() {
        let mut t = MockTransport::new();
        let cmd = make_cmd(0x1234, 0x5678, 0xBEEF, 42);
        let status = t.exchange(&cmd).unwrap();
        assert_eq!(status.digital_out, 0xBEEF);
        assert_eq!(status.digital_in, 0xBEEF);
        assert_eq!(status.adc[0], 0x1234);
        assert_eq!(status.adc[1], 0x5678);
        assert_eq!(status.seq_num, 42);
        assert!(status.flags.usb_configured);
    }

    #[test]
    fn observed_commands_and_responses_recorded() {
        let mut t = MockTransport::new();
        for i in 0..5 {
            t.exchange(&make_cmd(i, 0, 0, i as u8)).unwrap();
        }
        let state = t.state();
        let s = state.lock();
        assert_eq!(s.observed_commands.len(), 5);
        assert_eq!(s.observed_responses.len(), 5);
        assert_eq!(s.call_count, 5);
        assert_eq!(s.observed_commands[4].dac[0], 4);
    }

    #[test]
    fn fail_every_nth_injects_error() {
        let state = Arc::new(Mutex::new(MockState {
            latency: Duration::ZERO,
            fail_every_nth: Some(3),
            ..Default::default()
        }));
        let mut t = MockTransport::with_state(Arc::clone(&state));
        let r1 = t.exchange(&make_cmd(0, 0, 0, 0));
        let r2 = t.exchange(&make_cmd(0, 0, 0, 0));
        let r3 = t.exchange(&make_cmd(0, 0, 0, 0));
        let r4 = t.exchange(&make_cmd(0, 0, 0, 0));
        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert!(r3.is_err(), "3rd call should fail");
        assert!(r4.is_ok());
    }

    #[test]
    fn latency_is_honored() {
        let state = Arc::new(Mutex::new(MockState {
            latency: Duration::from_millis(5),
            ..Default::default()
        }));
        let mut t = MockTransport::with_state(Arc::clone(&state));
        let t0 = std::time::Instant::now();
        t.exchange(&make_cmd(0, 0, 0, 0)).unwrap();
        let elapsed = t0.elapsed();
        assert!(
            elapsed >= Duration::from_millis(4),
            "mock latency not applied ({}ms)",
            elapsed.as_millis()
        );
    }

    #[test]
    fn zero_latency_is_fast() {
        let state =
            Arc::new(Mutex::new(MockState { latency: Duration::ZERO, ..Default::default() }));
        let mut t = MockTransport::with_state(state);
        let t0 = std::time::Instant::now();
        for _ in 0..1000 {
            t.exchange(&make_cmd(0, 0, 0, 0)).unwrap();
        }
        let elapsed = t0.elapsed();
        assert!(
            elapsed.as_millis() < 100,
            "zero-latency mock is too slow: {}ms for 1000 exchanges",
            elapsed.as_millis()
        );
    }

    #[test]
    fn stats_are_updated() {
        let mut t = MockTransport::new();
        for _ in 0..5 {
            t.exchange(&make_cmd(0, 0, 0, 0)).unwrap();
        }
        let s = t.stats();
        assert_eq!(s.messages_sent, 5);
        assert_eq!(s.messages_received, 5);
        assert_eq!(s.errors, 0);
    }

    #[test]
    fn shared_state_between_transport_and_test() {
        let state = Arc::new(Mutex::new(MockState::default()));
        let mut t = MockTransport::with_state(Arc::clone(&state));
        t.exchange(&make_cmd(0xAA, 0xBB, 0xCC, 0xDD)).unwrap();
        // Test can read the same state without going through `t`
        let s = state.lock();
        assert_eq!(s.observed_commands[0].dac[0], 0xAA);
    }
}
