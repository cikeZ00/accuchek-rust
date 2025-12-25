//! Error types for the Accu-Chek application

use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum AccuChekError {
    #[error("USB error: {0}")]
    Usb(#[from] rusb::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Permission denied - must run as root/administrator")]
    PermissionDenied,

    #[error("No Accu-Chek device found")]
    NoDeviceFound,

    #[error("Device not in whitelist: vendor=0x{0:04x}, product=0x{1:04x}")]
    DeviceNotWhitelisted(u16, u16),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Device communication error: {0}")]
    Communication(String),

    #[error("Invalid device index: {0}")]
    InvalidDeviceIndex(usize),

    #[error("Empty data segment")]
    EmptyDataSegment,

    #[error("Association aborted by device")]
    AssociationAborted,

    #[error("Unexpected response from device")]
    UnexpectedResponse,

    #[error("Config parse error: {0}")]
    ConfigParse(String),
}
