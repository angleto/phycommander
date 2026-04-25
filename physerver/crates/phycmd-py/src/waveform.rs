//! pyo3 bindings for the on-chip function generator (vendor SETUP
//! plane — see `docs/firmware/PROTOCOL.md`).
//!
//! Exposes a single [`PyWaveformDev`] class that owns its own libusb
//! context and issues control transfers on EP0. Standalone — does
//! NOT require an iso scheduler, so scripts that only want to drive
//! the firmware fn-gen don't have to bring up the full RT stack.
//!
//! Python example:
//!
//! ```python
//! import phycmd
//! with phycmd.WaveformDev.open() as dev:
//!     dev.caps()
//!     dev.play_sine("dac0", freq_hz=1000, amplitude=4000, offset=2048)
//!     print(dev.state("dac0"))
//!     dev.stop("dac0")
//! ```

use std::{os::raw::c_int, ptr};

use libusb1_sys as ffi;
use phycmd_core::protocol::wave_types::*;
use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
    types::PyDict,
};

const VID: u16 = 0x2341;
const PID: u16 = 0x003e;
const INTERFACE: c_int = 0;

/// Python-facing wrapper around a libusb device handle that owns its
/// own context (independent of any IsoTransport).
#[pyclass(name = "WaveformDev", module = "phycmd", unsendable)]
pub struct PyWaveformDev {
    ctx: *mut ffi::libusb_context,
    dev_handle: *mut ffi::libusb_device_handle,
}

// SAFETY: libusb is internally synchronised; we only use EP0 control
// transfers from a single Python thread (pyo3's GIL serialises calls).
unsafe impl Send for PyWaveformDev {}

impl Drop for PyWaveformDev {
    fn drop(&mut self) {
        unsafe {
            if !self.dev_handle.is_null() {
                let _ = ffi::libusb_release_interface(self.dev_handle, INTERFACE);
                ffi::libusb_close(self.dev_handle);
            }
            if !self.ctx.is_null() {
                ffi::libusb_exit(self.ctx);
            }
        }
    }
}

fn libusb_err(rc: i32, hint: &str) -> PyErr {
    if rc == ffi::constants::LIBUSB_ERROR_PIPE {
        PyRuntimeError::new_err(format!(
            "{hint}: device STALLed the control transfer — check parameters against PROTOCOL.md \
             §2.5 error validation order"
        ))
    } else {
        PyRuntimeError::new_err(format!("{hint}: libusb error {rc}"))
    }
}

impl PyWaveformDev {
    fn ctrl_in(&self, b_request: u8, w_index: u16, length: u16) -> PyResult<Vec<u8>> {
        let mut buf = vec![0u8; length as usize];
        let n = unsafe {
            ffi::libusb_control_transfer(
                self.dev_handle,
                0xC0,
                b_request,
                0,
                w_index,
                buf.as_mut_ptr(),
                length,
                1000,
            )
        };
        if n < 0 {
            return Err(libusb_err(n, &format!("control_transfer IN bRequest=0x{b_request:02x}")));
        }
        buf.truncate(n as usize);
        Ok(buf)
    }

    fn ctrl_out(&self, b_request: u8, w_index: u16, data: &[u8]) -> PyResult<()> {
        let n = unsafe {
            ffi::libusb_control_transfer(
                self.dev_handle,
                0x40,
                b_request,
                0,
                w_index,
                data.as_ptr() as *mut u8,
                data.len() as u16,
                1000,
            )
        };
        if n < 0 {
            return Err(libusb_err(n, &format!("control_transfer OUT bRequest=0x{b_request:02x}")));
        }
        if n as usize != data.len() {
            return Err(PyRuntimeError::new_err(format!(
                "short control write: sent {} of {} bytes",
                n,
                data.len()
            )));
        }
        Ok(())
    }

    fn channel_id(name: &str) -> PyResult<u16> {
        channel_id_from_name(name).ok_or_else(|| {
            PyValueError::new_err(format!(
                "unknown channel {name:?} — use dac0..1, pwm0..7, dout0..15, din0..15, adc0..7"
            ))
        })
    }

    fn shape_code(name: &str) -> PyResult<u8> {
        match name.to_ascii_lowercase().as_str() {
            "dc" => Ok(1),
            "sine" => Ok(2),
            "square" => Ok(3),
            "triangle" => Ok(4),
            "sawtooth" => Ok(5),
            _ => Err(PyValueError::new_err(format!("unknown shape {name:?}"))),
        }
    }
}

#[pymethods]
impl PyWaveformDev {
    /// Open the device (VID 0x2341 PID 0x003e), claim the interface.
    /// Raises RuntimeError if the device is missing or held by
    /// another process (e.g. physerver).
    #[staticmethod]
    fn open() -> PyResult<Self> {
        unsafe {
            let mut ctx: *mut ffi::libusb_context = ptr::null_mut();
            let r = ffi::libusb_init(&mut ctx);
            if r != 0 {
                return Err(PyRuntimeError::new_err(format!("libusb_init failed: {r}")));
            }
            let dh = ffi::libusb_open_device_with_vid_pid(ctx, VID, PID);
            if dh.is_null() {
                ffi::libusb_exit(ctx);
                return Err(PyRuntimeError::new_err(
                    "Arduino Due not found. Is physerver holding the interface? Try: sudo \
                     systemctl stop physerver",
                ));
            }
            let _ = ffi::libusb_detach_kernel_driver(dh, INTERFACE);
            let r = ffi::libusb_claim_interface(dh, INTERFACE);
            if r != 0 {
                ffi::libusb_close(dh);
                ffi::libusb_exit(ctx);
                return Err(PyRuntimeError::new_err(format!("claim_interface failed: {r}")));
            }
            Ok(Self { ctx, dev_handle: dh })
        }
    }

    /// Context manager support: `with phycmd.WaveformDev.open() as dev: ...`
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __exit__(&mut self, _t: PyObject, _v: PyObject, _tb: PyObject) -> PyResult<bool> {
        // Drop will run when Python releases the last reference.
        Ok(false)
    }

    /// Return the device Capabilities struct as a dict.
    fn caps(&self, py: Python<'_>) -> PyResult<PyObject> {
        let raw = self.ctrl_in(VREQ_GEN_GET_CAPS, 0, 32)?;
        if raw.len() < 32 {
            return Err(PyRuntimeError::new_err(format!("short Capabilities: {} B", raw.len())));
        }
        let c: Capabilities = unsafe { ptr::read_unaligned(raw.as_ptr() as *const _) };
        let d = PyDict::new_bound(py);
        d.set_item("protocol_version", c.protocol_version)?;
        d.set_item("firmware_major", c.firmware_major)?;
        d.set_item("firmware_minor", c.firmware_minor)?;
        d.set_item("num_dac", c.num_dac)?;
        d.set_item("num_pwm", c.num_pwm)?;
        d.set_item("num_dout", c.num_dout)?;
        d.set_item("num_din", c.num_din)?;
        d.set_item("num_adc", c.num_adc)?;
        d.set_item("modes_dac", c.modes_dac)?;
        d.set_item("modes_pwm", c.modes_pwm)?;
        d.set_item("modes_dout", c.modes_dout)?;
        d.set_item("modes_din", c.modes_din)?;
        d.set_item("modes_adc", c.modes_adc)?;
        d.set_item("max_dac_sample_rate_hz", c.max_dac_sample_rate_hz)?;
        d.set_item("max_arb_buffer_samples", c.max_arb_buffer_samples)?;
        Ok(d.into())
    }

    /// Return the per-channel ChannelState struct as a dict.
    fn state(&self, py: Python<'_>, channel: &str) -> PyResult<PyObject> {
        let id = Self::channel_id(channel)?;
        let raw = self.ctrl_in(VREQ_GEN_GET_STATE, id, 32)?;
        if raw.len() < 32 {
            return Err(PyRuntimeError::new_err(format!("short ChannelState: {} B", raw.len())));
        }
        let s: ChannelState = unsafe { ptr::read_unaligned(raw.as_ptr() as *const _) };
        let d = PyDict::new_bound(py);
        d.set_item("channel_kind", s.channel_kind)?;
        d.set_item("channel_index", s.channel_index)?;
        d.set_item("shape", s.shape)?;
        d.set_item(
            "shape_name",
            match s.shape {
                0 => "off",
                1 => "dc",
                2 => "sine",
                3 => "square",
                4 => "triangle",
                5 => "sawtooth",
                6 => "arbitrary",
                16 => "lut",
                17 => "threshold",
                18 => "pulse_trig",
                19 => "pid",
                _ => "unknown",
            },
        )?;
        d.set_item("freq_mhz", s.freq_mhz)?;
        d.set_item("duty_x10", s.duty_x10)?;
        d.set_item("amplitude", s.amplitude)?;
        d.set_item("offset", s.offset)?;
        d.set_item("phase_offset_x16", s.phase_offset_x16)?;
        d.set_item("arb_n_samples", s.arb_n_samples)?;
        d.set_item("arb_loops_remaining", s.arb_loops_remaining)?;
        d.set_item("arb_sample_rate_hz", s.arb_sample_rate_hz)?;
        d.set_item("cur_phase_q24_8", s.cur_phase_q24_8)?;
        Ok(d.into())
    }

    /// Stop any generator on the channel and return it to MANUAL.
    fn stop(&self, channel: &str) -> PyResult<()> {
        let id = Self::channel_id(channel)?;
        self.ctrl_out(VREQ_GEN_STOP, id, &[])
    }

    /// Get/set the shared DAC sample clock.
    fn dac_get_clock(&self) -> PyResult<u32> {
        let raw = self.ctrl_in(VREQ_DAC_GET_CLOCK, 0, 4)?;
        Ok(u32::from_le_bytes(raw[..4].try_into().unwrap()))
    }

    fn dac_set_clock(&self, hz: u32) -> PyResult<()> {
        self.ctrl_out(VREQ_DAC_SET_CLOCK, 0, &hz.to_le_bytes())
    }

    /// Get/set the ADC sample rate.
    fn adc_get_rate(&self) -> PyResult<u32> {
        let raw = self.ctrl_in(VREQ_ADC_GET_RATE, 0, 4)?;
        Ok(u32::from_le_bytes(raw[..4].try_into().unwrap()))
    }

    fn adc_set_rate(&self, hz: u32) -> PyResult<()> {
        self.ctrl_out(VREQ_ADC_SET_RATE, 0, &hz.to_le_bytes())
    }

    /// Play a built-in waveform. `shape` is one of:
    /// "dc", "sine", "square", "triangle", "sawtooth".
    #[pyo3(signature = (channel, shape, freq_hz=1000.0, amplitude=4000, offset=2048, duty=0.5))]
    fn play_builtin(
        &self,
        channel: &str,
        shape: &str,
        freq_hz: f64,
        amplitude: u16,
        offset: u16,
        duty: f32,
    ) -> PyResult<()> {
        let id = Self::channel_id(channel)?;
        let shape_code = Self::shape_code(shape)?;
        let spec = WaveBuiltinSpec {
            shape: shape_code,
            flags: 0,
            duty_x10: (duty.clamp(0.0, 1.0) * 1000.0).round() as u16,
            amplitude,
            offset,
            freq_mhz: (freq_hz * 1000.0).round() as u32,
            phase_offset_x16: 0,
            _reserved1: 0,
        };
        let bytes = unsafe { std::slice::from_raw_parts(&spec as *const _ as *const u8, 16) };
        self.ctrl_out(VREQ_GEN_PLAY_BUILTIN, id, bytes)
    }

    /// Convenience shortcuts.
    #[pyo3(signature = (channel, freq_hz=1000.0, amplitude=4000, offset=2048))]
    fn play_sine(&self, channel: &str, freq_hz: f64, amplitude: u16, offset: u16) -> PyResult<()> {
        self.play_builtin(channel, "sine", freq_hz, amplitude, offset, 0.5)
    }

    #[pyo3(signature = (channel, freq_hz=1000.0, amplitude=4000, offset=2048, duty=0.5))]
    fn play_square(
        &self,
        channel: &str,
        freq_hz: f64,
        amplitude: u16,
        offset: u16,
        duty: f32,
    ) -> PyResult<()> {
        self.play_builtin(channel, "square", freq_hz, amplitude, offset, duty)
    }

    #[pyo3(signature = (channel, offset=2048))]
    fn play_dc(&self, channel: &str, offset: u16) -> PyResult<()> {
        self.play_builtin(channel, "dc", 0.0, 0, offset, 0.5)
    }

    /// Upload an arbitrary waveform (up to 1024 int16 DAC codes).
    #[pyo3(signature = (channel, samples, sample_rate_hz, loop_count=0))]
    fn play_arbitrary(
        &self,
        channel: &str,
        samples: Vec<i16>,
        sample_rate_hz: u32,
        loop_count: u16,
    ) -> PyResult<()> {
        let id = Self::channel_id(channel)?;
        if samples.is_empty() || samples.len() > 1024 {
            return Err(PyValueError::new_err("samples length must be 1..1024"));
        }
        let header = WaveArbHeader { n_samples: samples.len() as u16, loop_count, sample_rate_hz };
        let mut payload = Vec::with_capacity(8 + samples.len() * 2);
        payload.extend_from_slice(unsafe {
            std::slice::from_raw_parts(&header as *const _ as *const u8, 8)
        });
        for s in &samples {
            payload.extend_from_slice(&s.to_le_bytes());
        }
        self.ctrl_out(VREQ_GEN_PLAY_ARBITRARY, id, &payload)
    }

    /// Reactive LUT mode. `input_src` is "adc" or "din".
    #[pyo3(signature = (channel, input_src, input_arg, entries, output_mask=0))]
    fn play_lut(
        &self,
        channel: &str,
        input_src: &str,
        input_arg: u16,
        entries: Vec<i16>,
        output_mask: u16,
    ) -> PyResult<()> {
        let id = Self::channel_id(channel)?;
        if entries.is_empty() || entries.len() > 4096 {
            return Err(PyValueError::new_err("entries length must be 1..4096"));
        }
        let src = match input_src.to_ascii_lowercase().as_str() {
            "adc" => 1,
            "din" => 2,
            _ => return Err(PyValueError::new_err("input_src must be \"adc\" or \"din\"")),
        };
        let header = WaveLutSpec {
            input_src: src,
            _reserved0: 0,
            input_arg,
            n_entries: entries.len() as u16,
            output_mask,
            _reserved1: 0,
        };
        let mut payload = Vec::with_capacity(12 + entries.len() * 2);
        payload.extend_from_slice(unsafe {
            std::slice::from_raw_parts(&header as *const _ as *const u8, 12)
        });
        for e in &entries {
            payload.extend_from_slice(&e.to_le_bytes());
        }
        self.ctrl_out(VREQ_GEN_PLAY_LUT, id, &payload)
    }

    /// Reactive threshold comparator with hysteresis.
    #[pyo3(signature = (channel, input_src, input_arg, thr_high, thr_low, val_high, val_low))]
    fn play_threshold(
        &self,
        channel: &str,
        input_src: &str,
        input_arg: u16,
        thr_high: u16,
        thr_low: u16,
        val_high: u16,
        val_low: u16,
    ) -> PyResult<()> {
        let id = Self::channel_id(channel)?;
        let src = match input_src.to_ascii_lowercase().as_str() {
            "adc" => 1,
            "din" => 2,
            _ => return Err(PyValueError::new_err("input_src must be \"adc\" or \"din\"")),
        };
        let spec = WaveThresholdSpec {
            input_src: src,
            _reserved0: 0,
            input_arg,
            thr_high,
            thr_low,
            val_high,
            val_low,
            _reserved1: 0,
        };
        let bytes = unsafe { std::slice::from_raw_parts(&spec as *const _ as *const u8, 16) };
        self.ctrl_out(VREQ_GEN_PLAY_THRESHOLD, id, bytes)
    }

    /// Reactive pulse-trigger (DOUT only in v1 firmware).
    #[pyo3(signature = (channel, din_bit, edge="rising", active_level=1, duration_us=1000, cooldown_us=0))]
    fn play_pulse_trig(
        &self,
        channel: &str,
        din_bit: u8,
        edge: &str,
        active_level: u8,
        duration_us: u32,
        cooldown_us: u32,
    ) -> PyResult<()> {
        let id = Self::channel_id(channel)?;
        let edge_v = match edge {
            "rising" => 1,
            "falling" => 2,
            "any" => 3,
            _ => return Err(PyValueError::new_err("edge must be rising|falling|any")),
        };
        let spec = WavePulseSpec {
            input_din_bit: din_bit,
            edge: edge_v,
            active_level,
            _reserved0: 0,
            duration_us,
            cooldown_us,
        };
        let bytes = unsafe { std::slice::from_raw_parts(&spec as *const _ as *const u8, 12) };
        self.ctrl_out(VREQ_GEN_PLAY_PULSE_TRIG, id, bytes)
    }

    /// Reactive PID closed-loop on DAC channels.
    #[pyo3(signature = (channel, input_arg, setpoint, kp, ki=0.0, kd=0.0,
                        sample_rate_hz=1000, out_min=0, out_max=4095, integral_clamp=1_000_000))]
    #[allow(clippy::too_many_arguments)]
    fn play_pid(
        &self,
        channel: &str,
        input_arg: u16,
        setpoint: i32,
        kp: f32,
        ki: f32,
        kd: f32,
        sample_rate_hz: u32,
        out_min: u16,
        out_max: u16,
        integral_clamp: i32,
    ) -> PyResult<()> {
        let id = Self::channel_id(channel)?;
        let q16 = |f: f32| (f * 65536.0).round() as i32;
        let spec = WavePidSpec {
            input_src: 1,
            _reserved0: 0,
            input_arg,
            sample_rate_hz,
            setpoint,
            kp_q16_16: q16(kp),
            ki_q16_16: q16(ki),
            kd_q16_16: q16(kd),
            out_min,
            out_max,
            integral_clamp,
        };
        let bytes = unsafe { std::slice::from_raw_parts(&spec as *const _ as *const u8, 32) };
        self.ctrl_out(VREQ_GEN_PLAY_PID, id, bytes)
    }

    fn __repr__(&self) -> String {
        format!("WaveformDev(VID=0x{VID:04x}, PID=0x{PID:04x}, interface={INTERFACE})")
    }
}

/// Register the module with pyo3.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyWaveformDev>()?;
    Ok(())
}
