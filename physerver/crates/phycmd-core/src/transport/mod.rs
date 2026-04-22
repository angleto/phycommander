pub mod serial;
pub mod traits;
#[cfg(feature = "usb")]
pub mod usb;
#[cfg(feature = "usb")]
pub mod usb_iso;
#[cfg(feature = "usb")]
pub mod usb_loopback;
#[cfg(feature = "usb")]
pub mod usb_loopback_pipelined;

#[cfg(any(test, feature = "test-mock"))]
pub mod mock;

#[cfg(any(test, feature = "test-mock"))]
pub use mock::{MockPipelinedTransport, MockState, MockTransport};
pub use serial::SerialTransport;
pub use traits::{PipelinedTransport, Transport, TransportStats};
#[cfg(feature = "usb")]
pub use usb::UsbTransport;
#[cfg(feature = "usb")]
pub use usb_iso::{
    CapabilitiesView, ChannelStateView, IsoStats, IsoStatsSnapshot, IsoTransport, WaveformDevice,
    WaveformError,
};
#[cfg(feature = "usb")]
pub use usb_loopback::UsbLoopbackTransport;
#[cfg(feature = "usb")]
pub use usb_loopback_pipelined::PipelinedUsbLoopbackTransport;

use anyhow::Result;

/// Transport type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// USB CDC (virtual serial port) - Compatible, cross-platform
    Serial,
    /// Direct USB bulk transfer - Lowest latency, highest throughput
    #[cfg(feature = "usb")]
    Usb,
}

impl TransportType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "serial" | "cdc" | "tty" => Some(Self::Serial),
            #[cfg(feature = "usb")]
            "usb" | "libusb" | "bulk" => Some(Self::Usb),
            _ => None,
        }
    }
}

/// Create a transport based on type
pub fn create_transport(
    transport_type: TransportType,
    device_path: &str,
    baud_rate: u32,
) -> Result<Box<dyn Transport>> {
    match transport_type {
        TransportType::Serial => Ok(Box::new(SerialTransport::new(device_path, baud_rate)?)),
        #[cfg(feature = "usb")]
        TransportType::Usb => Ok(Box::new(UsbTransport::new()?)),
    }
}

/// Auto-detect best available transport
pub fn detect_transport() -> Result<Box<dyn Transport>> {
    // Try USB first (better performance) if available
    #[cfg(feature = "usb")]
    {
        if let Ok(transport) = UsbTransport::new() {
            tracing::info!("Using direct USB transport");
            return Ok(Box::new(transport));
        }
    }

    // Fall back to serial
    if let Ok(devices) = SerialTransport::available_devices() {
        for device in devices {
            if let Ok(transport) = SerialTransport::new(&device, 921600) {
                tracing::info!("Using serial transport on {}", device);
                return Ok(Box::new(transport));
            }
        }
    }

    anyhow::bail!("No compatible transport found")
}
