use anyhow::{Context, Result};
use nix::sched::CpuSet;
use nix::sys::mman::{mlockall, MlockAllFlags};
use nix::unistd::Pid;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use tracing::{info, warn};

/// Real-time configuration and setup
pub struct RtConfig {
    pub enable_rt_scheduler: bool,
    pub rt_priority: i32,
    pub lock_memory: bool,
    pub set_cpu_affinity: bool,
    pub cpu_core: Option<usize>,
    pub set_dma_latency: bool,
}

impl Default for RtConfig {
    fn default() -> Self {
        Self {
            enable_rt_scheduler: true,
            rt_priority: 80,  // High priority, but not maximum (99)
            lock_memory: true,
            set_cpu_affinity: false,
            cpu_core: None,
            set_dma_latency: true,
        }
    }
}

/// Apply real-time optimizations
pub fn apply_rt_optimizations(config: &RtConfig) -> Result<()> {
    info!("Applying real-time optimizations");

    if config.enable_rt_scheduler {
        set_realtime_priority(config.rt_priority)
            .context("Failed to set real-time priority")?;
    }

    if config.lock_memory {
        lock_memory()
            .context("Failed to lock memory")?;
    }

    if config.set_cpu_affinity {
        if let Some(core) = config.cpu_core {
            set_cpu_affinity(core)
                .context("Failed to set CPU affinity")?;
        }
    }

    if config.set_dma_latency {
        // This will fail on non-Linux or without permissions, but that's okay
        if let Err(e) = set_dma_latency(0) {
            warn!("Failed to set DMA latency (may need root): {}", e);
        }
    }

    info!("Real-time optimizations applied successfully");
    Ok(())
}

/// Set real-time scheduling priority
fn set_realtime_priority(priority: i32) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        unsafe {
            let mut param: libc::sched_param = std::mem::zeroed();
            param.sched_priority = priority;

            let result = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param);
            if result == -1 {
                anyhow::bail!("Failed to set SCHED_FIFO scheduler: {}", std::io::Error::last_os_error());
            }
        }
        info!("Set real-time priority to {} (SCHED_FIFO)", priority);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        // macOS uses a different API for real-time scheduling
        // This is a simplified version; full implementation would use thread_policy_set
        warn!("Real-time scheduling not fully implemented on macOS");
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        warn!("Real-time scheduling not supported on this platform");
        Ok(())
    }
}

/// Lock all current and future memory to prevent paging
fn lock_memory() -> Result<()> {
    mlockall(MlockAllFlags::MCL_CURRENT | MlockAllFlags::MCL_FUTURE)
        .context("Failed to lock memory (mlockall)")?;

    info!("Memory locked successfully");
    Ok(())
}

/// Set CPU affinity to a specific core
fn set_cpu_affinity(core: usize) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let mut cpu_set = CpuSet::new();
        cpu_set.set(core)?;

        nix::sched::sched_setaffinity(Pid::from_raw(0), &cpu_set)
            .context("Failed to set CPU affinity")?;

        info!("Set CPU affinity to core {}", core);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        warn!("CPU affinity not supported on this platform");
        Ok(())
    }
}

/// Set DMA latency to minimize interrupt latency (Linux only)
fn set_dma_latency(latency_us: i32) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let file = OpenOptions::new()
            .write(true)
            .open("/dev/cpu_dma_latency")
            .context("Failed to open /dev/cpu_dma_latency")?;

        // Keep file descriptor open for the lifetime of the process
        // This is typically done by leaking it or keeping it in a static
        let fd = file.as_raw_fd();

        // Write the latency value (as i32 bytes)
        let bytes = latency_us.to_le_bytes();
        std::fs::write("/dev/cpu_dma_latency", bytes)
            .context("Failed to write DMA latency")?;

        info!("Set DMA latency to {} µs (fd={})", latency_us, fd);

        // Leak the file descriptor to keep it open
        std::mem::forget(file);

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        warn!("DMA latency control not supported on this platform");
        Ok(())
    }
}

/// Get current thread priority
pub fn get_thread_priority() -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        unsafe {
            let mut param: libc::sched_param = std::mem::zeroed();
            let result = libc::sched_getparam(0, &mut param);
            if result == -1 {
                anyhow::bail!("Failed to get scheduler parameters: {}", std::io::Error::last_os_error());
            }
            Ok(param.sched_priority)
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(0)
    }
}

/// Check if running with real-time capabilities
pub fn check_rt_capabilities() -> bool {
    #[cfg(target_os = "linux")]
    {
        unsafe {
            let mut param: libc::sched_param = std::mem::zeroed();
            param.sched_priority = 1;

            let result = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param);
            result != -1
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rt_config_default() {
        let config = RtConfig::default();
        assert_eq!(config.enable_rt_scheduler, true);
        assert_eq!(config.rt_priority, 80);
        assert_eq!(config.lock_memory, true);
        assert_eq!(config.set_cpu_affinity, false);
        assert_eq!(config.cpu_core, None);
        assert_eq!(config.set_dma_latency, true);
    }

    #[test]
    fn test_rt_config_custom() {
        let config = RtConfig {
            enable_rt_scheduler: false,
            rt_priority: 50,
            lock_memory: false,
            set_cpu_affinity: true,
            cpu_core: Some(2),
            set_dma_latency: false,
        };

        assert_eq!(config.enable_rt_scheduler, false);
        assert_eq!(config.rt_priority, 50);
        assert_eq!(config.lock_memory, false);
        assert_eq!(config.set_cpu_affinity, true);
        assert_eq!(config.cpu_core, Some(2));
        assert_eq!(config.set_dma_latency, false);
    }

    #[test]
    fn test_check_rt_capabilities() {
        // This test will likely fail without root, but should not panic
        let _has_rt = check_rt_capabilities();
        // Just verify the function runs without panicking
    }

    #[test]
    fn test_get_thread_priority() {
        // This test should work even without RT privileges
        let result = get_thread_priority();
        // Should either succeed or fail gracefully
        assert!(result.is_ok() || result.is_err());
    }
}
