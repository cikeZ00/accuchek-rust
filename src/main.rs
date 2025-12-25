//! Accu-Chek USB Data Downloader
//! 
//! Downloads blood glucose samples from Roche Accu-Chek devices using USB.
//! Uses the IEEE 11073 Personal Health Device (PHD) protocol.
//!
//! Cross-platform: Works on Windows and Linux.
//!
//! Usage:
//!   ACCUCHEK_DBG=1 accuchek [device_index]
//!
//! On Linux, requires root privileges. On Windows, requires proper USB driver (WinUSB/libusb).

mod protocol;
mod device;
mod config;
mod error;

use std::env;
use log::{info, warn};
use crate::device::find_and_operate_accuchek;
use crate::config::Config;
use crate::error::AccuChekError;

fn main() -> Result<(), AccuChekError> {
    // Check for debug mode
    let debug_mode = env::var("ACCUCHEK_DBG").is_ok();
    
    // Initialize logger
    if debug_mode {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp(None)
            .init();
    }

    // On Unix, check for root privileges (not needed on Windows with proper driver)
    #[cfg(unix)]
    check_root_privileges()?;

    // Load config file
    let config = Config::load("config.txt").unwrap_or_else(|e| {
        warn!("Could not load config.txt: {}. Using defaults.", e);
        Config::default()
    });

    info!("Starting Accu-Chek downloader");

    // Parse device index from command line
    let device_index: Option<usize> = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok());

    // Initialize libusb context
    let context = rusb::Context::new()?;
    
    // Find and operate the device
    let readings = find_and_operate_accuchek(&context, &config, device_index)?;

    // Output readings as JSON
    let json = serde_json::to_string_pretty(&readings)?;
    println!("{}", json);

    info!("Done");
    Ok(())
}

#[cfg(unix)]
fn check_root_privileges() -> Result<(), AccuChekError> {
    // Use nix crate or raw syscall to check euid
    // For simplicity, we'll skip this check and let USB access fail with appropriate error
    // Users can run with sudo if needed
    Ok(())
}
