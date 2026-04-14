/// Configuration validator utility
///
/// Validates physerver configuration files for correctness
use anyhow::{Context, Result};
use clap::Parser;
use physerver::config::Config;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Validate PhyServer configuration files", long_about = None)]
struct Args {
    /// Configuration file to validate
    #[arg(short, long)]
    config: PathBuf,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Output format: text, json
    #[arg(short, long, default_value = "text")]
    format: String,
}

#[derive(Debug, serde::Serialize)]
struct ValidationResult {
    valid: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
    info: Vec<String>,
}

impl ValidationResult {
    fn new() -> Self {
        Self { valid: true, errors: Vec::new(), warnings: Vec::new(), info: Vec::new() }
    }

    fn error(&mut self, msg: String) {
        self.valid = false;
        self.errors.push(msg);
    }

    fn warning(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    fn info(&mut self, msg: String) {
        self.info.push(msg);
    }
}

fn validate_config(config: &Config, result: &mut ValidationResult) {
    // Validate transport configuration
    match config.transport.transport_type.as_str() {
        "usb" | "serial" => {
            result.info(format!("Transport type: {}", config.transport.transport_type));
        }
        _ => {
            result.error(format!(
                "Invalid transport type: '{}'. Must be 'usb' or 'serial'",
                config.transport.transport_type
            ));
        }
    }

    // Validate update rate
    if config.transport.update_rate == 0 {
        result.error("Update rate cannot be 0".to_string());
    } else if config.transport.update_rate > 10000 {
        result.warning(format!(
            "Update rate {} Hz is very high, may not be sustainable",
            config.transport.update_rate
        ));
    } else if config.transport.transport_type == "serial" && config.transport.update_rate > 1000 {
        result.warning(format!(
            "Serial transport at {} Hz may not be stable (recommended: ≤1000 Hz)",
            config.transport.update_rate
        ));
    } else {
        result.info(format!("Update rate: {} Hz", config.transport.update_rate));
    }

    // Validate baud rate
    let valid_baud_rates = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];
    if !valid_baud_rates.contains(&config.transport.baud_rate) {
        result.warning(format!(
            "Non-standard baud rate: {}. Common rates: {:?}",
            config.transport.baud_rate, valid_baud_rates
        ));
    }

    // Validate web configuration
    if config.web.port < 1024 && config.web.port != 0 {
        result.warning(format!(
            "Port {} requires elevated privileges (ports <1024)",
            config.web.port
        ));
    }

    if config.web.bind_address == "0.0.0.0" {
        result.warning(
            "Web server binds to all interfaces (0.0.0.0). Consider using 127.0.0.1 for security or setting up firewall".to_string()
        );
    }

    // Validate real-time configuration
    if config.realtime.enabled {
        result.info("Real-time scheduling enabled".to_string());

        if config.realtime.priority < 1 || config.realtime.priority > 99 {
            result.error(format!(
                "Real-time priority {} out of range (must be 1-99)",
                config.realtime.priority
            ));
        } else if config.realtime.priority > 90 {
            result.warning(format!(
                "Very high RT priority ({}). May interfere with kernel threads",
                config.realtime.priority
            ));
        }

        if config.realtime.cpu_affinity {
            if let Some(core) = config.realtime.cpu_core {
                result.info(format!("CPU affinity: core {}", core));
                if core >= num_cpus::get() {
                    result.error(format!(
                        "CPU core {} does not exist (system has {} cores)",
                        core,
                        num_cpus::get()
                    ));
                }
            } else {
                result.error("CPU affinity enabled but no core specified".to_string());
            }
        }
    }

    // Check for common issues
    if config.transport.auto_detect && config.transport.transport_type == "usb" {
        result.info("Auto-detect enabled with USB preference".to_string());
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Try to load the configuration
    let config = Config::from_file(&args.config).context("Failed to load configuration file")?;

    let mut result = ValidationResult::new();

    // Validate the configuration
    validate_config(&config, &mut result);

    // Output results
    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&result)?;
            println!("{}", json);
        }
        "text" | _ => {
            println!("Configuration Validation Report");
            println!("================================\n");
            println!("File: {}\n", args.config.display());

            if !result.errors.is_empty() {
                println!("❌ ERRORS:");
                for error in &result.errors {
                    println!("  • {}", error);
                }
                println!();
            }

            if !result.warnings.is_empty() {
                println!("⚠️  WARNINGS:");
                for warning in &result.warnings {
                    println!("  • {}", warning);
                }
                println!();
            }

            if args.verbose && !result.info.is_empty() {
                println!("ℹ️  INFO:");
                for info in &result.info {
                    println!("  • {}", info);
                }
                println!();
            }

            if result.valid {
                println!("✅ Configuration is VALID\n");
            } else {
                println!("❌ Configuration has ERRORS\n");
            }
        }
    }

    // Exit with appropriate code
    if result.valid {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
