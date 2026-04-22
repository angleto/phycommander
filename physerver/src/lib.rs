// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2014-2026 Angelo Leto <angelo@leto.blue>

//! `physerver` crate — the HTTP/IPC service layer of PhyCommander.
//!
//! The protocol, transport, and real-time primitives used to live in
//! this crate directly; they have now been extracted into the sibling
//! `phycmd-core` crate. This module re-exports them under the legacy
//! paths (`physerver::protocol`, `physerver::transport`, `physerver::rt`)
//! so that integration tests, benches, and external consumers keep
//! working unchanged while the rest of the workspace is refactored.

// Re-exports from the new core crate
pub use phycmd_core::protocol;
pub use phycmd_core::rt_setup as rt;
pub use phycmd_core::transport;

// Service-only modules (HTTP, IPC, system telemetry, config, ...)
pub mod config;
pub mod ipc;
pub mod sysinfo;
pub mod telemetry;
pub mod web;

// Legacy serial discovery helper (distinct from phycmd_core::transport::serial
// — this one contains `SerialPortHandler::find_phycmd_device` used by the
// auto-detect path in the service binary).
pub mod serial;

// Re-export common types at the crate root for ergonomic use.
pub use config::Config;
pub use ipc::{IpcClient, IpcServer};
pub use protocol::{Command, CommandFlags, Status, StatusFlags};
#[cfg(feature = "usb")]
pub use transport::UsbTransport;
pub use transport::{SerialTransport, Transport, TransportType};
