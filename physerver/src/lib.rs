pub mod protocol;
pub mod transport;
pub mod ipc;
pub mod sysinfo;
pub mod web;
pub mod rt;
pub mod telemetry;
pub mod config;

// Legacy serial module (deprecated, use transport::serial instead)
pub mod serial {
    pub use crate::transport::serial::*;
}

// Re-export common types
pub use protocol::{Command, Status, CommandFlags, StatusFlags};
pub use ipc::{IpcClient, IpcServer};
pub use transport::{Transport, TransportType, SerialTransport};
#[cfg(feature = "usb")]
pub use transport::UsbTransport;
pub use config::Config;
