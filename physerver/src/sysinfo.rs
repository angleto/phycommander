//! System hardware telemetry.
//!
//! Reads motherboard sensors from `/sys/class/hwmon` (temperatures, voltages,
//! fans, currents, powers) and basic system metrics (CPU frequency/governor,
//! load average, memory, uptime) from `/proc` and `/sys`.
//!
//! The reader is generic: any hwmon chip exposed by the kernel is captured
//! automatically. On this project's DN2800MT test host that means `coretemp`
//! and `drivetemp`; on a board with a Super I/O or RAPL it would add those
//! without any code change.
//!
//! All reads are best-effort and non-panicking: if sysfs is not present
//! (e.g. on macOS during local development) the snapshot has `available =
//! false` and empty vectors, and the HTTP handler returns it as-is.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

/// A full snapshot of system hardware telemetry at a given instant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SysInfoSnapshot {
    /// `false` when no sysfs data could be read (e.g. non-Linux host).
    pub available: bool,
    /// Wall-clock milliseconds since UNIX epoch when the snapshot was taken.
    pub timestamp_ms: u64,
    /// All hwmon chips discovered under `/sys/class/hwmon`.
    pub hwmon: Vec<HwmonChip>,
    /// Per-CPU frequency and governor.
    pub cpus: Vec<CpuInfo>,
    /// `/proc/loadavg` parsed values.
    pub load_avg: Option<LoadAvg>,
    /// `/proc/meminfo` parsed values (in kB as the kernel reports them).
    pub memory: Option<MemoryInfo>,
    /// System uptime in seconds from `/proc/uptime`.
    pub uptime_seconds: Option<f64>,
}

/// A single hwmon device (e.g. `coretemp`, `drivetemp`, `nct6775`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwmonChip {
    pub name: String,
    pub temperatures: Vec<SensorReading>,
    pub voltages: Vec<SensorReading>,
    pub fans: Vec<SensorReading>,
    pub currents: Vec<SensorReading>,
    pub powers: Vec<SensorReading>,
}

/// A single sensor reading already converted to its natural unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    pub label: String,
    pub value: f64,
    pub unit: String,
}

/// Per-CPU state pulled from `cpufreq`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub cpu: u32,
    pub freq_mhz: f64,
    pub governor: String,
}

/// Load averages over 1, 5 and 15 minutes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAvg {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

/// Memory usage in kB (kernel-native units from `/proc/meminfo`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_kb: u64,
    pub available_kb: u64,
    pub free_kb: u64,
    pub buffers_kb: u64,
    pub cached_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
}

/// Read a fresh snapshot from the real root filesystem.
///
/// This performs a handful of small file reads (a few dozen at most) and is
/// safe to call from an async HTTP handler without `spawn_blocking`.
pub fn read_snapshot() -> SysInfoSnapshot {
    read_snapshot_from(Path::new("/"))
}

/// Read a snapshot rooted at `root`. Exposed for unit tests that provide a
/// fake sysfs/procfs tree under a temp directory.
pub fn read_snapshot_from(root: &Path) -> SysInfoSnapshot {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let hwmon_root = root.join("sys/class/hwmon");
    let hwmon = read_hwmon_chips(&hwmon_root).unwrap_or_default();

    let cpu_root = root.join("sys/devices/system/cpu");
    let cpus = read_cpus(&cpu_root).unwrap_or_default();

    let load_avg = read_load_avg(&root.join("proc/loadavg")).ok();
    let memory = read_memory(&root.join("proc/meminfo")).ok();
    let uptime_seconds = read_uptime(&root.join("proc/uptime")).ok();

    let available = !hwmon.is_empty()
        || !cpus.is_empty()
        || load_avg.is_some()
        || memory.is_some()
        || uptime_seconds.is_some();

    SysInfoSnapshot { available, timestamp_ms, hwmon, cpus, load_avg, memory, uptime_seconds }
}

// --- hwmon ------------------------------------------------------------------

fn read_hwmon_chips(hwmon_root: &Path) -> std::io::Result<Vec<HwmonChip>> {
    if !hwmon_root.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<PathBuf> = fs::read_dir(hwmon_root)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Deterministic order: hwmon0, hwmon1, ...
    entries.sort();

    let mut chips = Vec::new();
    for path in entries {
        let name = match fs::read_to_string(path.join("name")) {
            Ok(n) => n.trim().to_string(),
            Err(_) => continue,
        };

        let chip = HwmonChip {
            name,
            temperatures: collect_sensors(&path, "temp", 0.001, "°C"),
            voltages: collect_sensors(&path, "in", 0.001, "V"),
            fans: collect_sensors(&path, "fan", 1.0, "RPM"),
            currents: collect_sensors(&path, "curr", 0.001, "A"),
            powers: collect_sensors(&path, "power", 0.000_001, "W"),
        };
        chips.push(chip);
    }

    Ok(chips)
}

/// Scan a single hwmon chip directory for sensors of the given category
/// (`temp`, `in`, `fan`, `curr`, `power`), apply `scale` to the raw kernel
/// value and tag the result with `unit`.
fn collect_sensors(chip_path: &Path, category: &str, scale: f64, unit: &str) -> Vec<SensorReading> {
    let entries = match fs::read_dir(chip_path) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    // Collect all indices N such that `{category}{N}_input` exists.
    let mut indices: Vec<u32> = entries
        .flatten()
        .filter_map(|e| {
            let fname = e.file_name();
            let s = fname.to_str()?;
            let rest = s.strip_prefix(category)?;
            let idx_str = rest.strip_suffix("_input")?;
            idx_str.parse::<u32>().ok()
        })
        .collect();
    indices.sort_unstable();

    indices
        .into_iter()
        .filter_map(|idx| {
            let raw =
                fs::read_to_string(chip_path.join(format!("{}{}_input", category, idx))).ok()?;
            let raw_val: f64 = raw.trim().parse().ok()?;

            let label = fs::read_to_string(chip_path.join(format!("{}{}_label", category, idx)))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("{}{}", category, idx));

            Some(SensorReading { label, value: raw_val * scale, unit: unit.to_string() })
        })
        .collect()
}

// --- cpufreq ----------------------------------------------------------------

fn read_cpus(cpu_root: &Path) -> std::io::Result<Vec<CpuInfo>> {
    if !cpu_root.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<(u32, PathBuf)> = fs::read_dir(cpu_root)?
        .flatten()
        .filter_map(|e| {
            let fname = e.file_name();
            let s = fname.to_str()?;
            let idx_str = s.strip_prefix("cpu")?;
            let idx: u32 = idx_str.parse().ok()?;
            Some((idx, e.path()))
        })
        .collect();
    entries.sort_by_key(|(idx, _)| *idx);

    let mut cpus = Vec::new();
    for (cpu, path) in entries {
        let cpufreq = path.join("cpufreq");
        if !cpufreq.exists() {
            continue;
        }
        let Ok(freq_raw) = fs::read_to_string(cpufreq.join("scaling_cur_freq")) else {
            continue;
        };
        let Ok(freq_khz) = freq_raw.trim().parse::<u64>() else {
            continue;
        };
        let governor = fs::read_to_string(cpufreq.join("scaling_governor"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| String::from("unknown"));

        cpus.push(CpuInfo { cpu, freq_mhz: freq_khz as f64 / 1000.0, governor });
    }

    Ok(cpus)
}

// --- /proc ------------------------------------------------------------------

fn read_load_avg(path: &Path) -> std::io::Result<LoadAvg> {
    let content = fs::read_to_string(path)?;
    let parts: Vec<&str> = content.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "malformed /proc/loadavg",
        ));
    }
    Ok(LoadAvg {
        one: parts[0].parse().unwrap_or(0.0),
        five: parts[1].parse().unwrap_or(0.0),
        fifteen: parts[2].parse().unwrap_or(0.0),
    })
}

fn read_memory(path: &Path) -> std::io::Result<MemoryInfo> {
    let content = fs::read_to_string(path)?;
    let mut mi = MemoryInfo {
        total_kb: 0,
        available_kb: 0,
        free_kb: 0,
        buffers_kb: 0,
        cached_kb: 0,
        swap_total_kb: 0,
        swap_free_kb: 0,
    };

    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else { continue };
        let val: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        match key {
            "MemTotal:" => mi.total_kb = val,
            "MemAvailable:" => mi.available_kb = val,
            "MemFree:" => mi.free_kb = val,
            "Buffers:" => mi.buffers_kb = val,
            "Cached:" => mi.cached_kb = val,
            "SwapTotal:" => mi.swap_total_kb = val,
            "SwapFree:" => mi.swap_free_kb = val,
            _ => {}
        }
    }

    Ok(mi)
}

fn read_uptime(path: &Path) -> std::io::Result<f64> {
    let content = fs::read_to_string(path)?;
    let first = content.split_whitespace().next().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "empty /proc/uptime")
    })?;
    first
        .parse::<f64>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

// --- tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn parses_coretemp_and_drivetemp_like_dn2800mt() {
        // This mirrors what we observed on the real DN2800MT test host:
        // hwmon0 = coretemp, only Core 0 (temp2); hwmon1 = drivetemp, no label.
        let dir = tempdir().unwrap();
        let root = dir.path();

        let hwmon0 = root.join("sys/class/hwmon/hwmon0");
        write_file(&hwmon0.join("name"), "coretemp\n");
        write_file(&hwmon0.join("temp2_input"), "38000\n");
        write_file(&hwmon0.join("temp2_label"), "Core 0\n");

        let hwmon1 = root.join("sys/class/hwmon/hwmon1");
        write_file(&hwmon1.join("name"), "drivetemp\n");
        write_file(&hwmon1.join("temp1_input"), "44000\n");

        let snap = read_snapshot_from(root);
        assert!(snap.available);
        assert_eq!(snap.hwmon.len(), 2);

        let core = &snap.hwmon[0];
        assert_eq!(core.name, "coretemp");
        assert_eq!(core.temperatures.len(), 1);
        assert_eq!(core.temperatures[0].label, "Core 0");
        assert!((core.temperatures[0].value - 38.0).abs() < 1e-6);
        assert_eq!(core.temperatures[0].unit, "°C");

        let drv = &snap.hwmon[1];
        assert_eq!(drv.name, "drivetemp");
        assert_eq!(drv.temperatures.len(), 1);
        // No label file => falls back to "temp1".
        assert_eq!(drv.temperatures[0].label, "temp1");
        assert!((drv.temperatures[0].value - 44.0).abs() < 1e-6);
    }

    #[test]
    fn parses_full_hwmon_with_voltage_fan_power() {
        // A richer chip, to exercise the non-temperature code paths.
        let dir = tempdir().unwrap();
        let root = dir.path();

        let h = root.join("sys/class/hwmon/hwmon0");
        write_file(&h.join("name"), "nct6775\n");
        write_file(&h.join("temp1_input"), "45500\n");
        write_file(&h.join("temp1_label"), "SYSTIN\n");
        write_file(&h.join("in0_input"), "1200\n"); // 1.200 V
        write_file(&h.join("in0_label"), "VCORE\n");
        write_file(&h.join("fan1_input"), "1850\n"); // RPM
        write_file(&h.join("curr1_input"), "2500\n"); // 2.5 A
        write_file(&h.join("power1_input"), "3500000\n"); // 3.5 W

        let snap = read_snapshot_from(root);
        let chip = &snap.hwmon[0];
        assert_eq!(chip.temperatures.len(), 1);
        assert_eq!(chip.voltages.len(), 1);
        assert_eq!(chip.fans.len(), 1);
        assert_eq!(chip.currents.len(), 1);
        assert_eq!(chip.powers.len(), 1);
        assert_eq!(chip.voltages[0].label, "VCORE");
        assert!((chip.voltages[0].value - 1.2).abs() < 1e-9);
        assert_eq!(chip.voltages[0].unit, "V");
        assert!((chip.fans[0].value - 1850.0).abs() < 1e-9);
        assert!((chip.currents[0].value - 2.5).abs() < 1e-9);
        assert!((chip.powers[0].value - 3.5).abs() < 1e-9);
    }

    #[test]
    fn parses_cpu_freq_and_governor() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        for (i, freq) in [(0u32, 1_866_632u64), (1, 1_866_638), (2, 1_862_000), (3, 1_862_000)] {
            let cpu = root.join(format!("sys/devices/system/cpu/cpu{}/cpufreq", i));
            write_file(&cpu.join("scaling_cur_freq"), &format!("{}\n", freq));
            write_file(&cpu.join("scaling_governor"), "performance\n");
        }
        // A "cpuidle" sibling that should be ignored (no cpufreq dir).
        fs::create_dir_all(root.join("sys/devices/system/cpu/cpuidle")).unwrap();

        let snap = read_snapshot_from(root);
        assert_eq!(snap.cpus.len(), 4);
        assert_eq!(snap.cpus[0].cpu, 0);
        assert!((snap.cpus[0].freq_mhz - 1866.632).abs() < 1e-3);
        assert_eq!(snap.cpus[0].governor, "performance");
        assert_eq!(snap.cpus[3].cpu, 3);
    }

    #[test]
    fn parses_loadavg_memory_uptime() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        write_file(&root.join("proc/loadavg"), "0.71 0.51 0.27 1/234 5678\n");
        write_file(&root.join("proc/uptime"), "5370.45 21123.89\n");
        write_file(
            &root.join("proc/meminfo"),
            "MemTotal:        3959480 kB\nMemFree:         2123456 kB\nMemAvailable:    3200000 \
             kB\nBuffers:           50000 kB\nCached:           500000 kB\nSwapTotal:       \
             1000000 kB\nSwapFree:         900000 kB\n",
        );

        let snap = read_snapshot_from(root);
        let la = snap.load_avg.expect("loadavg should parse");
        assert!((la.one - 0.71).abs() < 1e-6);
        assert!((la.five - 0.51).abs() < 1e-6);
        assert!((la.fifteen - 0.27).abs() < 1e-6);

        let mem = snap.memory.expect("meminfo should parse");
        assert_eq!(mem.total_kb, 3_959_480);
        assert_eq!(mem.available_kb, 3_200_000);
        assert_eq!(mem.swap_total_kb, 1_000_000);

        assert!((snap.uptime_seconds.unwrap() - 5370.45).abs() < 1e-6);
    }

    #[test]
    fn returns_empty_snapshot_when_nothing_is_present() {
        let dir = tempdir().unwrap();
        let snap = read_snapshot_from(dir.path());
        assert!(!snap.available);
        assert!(snap.hwmon.is_empty());
        assert!(snap.cpus.is_empty());
        assert!(snap.load_avg.is_none());
        assert!(snap.memory.is_none());
        assert!(snap.uptime_seconds.is_none());
    }
}
