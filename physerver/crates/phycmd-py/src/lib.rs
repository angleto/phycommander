//! `phycmd` Python module — pyo3 bindings for the PhyCommander
//! hard-real-time control stack.
//!
//! ## Module layout
//!
//! ```python
//! import phycmd
//!
//! # Configuration & writing
//! phy = phycmd.PhyCommander.open_mock(rate_hz=10_000, mock_latency_us=50)
//! phy.set_default_mode(phycmd.WriteMode.BlockUntilSent)
//! phy.set_dac0(1234)
//! phy.set_dac0(5678, mode=phycmd.WriteMode.Coalesce)
//!
//! # Observer pattern — callback-based
//! def on_status(frame):
//!     print(f"adc0={frame['adc'][0]} lat={frame['latency_us']}us")
//! phy.subscribe(on_status)
//!
//! # Stats + lifecycle
//! print(phy.stats())
//! phy.stop()
//! ```
//!
//! ## Concurrency model
//!
//! The RT scheduler runs on its own `std::thread` created inside
//! [`phycmd_rust::PhyCommander::open`]. Python method calls into
//! writer methods ([`PyPhyCommander::set_dac0`], etc.) release the
//! GIL around the underlying Rust call (`py.allow_threads(...)`)
//! so a BlockUntilSent wait does not stall other Python threads.
//!
//! Subscribers go through a **dispatcher thread** per registered
//! callback: the thread consumes from a
//! `broadcast::Receiver<StatusFrame>` with `blocking_recv`, and on
//! each frame it `Python::with_gil` and calls the user callback.
//! Slow callbacks only delay their own dispatcher — the RT loop
//! keeps running and the broadcast channel drops the oldest frames
//! for the lagging subscriber.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// NB: `phycmd-rust` builds a lib called `phycmd`, so we import from
// `phycmd::*`. Do NOT write `use phycmd_rust::*`.
use parking_lot::Mutex;
use phycmd_core::transport::mock::MockState;
use phycmd::{
    MockTransport, PhyCommander as RustPhyCommander, RtConfig, StagingError,
    StatusFrame, Transport, WriteMode as RustWriteMode,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

// -------------------------------------------------------------------------
//   WriteMode enum wrapper
// -------------------------------------------------------------------------

/// Write-conflict handling strategy. Mirrors the Rust `WriteMode`
/// enum. Can be passed to `set_default_mode(...)` and to any of the
/// setter methods as the `mode=` keyword argument.
#[pyclass(name = "WriteMode", eq, eq_int)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PyWriteMode {
    /// Overwrite the pending value with the new one. Never blocks.
    Coalesce,
    /// Block the caller until the pending value has been sent.
    BlockUntilSent,
    /// Raise a `PhycmdError` if the field is already dirty.
    ErrorOnConflict,
}

impl From<PyWriteMode> for RustWriteMode {
    fn from(m: PyWriteMode) -> Self {
        match m {
            PyWriteMode::Coalesce => RustWriteMode::Coalesce,
            PyWriteMode::BlockUntilSent => RustWriteMode::BlockUntilSent,
            PyWriteMode::ErrorOnConflict => RustWriteMode::ErrorOnConflict,
        }
    }
}

impl From<RustWriteMode> for PyWriteMode {
    fn from(m: RustWriteMode) -> Self {
        match m {
            RustWriteMode::Coalesce => PyWriteMode::Coalesce,
            RustWriteMode::BlockUntilSent => PyWriteMode::BlockUntilSent,
            RustWriteMode::ErrorOnConflict => PyWriteMode::ErrorOnConflict,
        }
    }
}

// -------------------------------------------------------------------------
//   PhyCommander wrapper
// -------------------------------------------------------------------------

/// Python-facing handle to a running PhyCommander instance.
///
/// Wraps `phycmd_rust::PhyCommander` in an `Arc<Mutex<Option<...>>>`
/// so that:
///   * `Drop` on the pyclass always tears the scheduler down cleanly
///   * dispatcher threads can keep a reference to it for the
///     `subscribe()` lifetime without preventing explicit stop
#[pyclass(name = "PhyCommander", unsendable)]
pub struct PyPhyCommander {
    inner: Arc<RustPhyCommander>,
    /// Stop flags for all dispatcher threads spawned by subscribe().
    /// Dropping the PyPhyCommander sets each to true and joins them.
    dispatchers: Mutex<Vec<DispatcherHandle>>,
}

struct DispatcherHandle {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl Drop for DispatcherHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

#[pymethods]
impl PyPhyCommander {
    /// Open a PhyCommander against a MockTransport for testing.
    ///
    /// `rate_hz` is the target tick rate, `mock_latency_us` the
    /// simulated transport round-trip latency in microseconds.
    ///
    /// This is the only constructor exposed by this first iteration
    /// of the binding — a `from_usb()` / `from_serial()` variant
    /// will land once the physerver integration of Phase C3 is in.
    #[staticmethod]
    #[pyo3(signature = (rate_hz=10_000, mock_latency_us=50, default_mode=PyWriteMode::BlockUntilSent))]
    fn open_mock(
        rate_hz: u32,
        mock_latency_us: u64,
        default_mode: PyWriteMode,
    ) -> PyResult<Self> {
        let mock_state = Arc::new(Mutex::new(MockState {
            latency: Duration::from_micros(mock_latency_us),
            ..Default::default()
        }));
        let transport: Box<dyn Transport> =
            Box::new(MockTransport::with_state(Arc::clone(&mock_state)));

        let config = RtConfig {
            rate_hz,
            default_write_mode: default_mode.into(),
            enable_rt: false,
            lock_memory: false,
            dma_latency_us: None,
            cpu_affinity: None,
            ..Default::default()
        };

        let phy = RustPhyCommander::open(config, transport)
            .map_err(|e| PyValueError::new_err(format!("open failed: {e}")))?;

        Ok(Self {
            inner: Arc::new(phy),
            dispatchers: Mutex::new(Vec::new()),
        })
    }

    // -----------------------------------------------------------------
    //   Mode configuration
    // -----------------------------------------------------------------

    fn set_default_mode(&self, mode: PyWriteMode) {
        self.inner.set_default_mode(mode.into());
    }

    fn default_mode(&self) -> PyWriteMode {
        self.inner.default_mode().into()
    }

    // -----------------------------------------------------------------
    //   Writers — each releases the GIL so a BlockUntilSent wait
    //   does not stall other Python threads.
    // -----------------------------------------------------------------

    #[pyo3(signature = (value, *, mode=None))]
    fn set_dac0(&self, py: Python<'_>, value: u16, mode: Option<PyWriteMode>) -> PyResult<()> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(move || match mode {
            Some(m) => inner.set_dac0_with(m.into(), value),
            None => inner.set_dac0(value),
        })
        .map_err(staging_err)
    }

    #[pyo3(signature = (value, *, mode=None))]
    fn set_dac1(&self, py: Python<'_>, value: u16, mode: Option<PyWriteMode>) -> PyResult<()> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(move || match mode {
            Some(m) => inner.set_dac1_with(m.into(), value),
            None => inner.set_dac1(value),
        })
        .map_err(staging_err)
    }

    #[pyo3(signature = (value, *, mode=None))]
    fn set_pwm0(&self, py: Python<'_>, value: u16, mode: Option<PyWriteMode>) -> PyResult<()> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(move || match mode {
            Some(m) => inner.set_pwm0_with(m.into(), value),
            None => inner.set_pwm0(value),
        })
        .map_err(staging_err)
    }

    #[pyo3(signature = (value, *, mode=None))]
    fn set_pwm1(&self, py: Python<'_>, value: u16, mode: Option<PyWriteMode>) -> PyResult<()> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(move || match mode {
            Some(m) => inner.set_pwm1_with(m.into(), value),
            None => inner.set_pwm1(value),
        })
        .map_err(staging_err)
    }

    #[pyo3(signature = (mask, *, mode=None))]
    fn set_digital_out(
        &self,
        py: Python<'_>,
        mask: u16,
        mode: Option<PyWriteMode>,
    ) -> PyResult<()> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(move || match mode {
            Some(m) => inner.set_digital_out_with(m.into(), mask),
            None => inner.set_digital_out(mask),
        })
        .map_err(staging_err)
    }

    #[pyo3(signature = (bit, value, *, mode=None))]
    fn set_digital_out_bit(
        &self,
        py: Python<'_>,
        bit: u8,
        value: bool,
        mode: Option<PyWriteMode>,
    ) -> PyResult<()> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(move || match mode {
            Some(m) => inner.set_digital_out_bit_with(m.into(), bit, value),
            None => inner.set_digital_out_bit(bit, value),
        })
        .map_err(staging_err)
    }

    // -----------------------------------------------------------------
    //   Observer pattern — subscribe with a Python callback
    // -----------------------------------------------------------------

    /// Subscribe a Python callable to the status bus.
    ///
    /// The callable will be invoked once per RT tick with a dict
    /// containing the full `StatusFrame` (see the module-level
    /// documentation for field descriptions).
    ///
    /// Internally a dedicated dispatcher thread is spawned. The
    /// thread consumes frames with `blocking_recv` and acquires the
    /// GIL for each callback invocation. A slow callback only
    /// delays *its own* dispatcher — the RT loop is never
    /// back-pressured, and a consumer that falls behind more than
    /// the broadcast capacity will see `Lagged` gaps in its stream.
    ///
    /// Subscribers are unregistered automatically when the
    /// `PhyCommander` is dropped or `stop()` is called.
    fn subscribe(&self, callback: PyObject) -> PyResult<()> {
        let mut rx = self.inner.subscribe();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);

        let join = thread::Builder::new()
            .name("phycmd-py-dispatch".to_string())
            .spawn(move || {
                use tokio::sync::broadcast::error::TryRecvError;
                while !stop_clone.load(Ordering::Acquire) {
                    match rx.try_recv() {
                        Ok(frame) => {
                            Python::with_gil(|py| {
                                let dict = frame_to_pydict(py, &frame);
                                if let Ok(d) = dict {
                                    let _ = callback.call1(py, (d,));
                                }
                            });
                        }
                        Err(TryRecvError::Empty) => {
                            // Nothing yet; brief yield. This is the
                            // tradeoff for not needing an async runtime
                            // inside the dispatcher.
                            thread::sleep(Duration::from_micros(200));
                        }
                        Err(TryRecvError::Lagged(_)) => continue,
                        Err(TryRecvError::Closed) => break,
                    }
                }
            })
            .map_err(|e| PyRuntimeError::new_err(format!("spawn dispatcher: {e}")))?;

        self.dispatchers.lock().push(DispatcherHandle {
            stop,
            join: Some(join),
        });
        Ok(())
    }

    /// Number of currently active bus subscribers (visible to the
    /// Rust scheduler).
    fn subscriber_count(&self) -> usize {
        self.inner.subscriber_count()
    }

    // -----------------------------------------------------------------
    //   Stats
    // -----------------------------------------------------------------

    fn stats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let snap = self.inner.stats();
        let d = PyDict::new_bound(py);
        d.set_item("tick_count", snap.tick_count)?;
        d.set_item("tick_ok", snap.tick_ok)?;
        d.set_item("missed_ticks", snap.missed_ticks)?;
        d.set_item("transport_errors", snap.transport_errors)?;
        d.set_item("latency_max_us", snap.latency_max_us)?;
        d.set_item("mean_latency_us", snap.mean_latency_us)?;
        d.set_item("jitter_min_us", snap.jitter_min_us)?;
        d.set_item("jitter_max_us", snap.jitter_max_us)?;
        d.set_item("mean_abs_jitter_us", snap.mean_abs_jitter_us)?;
        let hist = PyList::new_bound(py, snap.jitter_histogram);
        d.set_item("jitter_histogram", hist)?;
        Ok(d)
    }

    fn reset_stats(&self) {
        self.inner.reset_stats();
    }

    // -----------------------------------------------------------------
    //   Lifecycle
    // -----------------------------------------------------------------

    /// Signal the scheduler to stop. Does not wait.
    fn stop(&self) {
        self.inner.stop();
    }

    /// `True` while the scheduler thread is still running.
    fn is_running(&self) -> bool {
        self.inner.is_running()
    }
}

impl Drop for PyPhyCommander {
    fn drop(&mut self) {
        // Stop all dispatcher threads first so they stop touching
        // the broadcast channel.
        self.dispatchers.lock().clear();
        // The inner PhyCommander is dropped automatically once its
        // final Arc reference goes away.
        self.inner.stop();
    }
}

// -------------------------------------------------------------------------
//   Helpers
// -------------------------------------------------------------------------

/// Convert a StagingError into a Python ValueError.
fn staging_err(e: StagingError) -> PyErr {
    PyValueError::new_err(format!("{e}"))
}

/// Convert a StatusFrame into a Python dict.
fn frame_to_pydict<'py>(
    py: Python<'py>,
    frame: &StatusFrame,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("cmd_seq", frame.cmd_seq)?;
    d.set_item("wire_seq", frame.wire_seq)?;
    d.set_item("tick_index", frame.tick_index)?;
    d.set_item("tick_expected_ns", frame.tick_expected_ns)?;
    d.set_item("tick_sent_ns", frame.tick_sent_ns)?;
    d.set_item("tick_recv_ns", frame.tick_recv_ns)?;
    d.set_item("latency_us", frame.latency_us)?;
    d.set_item("jitter_us", frame.jitter_us)?;
    d.set_item("missed_ticks_prior", frame.missed_ticks_prior)?;

    let st = PyDict::new_bound(py);
    st.set_item("digital_in", frame.status.digital_in)?;
    st.set_item("digital_out", frame.status.digital_out)?;
    let adc = PyList::new_bound(py, frame.status.adc);
    st.set_item("adc", adc)?;
    st.set_item("seq_num", frame.status.seq_num)?;
    st.set_item("loop_time_us", frame.status.loop_time_us)?;
    st.set_item("uptime_ms", frame.status.uptime_ms)?;
    st.set_item("error_count", frame.status.error_count)?;
    d.set_item("status", st)?;

    Ok(d)
}

// -------------------------------------------------------------------------
//   Module entry point
// -------------------------------------------------------------------------

/// pyo3 module entry point. The function name matches the cdylib
/// name declared in Cargo.toml (`phycmd_ext`) so pyo3 generates
/// `PyInit_phycmd_ext`. The user-facing `phycmd` Python package
/// imports from `.phycmd_ext` — see `python/phycmd/__init__.py`.
mod waveform;

#[pymodule]
fn phycmd_ext(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPhyCommander>()?;
    m.add_class::<PyWriteMode>()?;
    waveform::register(m)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
