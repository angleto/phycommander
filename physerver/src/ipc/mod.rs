use crate::protocol::{Command, Status};
use anyhow::{Context, Result};
use shared_memory::{Shmem, ShmemConf};
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};
use tracing::{info, warn};

/// Shared memory layout for IPC
/// This structure is shared between physerver and client applications
#[repr(C)]
pub struct SharedState {
    // Metadata
    pub version: AtomicU32,
    pub command_seq: AtomicU64,  // Incremented when command changes
    pub status_seq: AtomicU64,   // Incremented when status updates

    // Command data (from client to server to device)
    pub cmd_digital_out: AtomicU16,
    pub cmd_dac0: AtomicU16,
    pub cmd_dac1: AtomicU16,
    pub cmd_pwm0: AtomicU16,
    pub cmd_pwm1: AtomicU16,
    pub cmd_flags: AtomicU8,

    // Status data (from device to server to client)
    pub status_digital_in: AtomicU16,
    pub status_digital_out: AtomicU16,
    pub status_adc0: AtomicU16,
    pub status_adc1: AtomicU16,
    pub status_adc2: AtomicU16,
    pub status_adc3: AtomicU16,
    pub status_adc4: AtomicU16,
    pub status_adc5: AtomicU16,
    pub status_adc6: AtomicU16,
    pub status_adc7: AtomicU16,
    pub status_flags: AtomicU8,
    pub status_seq_num: AtomicU8,
    pub status_loop_time_us: AtomicU16,
    pub status_uptime_ms: AtomicU32,
    pub status_error_count: AtomicU16,
}

const SHARED_MEM_NAME: &str = "phycmd_state";
const SHARED_MEM_SIZE: usize = std::mem::size_of::<SharedState>();
const PROTOCOL_VERSION: u32 = 1;

pub struct IpcServer {
    shmem: Shmem,
}

impl IpcServer {
    /// Create a new IPC server (physerver side)
    pub fn new() -> Result<Self> {
        info!("Creating shared memory segment: {}", SHARED_MEM_NAME);

        let shmem = ShmemConf::new()
            .size(SHARED_MEM_SIZE)
            .os_id(SHARED_MEM_NAME)
            .create()
            .context("Failed to create shared memory")?;

        info!("Shared memory created successfully ({} bytes)", SHARED_MEM_SIZE);

        // Initialize the shared state
        let state = unsafe { &*(shmem.as_ptr() as *const SharedState) };
        state.version.store(PROTOCOL_VERSION, Ordering::Release);
        state.command_seq.store(0, Ordering::Release);
        state.status_seq.store(0, Ordering::Release);

        Ok(Self { shmem })
    }

    /// Get reference to shared state
    fn state(&self) -> &SharedState {
        unsafe { &*(self.shmem.as_ptr() as *const SharedState) }
    }

    /// Read command from shared memory
    pub fn read_command(&self) -> Command {
        let state = self.state();

        Command {
            digital_out: state.cmd_digital_out.load(Ordering::Acquire),
            dac: [
                state.cmd_dac0.load(Ordering::Acquire),
                state.cmd_dac1.load(Ordering::Acquire),
            ],
            pwm: [
                state.cmd_pwm0.load(Ordering::Acquire),
                state.cmd_pwm1.load(Ordering::Acquire),
            ],
            flags: crate::protocol::CommandFlags::from_byte(
                state.cmd_flags.load(Ordering::Acquire)
            ),
            seq_num: 0, // Will be set by serial handler
        }
    }

    /// Write status to shared memory
    pub fn write_status(&self, status: &Status) {
        let state = self.state();

        state.status_digital_in.store(status.digital_in, Ordering::Release);
        state.status_digital_out.store(status.digital_out, Ordering::Release);

        for (i, &adc_val) in status.adc.iter().enumerate() {
            match i {
                0 => state.status_adc0.store(adc_val, Ordering::Release),
                1 => state.status_adc1.store(adc_val, Ordering::Release),
                2 => state.status_adc2.store(adc_val, Ordering::Release),
                3 => state.status_adc3.store(adc_val, Ordering::Release),
                4 => state.status_adc4.store(adc_val, Ordering::Release),
                5 => state.status_adc5.store(adc_val, Ordering::Release),
                6 => state.status_adc6.store(adc_val, Ordering::Release),
                7 => state.status_adc7.store(adc_val, Ordering::Release),
                _ => {}
            }
        }

        state.status_flags.store(status.flags.to_byte(), Ordering::Release);
        state.status_seq_num.store(status.seq_num, Ordering::Release);
        state.status_loop_time_us.store(status.loop_time_us, Ordering::Release);
        state.status_uptime_ms.store(status.uptime_ms, Ordering::Release);
        state.status_error_count.store(status.error_count, Ordering::Release);

        // Increment status sequence to notify clients
        state.status_seq.fetch_add(1, Ordering::Release);
    }

    /// Check if command has been updated by client
    pub fn command_updated(&self) -> bool {
        let seq = self.state().command_seq.load(Ordering::Acquire);
        // This is simplified; real implementation would track last seen sequence
        seq > 0
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        info!("Cleaning up shared memory");
    }
}

/// IPC Client (for test applications)
pub struct IpcClient {
    shmem: Shmem,
}

impl IpcClient {
    /// Connect to existing IPC server
    pub fn connect() -> Result<Self> {
        info!("Connecting to shared memory: {}", SHARED_MEM_NAME);

        let shmem = ShmemConf::new()
            .os_id(SHARED_MEM_NAME)
            .open()
            .context("Failed to open shared memory. Is physerver running?")?;

        // Verify version
        let state = unsafe { &*(shmem.as_ptr() as *const SharedState) };
        let version = state.version.load(Ordering::Acquire);

        if version != PROTOCOL_VERSION {
            warn!("Protocol version mismatch: expected {}, got {}", PROTOCOL_VERSION, version);
        }

        info!("Connected to shared memory successfully");

        Ok(Self { shmem })
    }

    /// Get reference to shared state
    fn state(&self) -> &SharedState {
        unsafe { &*(self.shmem.as_ptr() as *const SharedState) }
    }

    /// Write command to shared memory
    pub fn write_command(&self, cmd: &Command) {
        let state = self.state();

        state.cmd_digital_out.store(cmd.digital_out, Ordering::Release);
        state.cmd_dac0.store(cmd.dac[0], Ordering::Release);
        state.cmd_dac1.store(cmd.dac[1], Ordering::Release);
        state.cmd_pwm0.store(cmd.pwm[0], Ordering::Release);
        state.cmd_pwm1.store(cmd.pwm[1], Ordering::Release);
        state.cmd_flags.store(cmd.flags.to_byte(), Ordering::Release);

        // Increment sequence to notify server
        state.command_seq.fetch_add(1, Ordering::Release);
    }

    /// Read status from shared memory
    pub fn read_status(&self) -> Status {
        let state = self.state();

        Status {
            digital_in: state.status_digital_in.load(Ordering::Acquire),
            digital_out: state.status_digital_out.load(Ordering::Acquire),
            adc: [
                state.status_adc0.load(Ordering::Acquire),
                state.status_adc1.load(Ordering::Acquire),
                state.status_adc2.load(Ordering::Acquire),
                state.status_adc3.load(Ordering::Acquire),
                state.status_adc4.load(Ordering::Acquire),
                state.status_adc5.load(Ordering::Acquire),
                state.status_adc6.load(Ordering::Acquire),
                state.status_adc7.load(Ordering::Acquire),
            ],
            flags: crate::protocol::StatusFlags::from_byte(
                state.status_flags.load(Ordering::Acquire)
            ),
            seq_num: state.status_seq_num.load(Ordering::Acquire),
            loop_time_us: state.status_loop_time_us.load(Ordering::Acquire),
            uptime_ms: state.status_uptime_ms.load(Ordering::Acquire),
            error_count: state.status_error_count.load(Ordering::Acquire),
        }
    }

    /// Wait for status update
    pub fn wait_status_update(&self, timeout: std::time::Duration) -> Result<Status> {
        let start = std::time::Instant::now();
        let initial_seq = self.state().status_seq.load(Ordering::Acquire);

        loop {
            let current_seq = self.state().status_seq.load(Ordering::Acquire);
            if current_seq != initial_seq {
                return Ok(self.read_status());
            }

            if start.elapsed() > timeout {
                anyhow::bail!("Timeout waiting for status update");
            }

            std::thread::sleep(std::time::Duration::from_micros(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_state_size() {
        // Ensure SharedState has a reasonable size
        let size = std::mem::size_of::<SharedState>();
        assert!(size > 0);
        assert!(size < 4096); // Should fit in a page
    }

    #[test]
    fn test_constants() {
        assert_eq!(SHARED_MEM_NAME, "phycmd_state");
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    // Note: IPC roundtrip tests are commented out as they require
    // system resources (shared memory) that may conflict with running server
    // These should be tested in integration tests instead

    /*
    #[test]
    fn test_ipc_server_roundtrip() -> Result<()> {
        // This test requires shared memory which may conflict with running server
        // Test manually or use integration tests
        Ok(())
    }

    #[test]
    fn test_ipc_command_roundtrip() -> Result<()> {
        // This test requires shared memory which may conflict with running server
        // Test manually or use integration tests
        Ok(())
    }
    */
}
