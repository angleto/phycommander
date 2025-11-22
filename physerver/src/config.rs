use serde::{Deserialize, Serialize};
use std::path::Path;
use anyhow::{Context, Result};

/// Physerver configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Transport configuration
    pub transport: TransportConfig,

    /// Web server configuration
    pub web: WebConfig,

    /// Real-time configuration
    pub realtime: RealtimeConfig,

    /// IPC configuration
    pub ipc: IpcConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Transport type: "usb" or "serial"
    #[serde(rename = "type")]
    pub transport_type: String,

    /// Serial port device (for serial transport)
    #[serde(default = "default_serial_port")]
    pub serial_port: String,

    /// Baud rate (for serial transport, ignored by USB)
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,

    /// Update rate in Hz
    #[serde(default = "default_update_rate")]
    pub update_rate: u32,

    /// Auto-detect device
    #[serde(default)]
    pub auto_detect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Web server port
    #[serde(default = "default_web_port")]
    pub port: u16,

    /// Enable web server
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Bind address (0.0.0.0 for all interfaces, 127.0.0.1 for localhost only)
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeConfig {
    /// Enable real-time scheduling
    #[serde(default)]
    pub enabled: bool,

    /// Real-time priority (1-99)
    #[serde(default = "default_rt_priority")]
    pub priority: i32,

    /// Lock memory (mlockall)
    #[serde(default = "default_true")]
    pub lock_memory: bool,

    /// Set CPU affinity
    #[serde(default)]
    pub cpu_affinity: bool,

    /// CPU core to pin to (if cpu_affinity is true)
    pub cpu_core: Option<usize>,

    /// Set DMA latency
    #[serde(default = "default_true")]
    pub dma_latency: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcConfig {
    /// Enable IPC (shared memory)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Shared memory name
    #[serde(default = "default_shm_name")]
    pub shm_name: String,
}

// Default values
fn default_serial_port() -> String {
    "/dev/ttyACM0".to_string()
}

fn default_baud_rate() -> u32 {
    921600
}

fn default_update_rate() -> u32 {
    1000
}

fn default_web_port() -> u16 {
    8080
}

fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

fn default_rt_priority() -> i32 {
    80
}

fn default_shm_name() -> String {
    "phycmd_state".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            transport: TransportConfig {
                transport_type: "usb".to_string(), // Prefer USB for best performance
                serial_port: default_serial_port(),
                baud_rate: default_baud_rate(),
                update_rate: 5000, // 5 kHz with USB
                auto_detect: true,
            },
            web: WebConfig {
                port: default_web_port(),
                enabled: true,
                bind_address: default_bind_address(),
            },
            realtime: RealtimeConfig {
                enabled: false, // User must opt-in
                priority: default_rt_priority(),
                lock_memory: true,
                cpu_affinity: false,
                cpu_core: None,
                dma_latency: true,
            },
            ipc: IpcConfig {
                enabled: true,
                shm_name: default_shm_name(),
            },
        }
    }
}

impl Config {
    /// Load configuration from TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = std::fs::read_to_string(path.as_ref())
            .context("Failed to read config file")?;

        let config: Config = toml::from_str(&contents)
            .context("Failed to parse config file")?;

        Ok(config)
    }

    /// Load configuration with fallback to default
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> Self {
        match Self::from_file(path) {
            Ok(config) => {
                tracing::info!("Loaded configuration from file");
                config
            }
            Err(e) => {
                tracing::warn!("Failed to load config, using defaults: {}", e);
                Self::default()
            }
        }
    }

    /// Save configuration to TOML file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        std::fs::write(path, contents)
            .context("Failed to write config file")?;

        Ok(())
    }

    /// Create example configuration file
    pub fn create_example<P: AsRef<Path>>(path: P) -> Result<()> {
        let config = Self::default();
        config.save(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.transport.transport_type, "usb");
        assert_eq!(config.transport.update_rate, 5000);
        assert_eq!(config.web.port, 8080);
        assert!(config.web.enabled);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();

        // Should be valid TOML
        let _parsed: Config = toml::from_str(&toml_str).unwrap();
    }

    #[test]
    fn test_config_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.transport.transport_type, parsed.transport.transport_type);
        assert_eq!(config.transport.update_rate, parsed.transport.update_rate);
        assert_eq!(config.web.port, parsed.web.port);
        assert_eq!(config.web.enabled, parsed.web.enabled);
    }

    #[test]
    fn test_transport_config_custom() {
        let mut config = Config::default();
        config.transport.transport_type = "serial".to_string();
        config.transport.update_rate = 1000;
        config.transport.serial_port = "/dev/ttyUSB0".to_string();
        config.transport.baud_rate = 115200;

        assert_eq!(config.transport.transport_type, "serial");
        assert_eq!(config.transport.update_rate, 1000);
        assert_eq!(config.transport.serial_port, "/dev/ttyUSB0");
        assert_eq!(config.transport.baud_rate, 115200);
    }

    #[test]
    fn test_web_config_custom() {
        let mut config = Config::default();
        config.web.port = 9090;
        config.web.enabled = false;
        config.web.bind_address = "127.0.0.1".to_string();

        assert_eq!(config.web.port, 9090);
        assert!(!config.web.enabled);
        assert_eq!(config.web.bind_address, "127.0.0.1");
    }

    #[test]
    fn test_realtime_config_custom() {
        let mut config = Config::default();
        config.realtime.enabled = true;
        config.realtime.priority = 50;
        config.realtime.lock_memory = false;
        config.realtime.cpu_affinity = true;
        config.realtime.cpu_core = Some(2);
        config.realtime.dma_latency = false;

        assert!(config.realtime.enabled);
        assert_eq!(config.realtime.priority, 50);
        assert!(!config.realtime.lock_memory);
        assert!(config.realtime.cpu_affinity);
        assert_eq!(config.realtime.cpu_core, Some(2));
        assert!(!config.realtime.dma_latency);
    }

    #[test]
    fn test_ipc_config_custom() {
        let mut config = Config::default();
        config.ipc.enabled = false;
        config.ipc.shm_name = "custom_shm".to_string();

        assert!(!config.ipc.enabled);
        assert_eq!(config.ipc.shm_name, "custom_shm");
    }

    #[test]
    fn test_load_or_default_with_nonexistent_file() {
        let config = Config::load_or_default("/nonexistent/path/config.toml");

        // Should return default config
        assert_eq!(config.transport.transport_type, "usb");
        assert_eq!(config.web.port, 8080);
    }

    #[test]
    fn test_save_and_load_config() -> anyhow::Result<()> {
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("physerver_test_config.toml");

        // Create and save a config
        let mut original = Config::default();
        original.transport.transport_type = "serial".to_string();
        original.web.port = 9999;

        original.save(&config_path)?;

        // Load it back
        let loaded = Config::from_file(&config_path)?;

        assert_eq!(loaded.transport.transport_type, "serial");
        assert_eq!(loaded.web.port, 9999);

        // Clean up
        fs::remove_file(&config_path)?;

        Ok(())
    }

    #[test]
    fn test_config_from_invalid_file() {
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("physerver_invalid_config.toml");

        // Write invalid TOML
        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(b"this is not valid toml [[[").unwrap();

        // Should fail to load
        let result = Config::from_file(&config_path);
        assert!(result.is_err());

        // Clean up
        fs::remove_file(&config_path).ok();
    }

    #[test]
    fn test_default_serial_port() {
        assert_eq!(default_serial_port(), "/dev/ttyACM0");
    }

    #[test]
    fn test_default_baud_rate() {
        assert_eq!(default_baud_rate(), 921600);
    }

    #[test]
    fn test_default_update_rate() {
        assert_eq!(default_update_rate(), 1000);
    }

    #[test]
    fn test_default_web_port() {
        assert_eq!(default_web_port(), 8080);
    }

    #[test]
    fn test_default_bind_address() {
        assert_eq!(default_bind_address(), "0.0.0.0");
    }

    #[test]
    fn test_default_rt_priority() {
        assert_eq!(default_rt_priority(), 80);
    }

    #[test]
    fn test_default_shm_name() {
        assert_eq!(default_shm_name(), "phycmd_state");
    }

    #[test]
    fn test_default_true() {
        assert_eq!(default_true(), true);
    }
}
