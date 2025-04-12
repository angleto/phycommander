// All modules live in the physerver lib crate (src/lib.rs), which also
// re-exports the protocol / transport / rt primitives from phycmd-core.
// This binary imports them from the lib instead of redeclaring `mod`s,
// which used to compile duplicated copies of every module into the bin.
use physerver::{
    config,
    ipc,
    protocol,
    rt,
    serial,
    telemetry,
    transport,
    web,
};

use anyhow::{Context, Result};
use clap::Parser;
use transport::Transport;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Transport type: "serial" or "usb"
    #[arg(short, long)]
    transport: Option<String>,

    /// Serial port device (e.g., /dev/ttyACM0)
    #[arg(short, long)]
    port: Option<String>,

    /// Baud rate
    #[arg(short, long)]
    baud: Option<u32>,

    /// Web server port
    #[arg(short, long)]
    web_port: Option<u16>,

    /// Update rate in Hz
    #[arg(short, long)]
    rate: Option<u32>,

    /// Enable real-time scheduling (requires root/capabilities)
    #[arg(long)]
    rt: Option<bool>,

    /// Real-time priority (1-99)
    #[arg(long)]
    rt_priority: Option<i32>,

    /// Disable IPC (shared memory)
    #[arg(long, default_value = "false")]
    no_ipc: bool,

    /// Disable web server
    #[arg(long, default_value = "false")]
    no_web: bool,

    /// Auto-detect device
    #[arg(long)]
    auto_detect: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    telemetry::init();

    // Parse command-line arguments
    let args = Args::parse();

    info!("PhyServer - Physical Commander Server v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let mut config = if let Some(config_path) = &args.config {
        info!("Loading configuration from: {}", config_path);
        config::Config::from_file(config_path)
            .context("Failed to load configuration file")?
    } else {
        config::Config::default()
    };

    // Override config with command-line arguments
    if let Some(transport_type) = &args.transport {
        config.transport.transport_type = transport_type.clone();
    }
    if let Some(port) = &args.port {
        config.transport.serial_port = port.clone();
    }
    if let Some(baud) = args.baud {
        config.transport.baud_rate = baud;
    }
    if let Some(rate) = args.rate {
        config.transport.update_rate = rate;
    }
    if let Some(auto_detect) = args.auto_detect {
        config.transport.auto_detect = auto_detect;
    }
    if let Some(web_port) = args.web_port {
        config.web.port = web_port;
    }
    if let Some(rt) = args.rt {
        config.realtime.enabled = rt;
    }
    if let Some(rt_priority) = args.rt_priority {
        config.realtime.priority = rt_priority;
    }

    // Create transport
    info!("Transport mode: {}", config.transport.transport_type);

    let mut transport: Box<dyn Transport> = if config.transport.auto_detect {
        info!("Auto-detecting PhyCMD device...");

        // Try USB first (better performance) if feature is enabled
        #[cfg(feature = "usb")]
        {
            match transport::usb::UsbTransport::new() {
                Ok(usb) => {
                    info!("✓ USB transport detected");
                    Box::new(usb)
                }
                Err(_) => {
                    info!("USB transport not available, trying serial...");
                    // Try serial auto-detect
                    let port_name = serial::SerialPortHandler::find_phycmd_device()
                        .context("Failed to auto-detect any device")?;
                    let serial = transport::serial::SerialTransport::new(&port_name, config.transport.baud_rate)
                        .context("Failed to open serial transport")?;
                    info!("✓ Serial transport detected on {}", port_name);
                    Box::new(serial)
                }
            }
        }
        #[cfg(not(feature = "usb"))]
        {
            // USB not available, use serial only
            info!("USB support not compiled in, using serial...");
            let port_name = serial::SerialPortHandler::find_phycmd_device()
                .context("Failed to auto-detect serial device")?;
            let serial = transport::serial::SerialTransport::new(&port_name, config.transport.baud_rate)
                .context("Failed to open serial transport")?;
            info!("✓ Serial transport detected on {}", port_name);
            Box::new(serial)
        }
    } else {
        // Use configured transport type
        match config.transport.transport_type.as_str() {
            #[cfg(feature = "usb")]
            "usb" => {
                info!("Creating USB transport...");
                let usb = transport::usb::UsbTransport::new()
                    .context("Failed to create USB transport")?;
                info!("✓ USB transport initialized (VID:PID = 0x2341:0x003e)");
                Box::new(usb)
            }
            #[cfg(not(feature = "usb"))]
            "usb" => {
                anyhow::bail!("USB transport not available. Rebuild with --features usb and install libudev-dev");
            }
            "serial" => {
                info!("Creating serial transport on {}", config.transport.serial_port);
                let serial = transport::serial::SerialTransport::new(
                    &config.transport.serial_port,
                    config.transport.baud_rate
                ).context("Failed to create serial transport")?;
                info!("✓ Serial transport initialized ({} baud)", config.transport.baud_rate);
                Box::new(serial)
            }
            _ => {
                anyhow::bail!("Invalid transport type: {}. Use 'serial'{}",
                    config.transport.transport_type,
                    if cfg!(feature = "usb") { " or 'usb'" } else { "" });
            }
        }
    };

    info!("Transport info: max_rate={}Hz, typical_latency={}µs",
          transport.max_rate(),
          transport.typical_latency_us());

    // Apply real-time optimizations if requested
    if config.realtime.enabled {
        info!("Applying real-time optimizations...");

        if !rt::check_rt_capabilities() {
            warn!("No real-time capabilities detected. Run with sudo or set CAP_SYS_NICE capability:");
            warn!("  sudo setcap cap_sys_nice=eip target/release/physerver");
        }

        let rt_config = rt::RtConfig {
            enable_rt_scheduler: true,
            rt_priority: config.realtime.priority,
            lock_memory: true,
            set_cpu_affinity: config.realtime.cpu_affinity,
            cpu_core: config.realtime.cpu_core,
            set_dma_latency: true,
        };

        if let Err(e) = rt::apply_rt_optimizations(&rt_config) {
            warn!("Failed to apply some RT optimizations: {}", e);
        }
    }

    // Create shared state for web server
    let web_state = Arc::new(web::AppState::new());

    // Create IPC server if enabled
    let ipc_server = if !args.no_ipc {
        match ipc::IpcServer::new() {
            Ok(server) => {
                info!("IPC server initialized");
                Some(server)
            }
            Err(e) => {
                warn!("Failed to create IPC server: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Start web server if enabled
    if !args.no_web {
        let web_state_clone = web_state.clone();
        let web_port = config.web.port;

        tokio::spawn(async move {
            if let Err(e) = web::start_web_server(web_state_clone, web_port).await {
                error!("Web server error: {}", e);
            }
        });

        info!("Web server started on http://{}:{}", config.web.bind_address, config.web.port);
    }

    let update_rate = config.transport.update_rate;
    info!("Starting main communication loop at {} Hz", update_rate);

    // Calculate loop period
    let period = Duration::from_micros(1_000_000 / update_rate as u64);
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Main communication loop
    let mut seq_num: u8 = 0;

    loop {
        interval.tick().await;

        // Read command from IPC or web state
        let mut cmd = if let Some(ref ipc) = ipc_server {
            ipc.read_command()
        } else {
            web_state.current_command.read().await.clone()
        };

        // Set sequence number
        cmd.seq_num = seq_num;

        // Exchange with device
        match transport.exchange(&cmd) {
            Ok(status) => {
                // Verify sequence number
                if status.seq_num != seq_num {
                    warn!("Sequence mismatch: sent {}, received {}", seq_num, status.seq_num);
                }

                // Update IPC
                if let Some(ref ipc) = ipc_server {
                    ipc.write_status(&status);
                }

                // Update web state
                {
                    let mut current_status = web_state.current_status.write().await;
                    *current_status = status.clone();
                }

                // Broadcast to WebSocket clients
                let _ = web_state.status_broadcast.send(status.clone());

                // Check for errors
                if status.flags.error {
                    warn!("Device reported error flag. Error count: {}", status.error_count);
                }

                if status.loop_time_us > 1000 {
                    warn!("Device loop time high: {} µs", status.loop_time_us);
                }
            }
            Err(e) => {
                error!("Communication error: {}", e);
                // Continue running, will retry next iteration
            }
        }

        seq_num = seq_num.wrapping_add(1);
    }
}
