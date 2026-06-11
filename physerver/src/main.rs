// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2014-2026 Angelo Leto <angelo@leto.blue>

// All modules live in the physerver lib crate (src/lib.rs), which also
// re-exports the protocol / transport / rt primitives from phycmd-core.
// This binary imports them from the lib instead of redeclaring `mod`s,
// which used to compile duplicated copies of every module into the bin.
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
// Hard-RT primitives from phycmd-core (re-exported transparently via
// physerver::transport because phycmd-core is the actual implementor).
use phycmd_core::{
    CommandStaging, RtConfig as CoreRtConfig, RtScheduler, RtStats, StatusBus, WriteMode,
};
use physerver::{config, ipc, rt, serial, telemetry, transport, web};
use tracing::{error, info, warn};
use transport::Transport;

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
        config::Config::from_file(config_path).context("Failed to load configuration file")?
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

    // Iso mode short-circuits the bulk Transport creation entirely:
    // the IsoTransport spawned later owns its own libusb context and
    // I/O thread and never goes through the Box<dyn Transport> path.
    let iso_mode = config.transport.transport_type.eq_ignore_ascii_case("iso");

    let transport: Option<Box<dyn Transport>> = if iso_mode {
        info!(
            "Iso mode selected — skipping bulk Transport creation; IsoTransport will start after \
             web/ipc plumbing is up"
        );
        None
    } else {
        Some(create_bulk_transport(&config)?)
    };

    if let Some(t) = transport.as_ref() {
        info!(
            "Transport info: max_rate={}Hz, typical_latency={}µs",
            t.max_rate(),
            t.typical_latency_us()
        );
    }

    // Apply real-time optimizations if requested
    if config.realtime.enabled {
        info!("Applying real-time optimizations...");

        if !rt::check_rt_capabilities() {
            warn!(
                "No real-time capabilities detected. Run with sudo or set CAP_SYS_NICE capability:"
            );
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
        let auth = config.auth.clone();
        let scheme = if auth.tls_cert_file.is_some() && auth.tls_key_file.is_some() {
            "https"
        } else {
            "http"
        };

        tokio::spawn(async move {
            if let Err(e) = web::start_web_server(web_state_clone, web_port, auth).await {
                error!("Web server error: {}", e);
            }
        });

        info!(
            "Web server started on {}://{}:{}",
            scheme, config.web.bind_address, config.web.port
        );
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
    //     * CommandStaging  — HTTP/WS handlers push setpoints here. The scheduler takes a snapshot
    //       each tick.
    //     * StatusBus       — scheduler publishes each status frame. A tokio task consumes and
    //       forwards to the WebSocket broadcast + IPC + web_state.
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

    // IPC wiring. `IpcServer` now implements Send+Sync (see
    // `src/ipc/mod.rs`) because SharedState is built entirely on
    // atomics, so sharing the Arc between the two tokio tasks below
    // is safe. When iso-mode or bulk-mode isn't using IPC at all
    // (e.g. the user passed --no-ipc), `ipc_server` is None and the
    // tasks simply skip the IPC push/pull.
    let ipc_server_consumer = ipc_server.clone();
    let ipc_server_forwarder = ipc_server.clone();

    // Consumer task: forward status frames from the bus to the web
    // state + WebSocket broadcast, and push into IPC shared memory.
    //
    // Iso mode produces ~8000 frames/s (one per USB microframe), which
    // is far more than any browser can display and would burn CPU on
    // JSON serialisation in the WebSocket fan-out (one full Status
    // marshal per client per frame). We always update `current_status`
    // (cheap RwLock write — also serves the REST /api/status path),
    // but throttle the WebSocket broadcast to a max of ~250 Hz with a
    // wall-clock minimum interval. Bulk mode at 1 kHz also gets
    // throttled to 250 Hz, which is still well above the dashboard's
    // refresh budget. IPC writes are cheap atomic stores so we push
    // every frame to keep shared-memory readers tight on latency.
    {
        let mut rx = bus.subscribe();
        let web_state_c = web_state.clone();
        let ipc_c = ipc_server_consumer;
        tokio::spawn(async move {
            const WS_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(4);
            let mut last_ws_send = std::time::Instant::now()
                .checked_sub(WS_MIN_INTERVAL)
                .unwrap_or_else(std::time::Instant::now);
            loop {
                match rx.recv().await {
                    Ok(frame) => {
                        let status = frame.status;
                        web_state_c.maybe_init_error_baseline(status.error_count);
                        {
                            let mut s = web_state_c.current_status.write().await;
                            *s = status.clone();
                        }
                        // Full-rate ADC capture for /api/adc/capture. Push
                        // every frame (no WS-style throttle) so a client
                        // that polls sees the waveform at the true 8 kHz
                        // rate. Mutex contention is a non-event: push takes
                        // <100 ns and the handler only runs at ~10 Hz.
                        web_state_c.adc_ring.lock().push(
                            status.adc,
                            status.digital_in,
                            status.digital_out,
                        );
                        if let Some(ipc) = ipc_c.as_ref() {
                            ipc.write_status(&status);
                        }
                        let now = std::time::Instant::now();
                        if now.duration_since(last_ws_send) >= WS_MIN_INTERVAL {
                            last_ws_send = now;
                            let _ = web_state_c.status_broadcast.send(status);
                        }
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
    // Also pulls commands from IPC (shared memory) so external clients
    // that write via `IpcClient::write_command` get their setpoints
    // applied without going through HTTP/WS. Last-writer-wins between
    // HTTP and IPC within the same tick, which is consistent with the
    // pre-regression behavior.
    {
        let staging_c = Arc::clone(&staging);
        let web_state_c = web_state.clone();
        let ipc_c = ipc_server_forwarder;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Track the last IPC command sequence we observed, so we
            // only push into staging when a client actually wrote
            // something new. Prevents IPC → staging → ack → IPC loops.
            let mut last_ipc_seq: u64 = 0;
            loop {
                interval.tick().await;
                let cmd_http = web_state_c.current_command.read().await.clone();
                // Coalesce writes — cheap, non-blocking.
                let _ = staging_c.set_dac0(cmd_http.dac[0]);
                let _ = staging_c.set_dac1(cmd_http.dac[1]);
                let _ = staging_c.set_pwm0(cmd_http.pwm[0]);
                let _ = staging_c.set_pwm1(cmd_http.pwm[1]);
                let _ = staging_c.set_digital_out(cmd_http.digital_out);
                let _ = staging_c.set_flags(cmd_http.flags);

                if let Some(ipc) = ipc_c.as_ref() {
                    let (cmd_ipc, seq) = ipc.read_command_with_seq();
                    if seq != last_ipc_seq {
                        last_ipc_seq = seq;
                        let _ = staging_c.set_dac0(cmd_ipc.dac[0]);
                        let _ = staging_c.set_dac1(cmd_ipc.dac[1]);
                        let _ = staging_c.set_pwm0(cmd_ipc.pwm[0]);
                        let _ = staging_c.set_pwm1(cmd_ipc.pwm[1]);
                        let _ = staging_c.set_digital_out(cmd_ipc.digital_out);
                        let _ = staging_c.set_flags(cmd_ipc.flags);
                    }
                }
            }
        });
    }

    // Expose stats through the web module.
    web_state.set_rt_stats(Arc::clone(&stats));

    if iso_mode {
        // -------------------------------------------------------------
        //   Iso path — IsoTransport runs its own libusb I/O thread.
        //   No RtScheduler, no fixed tick rate: status frames flow at
        //   the USB microframe rate (8 kHz HS) and commands are taken
        //   from `staging` once per OUT transfer (every ~1 ms).
        // -------------------------------------------------------------
        let iso = Arc::new(
            phycmd_core::transport::IsoTransport::new(
                Arc::clone(&staging),
                Arc::clone(&bus),
                Arc::clone(&stats),
            )
            .context("Failed to start IsoTransport")?,
        );
        web_state.set_iso_stats(iso.iso_stats_arc());
        web_state.set_waveforms(iso.waveforms());
        web_state.set_waveform_dev(iso.waveform_dev());
        web_state.set_iso_transport(Arc::clone(&iso));

        // Default-off at startup: stop every on-chip generator and
        // zero the streaming command. Without this, a physerver
        // restart that doesn't go through a firmware flash would
        // leave whatever waveform was running before still going,
        // and the streaming Command frame's last-known DAC/DOUT
        // values would persist on the pads. Errors here are logged
        // but non-fatal — at worst the user sees stale outputs and
        // can hit Stop on the dashboard.
        {
            let dev = iso.waveform_dev();
            for ch in
                ["dac0", "dac1", "pwm0", "pwm1", "pwm2", "pwm3", "pwm4", "pwm5", "pwm6", "pwm7"]
            {
                if let Err(e) = dev.stop(ch) {
                    warn!("startup-off: fngen stop {ch} failed: {e}");
                }
            }
            let mut cmd = web_state.current_command.write().await;
            *cmd = phycmd_core::Command::default();
            info!("startup: all DAC/PWM/DOUT outputs forced OFF");
        }

        notify_systemd_ready(&web_state);

        // Block until shutdown. IsoTransport's Drop signals stop +
        // joins its I/O thread cleanly.
        let _ = tokio::signal::ctrl_c().await;
        info!("shutdown requested, stopping IsoTransport");
        drop(iso);
        info!("physerver (iso mode) terminated cleanly");
        return Ok(());
    }

    // -----------------------------------------------------------------
    //   Bulk path — pre-existing hard-RT scheduler on a dedicated
    //   SCHED_FIFO std::thread, sync exchange() per tick.
    // -----------------------------------------------------------------
    let transport = transport.expect("bulk transport must exist when iso_mode is false");
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

    notify_systemd_ready(&web_state);

    // Block the main thread until the RT thread exits. When SIGTERM
    // hits us (systemd stop), tokio's main will exit, which drops
    // everything and the RT thread exits via the stop flag.
    rt_thread.join().map_err(|_| anyhow::anyhow!("RT thread panicked"))?;

    info!("physerver terminated cleanly");
    Ok(())
}

/// Systemd integration: notify READY + start a watchdog kicker task
/// if the service unit configured WatchdogSec=. The kicker only
/// pings sd_notify(WATCHDOG=1) when the healthcheck passes, so a
/// stalled iso transport or a stuck USB reconnect causes systemd to
/// restart us — no more silent "service is up, seq_num frozen"
/// deployments. Running under `cargo run` (NOTIFY_SOCKET /
/// WATCHDOG_USEC unset) is a no-op. Called from both the iso and the
/// bulk paths of main() once their transport is up.
fn notify_systemd_ready(web_state: &Arc<web::AppState>) {
    // Best-effort READY notification — no-op outside systemd.
    if let Err(e) = sd_notify::notify(&[sd_notify::NotifyState::Ready]) {
        warn!("sd_notify(READY) failed: {e}");
    }
    if let Some(watchdog) = sd_notify::watchdog_enabled() {
        let interval = watchdog / 3;
        info!(
            "systemd watchdog: kicking every {} ms (WatchdogSec = {} ms)",
            interval.as_millis(),
            watchdog.as_millis()
        );
        let wd_state = web_state.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let checks = web::compute_health(&wd_state).await;
                if checks.healthy() {
                    if let Err(e) = sd_notify::notify(&[sd_notify::NotifyState::Watchdog]) {
                        warn!("sd_notify(WATCHDOG) failed: {e}");
                    }
                } else {
                    // Log only at debug so a sustained degraded
                    // state doesn't flood the journal. WatchdogSec
                    // will trip within one systemd-configured
                    // interval if we keep skipping.
                    tracing::debug!("skipping watchdog kick: health degraded ({:?})", checks);
                }
            }
        });
    }
}

/// Build the bulk Transport (USB or serial) from the config. Iso mode
/// short-circuits this entirely and uses IsoTransport instead.
fn create_bulk_transport(config: &config::Config) -> Result<Box<dyn Transport>> {
    if config.transport.auto_detect {
        info!("Auto-detecting PhyCMD device...");
        #[cfg(feature = "usb")]
        {
            match transport::usb::UsbTransport::new() {
                Ok(usb) => {
                    info!("✓ USB transport detected");
                    return Ok(Box::new(usb));
                }
                Err(_) => {
                    info!("USB transport not available, trying serial...");
                }
            }
        }
        let port_name = serial::SerialPortHandler::find_phycmd_device()
            .context("Failed to auto-detect any device")?;
        let serial_t =
            transport::serial::SerialTransport::new(&port_name, config.transport.baud_rate)
                .context("Failed to open serial transport")?;
        info!("✓ Serial transport detected on {}", port_name);
        return Ok(Box::new(serial_t));
    }

    match config.transport.transport_type.as_str() {
        #[cfg(feature = "usb")]
        "usb" => {
            info!("Creating USB transport...");
            let usb =
                transport::usb::UsbTransport::new().context("Failed to create USB transport")?;
            info!("✓ USB transport initialized (VID:PID = 0x2341:0x003e)");
            Ok(Box::new(usb))
        }
        #[cfg(not(feature = "usb"))]
        "usb" => {
            anyhow::bail!(
                "USB transport not available. Rebuild with --features usb and install libudev-dev"
            );
        }
        "serial" => {
            info!("Creating serial transport on {}", config.transport.serial_port);
            let serial_t = transport::serial::SerialTransport::new(
                &config.transport.serial_port,
                config.transport.baud_rate,
            )
            .context("Failed to create serial transport")?;
            info!("✓ Serial transport initialized ({} baud)", config.transport.baud_rate);
            Ok(Box::new(serial_t))
        }
        other => {
            anyhow::bail!("Invalid transport type: {other}. Use 'serial', 'usb', or 'iso'");
        }
    }
}
