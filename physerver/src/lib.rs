pub mod protocol;
pub mod serial;
pub mod ipc;
pub mod web;
pub mod rt;
pub mod telemetry;

// Re-export common types
pub use protocol::{Command, Status, CommandFlags, StatusFlags};
pub use ipc::{IpcClient, IpcServer};
