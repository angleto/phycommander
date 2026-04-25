// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2014-2026 Angelo Leto <angelo@leto.blue>

//! phycmd-core — Core protocol, transport, and real-time primitives.
//!
//! This crate contains the parts of PhyCommander that are independent of
//! the HTTP service, the IPC shared-memory layer, and any tokio runtime:
//!
//!   * [`protocol`]  — PhyCMD-64 message types, CRC, encode/decode.
//!   * [`transport`] — Abstract [`transport::Transport`] trait plus the concrete
//!     [`transport::SerialTransport`] and (behind the `usb` feature) [`transport::UsbTransport`]
//!     implementations.
//!   * [`rt_setup`]  — Linux real-time primitives: `SCHED_FIFO` priority, `mlockall`, CPU affinity,
//!     `/dev/cpu_dma_latency`.
//!
//! Higher-level pieces that will be added in later phases of the
//! hard-real-time rework (staging buffer with write-mode semantics,
//! status bus with the observer pattern, hard-paced `RtScheduler`,
//! stats/jitter accounting, pyo3 bindings) live in sibling crates
//! that depend on this one.

pub mod config;
pub mod protocol;
pub mod rt_setup;
pub mod scheduler;
pub mod staging;
pub mod stats;
pub mod status_bus;
pub mod transport;
pub mod waveforms;

// Common re-exports to keep downstream users' imports short.
pub use config::{RtConfig, RtConfigError};
pub use protocol::{
    channel_id, channel_id_from_name, Capabilities, ChannelKind, ChannelState, Command,
    CommandFlags, InputSrc, PulseEdge, Status, StatusFlags, WaveArbHeader, WaveBuiltinSpec,
    WaveLutSpec, WavePidSpec, WavePulseSpec, WaveShape, WaveThresholdSpec, MESSAGE_SIZE,
};
pub use scheduler::{RtScheduler, RtSchedulerStopHandle};
pub use staging::{CommandStaging, StagingError, WriteMode};
pub use stats::{RtStats, RtStatsSnapshot, JITTER_BUCKET_BOUNDS_US, JITTER_NUM_BUCKETS};
pub use status_bus::{StatusBus, StatusFrame};
#[cfg(feature = "usb")]
pub use transport::UsbTransport;
pub use transport::{
    PipelinedTransport, SerialTransport, Transport, TransportStats, TransportType,
};
pub use waveforms::{WaveformBank, WaveformBankSnapshot, WaveformShape, WaveformSpec};
