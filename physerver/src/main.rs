// All modules live in the physerver lib crate (src/lib.rs), which also
// re-exports the protocol / transport / rt primitives from phycmd-core.
// This binary imports them from the lib instead of redeclaring `mod`s,
// which used to compile duplicated copies of every module into the bin.
use physerver::{
    config,
    ipc,
    rt,
    serial,
    telemetry,
    transport,
    web,
};

// Hard-RT primitives from phycmd-core (re-exported transparently via
// physerver::transport because phycmd-core is the actual implementor).
use phycmd_core::{
    CommandStaging, RtConfig as CoreRtConfig, RtScheduler, RtStats,
    StatusBus, WriteMode,
};

use anyhow::{Context, Result};
use clap::Parser;
use transport::Transport;
use std::sync::Arc;
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

    // Create IPC server if enabled. Wrap in Arc so we can clone the
    // handle into both the status-consumer and command-forwarder tokio
    // tasks below. IpcServer itself does not implement Clone.
    let ipc_server: Option<Arc<ipc::IpcServer>> = if !args.no_ipc {
        match ipc::IpcServer::new() {
            Ok(server) => {
                info!("IPC server initialized");
                Some(Arc::new(server))
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
    info!("Starting hard-RT scheduler at {} Hz", update_rate);

    // =================================================================
    //   Hard real-time loop via phycmd-core::RtScheduler
    //
    //   The scheduler runs on a dedicated std::thread with SCHED_FIFO
    //   and clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME). It is
    //   fully decoupled from the tokio async runtime that serves HTTP
    //   and WebSocket — tokio cannot preempt or delay the RT thread.
    //
    //   Communication with the tokio side:
    //     * CommandStaging  — HTTP/WS handlers push setpoints here.
    //                          The scheduler takes a snapshot each tick.
    //     * StatusBus       — scheduler publishes each status frame.
    //                          A tokio task consumes and forwards to
    //                          the WebSocket broadcast + IPC + web_state.
    // =================================================================

    // RT shared state
    let staging = Arc::new(CommandStaging::new(WriteMode::Coalesce));
    let bus = Arc::new(StatusBus::new(1024));
    let stats = Arc::new(RtStats::new());

    // Build the RT scheduler config from the service config.toml
    let rt_cfg = CoreRtConfig {
        rate_hz: update_rate,
        default_write_mode: WriteMode::Coalesce,
        // RT optimisations have already been applied at the process
        // level above; tell the scheduler NOT to re-apply them on its
        // own thread (mlockall is process-wide; SCHED_FIFO is
        // thread-specific and the rt::apply_rt_optimizations call
        // above only affected the main thread). So we re-enable here
        // so the dedicated RT thread also gets SCHED_FIFO.
        enable_rt: config.realtime.enabled,
        rt_priority: config.realtime.priority,
        lock_memory: false, // already done process-wide
        cpu_affinity: config.realtime.cpu_core,
        dma_latency_us: None,
        ..Default::default()
    };

    // Note on IPC: the shared_memory crate's IpcServer holds raw
    // pointers that are not Send, so we cannot use it from tokio
    // tasks directly. For now the status goes to the web state and
    // broadcast only; IPC shared memory is initialised but not
    // written to by the new RT scheduler path. A future refactor
    // will move IPC read/write into the RT thread itself via a
    // custom Transport wrapper.
    let _ipc_server = ipc_server; // keep alive, avoid drop

    // Consumer task: forward every status frame from the bus to the
    // web state + WebSocket broadcast.
    {
        let mut rx = bus.subscribe();
        let web_state_c = web_state.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(frame) => {
                        let status = frame.status;
                        web_state_c.maybe_init_error_baseline(status.error_count);
                        {
                            let mut s = web_state_c.current_status.write().await;
                            *s = status.clone();
                        }
                        let _ = web_state_c.status_broadcast.send(status);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!("status bus lagged by {n} frames");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // Command forwarder task: poll web_state.current_command at 500 Hz
    // and push every field into the staging buffer in Coalesce mode.
    {
        let staging_c = Arc::clone(&staging);
        let web_state_c = web_state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let cmd = web_state_c.current_command.read().await.clone();
                // Coalesce writes — cheap, non-blocking.
                let _ = staging_c.set_dac0(cmd.dac[0]);
                let _ = staging_c.set_dac1(cmd.dac[1]);
                let _ = staging_c.set_pwm0(cmd.pwm[0]);
                let _ = staging_c.set_pwm1(cmd.pwm[1]);
                let _ = staging_c.set_digital_out(cmd.digital_out);
                let _ = staging_c.set_flags(cmd.flags);
            }
        });
    }

    // Expose stats through the web module.
    web_state.set_rt_stats(Arc::clone(&stats));

    // Spawn the RT scheduler on a dedicated thread with its own
    // SCHED_FIFO scheduling class. This is where the hard-real-time
    // ticking actually happens.
    let scheduler = RtScheduler::new(
        rt_cfg,
        Arc::clone(&staging),
        Arc::clone(&bus),
        Arc::clone(&stats),
        transport,
    );
    let stop_handle = scheduler.stop_handle();
    let rt_thread = std::thread::Builder::new()
        .name("phycmd-rt".to_string())
        .spawn(move || {
            if let Err(e) = scheduler.run() {
                error!("RT scheduler terminated with error: {e}");
            }
        })
        .context("Failed to spawn RT scheduler thread")?;

    // Install a signal handler so Ctrl-C / systemd stop shuts the
    // RT thread down cleanly instead of aborting mid-tick.
    tokio::spawn({
        let stop_handle = stop_handle.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            info!("shutdown requested, stopping RT scheduler");
            stop_handle.stop();
        }
    });

    // Block the main thread until the RT thread exits. When SIGTERM
    // hits us (systemd stop), tokio's main will exit, which drops
    // everything and the RT thread exits via the stop flag.
    rt_thread
        .join()
        .map_err(|_| anyhow::anyhow!("RT thread panicked"))?;

    info!("physerver terminated cleanly");
    Ok(())
}
