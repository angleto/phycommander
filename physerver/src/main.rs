mod protocol;
mod serial;
mod ipc;
mod web;
mod rt;
mod telemetry;

use anyhow::{Context, Result};
use clap::Parser;
use protocol::{Command, Status};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Serial port device (e.g., /dev/ttyACM0)
    #[arg(short, long)]
    port: Option<String>,

    /// Baud rate
    #[arg(short, long, default_value = "921600")]
    baud: u32,

    /// Web server port
    #[arg(short, long, default_value = "8080")]
    web_port: u16,

    /// Update rate in Hz
    #[arg(short, long, default_value = "1000")]
    rate: u32,

    /// Enable real-time scheduling (requires root/capabilities)
    #[arg(long, default_value = "false")]
    rt: bool,

    /// Real-time priority (1-99)
    #[arg(long, default_value = "80")]
    rt_priority: i32,

    /// Disable IPC (shared memory)
    #[arg(long, default_value = "false")]
    no_ipc: bool,

    /// Disable web server
    #[arg(long, default_value = "false")]
    no_web: bool,

    /// Auto-detect device
    #[arg(long, default_value = "false")]
    auto_detect: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    telemetry::init();

    // Parse command-line arguments
    let args = Args::parse();

    info!("PhySever - Physical Commander Server v{}", env!("CARGO_PKG_VERSION"));

    // Determine serial port
    let port_name = if let Some(port) = args.port {
        port
    } else if args.auto_detect {
        info!("Auto-detecting PhyCMD device...");
        serial::SerialPortHandler::find_phycmd_device()
            .context("Failed to auto-detect device")?
    } else {
        // List available ports and ask user to specify
        let ports = serial::SerialPortHandler::available_ports()?;
        if ports.is_empty() {
            anyhow::bail!("No serial ports found. Is the device connected?");
        }

        info!("Available serial ports:");
        for port in &ports {
            info!("  - {}", port);
        }

        anyhow::bail!("Please specify a port with --port or use --auto-detect");
    };

    // Apply real-time optimizations if requested
    if args.rt {
        info!("Applying real-time optimizations...");

        if !rt::check_rt_capabilities() {
            warn!("No real-time capabilities detected. Run with sudo or set CAP_SYS_NICE capability:");
            warn!("  sudo setcap cap_sys_nice=eip target/release/physerver");
        }

        let rt_config = rt::RtConfig {
            enable_rt_scheduler: true,
            rt_priority: args.rt_priority,
            lock_memory: true,
            set_cpu_affinity: false,
            cpu_core: None,
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
        let web_port = args.web_port;

        tokio::spawn(async move {
            if let Err(e) = web::start_web_server(web_state_clone, web_port).await {
                error!("Web server error: {}", e);
            }
        });
    }

    // Open serial port
    let mut serial = serial::SerialPortHandler::new(&port_name, args.baud)
        .context("Failed to open serial port")?;

    info!("Starting main communication loop at {} Hz", args.rate);

    // Calculate loop period
    let period = Duration::from_micros(1_000_000 / args.rate as u64);
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
        match serial.exchange(&cmd) {
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
                let _ = web_state.status_broadcast.send(status);

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
