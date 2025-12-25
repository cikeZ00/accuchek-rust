//! Accu-Chek USB Data Downloader
//! 
//! Downloads blood glucose samples from Roche Accu-Chek devices using USB.
//! Uses the IEEE 11073 Personal Health Device (PHD) protocol.
//!
//! Cross-platform: Works on Windows and Linux.
//!
//! Usage:
//!   accuchek              - Launch GUI
//!   accuchek sync         - Download from device (CLI mode)
//!   accuchek --help       - Show help
//!   ACCUCHEK_DBG=1 accuchek sync - Enable debug output
//!
//! On Linux, requires root privileges. On Windows, requires proper USB driver (WinUSB/libusb).

// Hide console window on Windows when running GUI mode (doesn't affect CLI when run from terminal)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod protocol;
mod device;
mod config;
mod error;
mod storage;
mod gui;
mod export;

use std::env;
use log::{info, warn};
use crate::device::find_and_operate_accuchek;
use crate::config::{Config, default_database_path, ensure_data_dir, config_file_path};
use crate::error::AccuChekError;
use crate::storage::Storage;

/// Attach to parent console on Windows (needed for CLI output with windows_subsystem = "windows")
/// This redirects stdout/stderr to the parent console when running from a terminal.
#[cfg(windows)]
fn attach_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
    }
    
    #[link(name = "msvcrt")]
    extern "C" {
        fn freopen(filename: *const i8, mode: *const i8, stream: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn __acrt_iob_func(index: u32) -> *mut std::ffi::c_void;
    }
    
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFFFFFF;
    
    unsafe {
        // Try to attach to the parent console (e.g., PowerShell, cmd)
        if AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
            let conout = b"CONOUT$\0".as_ptr() as *const i8;
            let mode_w = b"w\0".as_ptr() as *const i8;
            
            // Redirect stdout (stream 1) and stderr (stream 2) to console
            freopen(conout, mode_w, __acrt_iob_func(1)); // stdout
            freopen(conout, mode_w, __acrt_iob_func(2)); // stderr
        }
    }
}

#[cfg(not(windows))]
fn attach_console() {
    // No-op on non-Windows platforms
}

fn main() -> Result<(), AccuChekError> {
    let args: Vec<String> = env::args().collect();
    
    // Check if we're in CLI mode (any arguments passed)
    let cli_mode = args.len() > 1;
    
    // Attach to parent console on Windows for CLI output
    if cli_mode {
        attach_console();
    }
    
    // Check for debug mode
    let debug_mode = env::var("ACCUCHEK_DBG").is_ok();
    
    // Initialize logger
    if debug_mode {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp(None)
            .init();
    }

    // Ensure data directory exists
    if let Err(e) = ensure_data_dir() {
        eprintln!("Warning: Could not create data directory: {}", e);
    }

    // Create default config if it doesn't exist
    let cfg_path = config_file_path();
    if !cfg_path.exists() {
        if let Err(e) = Config::create_default(&cfg_path) {
            if debug_mode {
                warn!("Could not create default config: {}", e);
            }
        }
    }

    // Try loading config from data directory first, then current directory
    let config = Config::load(config_file_path())
        .or_else(|_| Config::load("config.txt"))
        .unwrap_or_else(|e| {
            if debug_mode {
                warn!("Could not load config: {}. Using defaults.", e);
            }
            Config::default()
        });

    // Use configured path or default OS-specific path
    let db_path = config.database_path
        .clone()
        .unwrap_or_else(|| default_database_path().to_string_lossy().to_string());

    // Parse command
    match args.get(1).map(|s| s.as_str()) {
        Some("sync") | Some("download") => {
            // CLI sync mode
            cmd_sync(&config, &db_path, args.get(2))?;
        }
        Some("--help") | Some("-h") | Some("help") => {
            print_help();
        }
        Some("--version") | Some("-V") => {
            println!("accuchek {}", env!("CARGO_PKG_VERSION"));
        }
        Some("path") | Some("paths") => {
            cmd_show_paths();
        }
        _ => {
            // Default: launch GUI
            gui::run_gui(db_path).map_err(|e| {
                AccuChekError::Communication(format!("GUI error: {}", e))
            })?;
        }
    }

    Ok(())
}

/// Show data paths
fn cmd_show_paths() {
    use crate::config::{get_data_dir, default_export_dir};
    
    println!("Accu-Chek Data Paths:");
    println!("  Data directory:  {}", get_data_dir().display());
    println!("  Database:        {}", default_database_path().display());
    println!("  Config file:     {}", config_file_path().display());
    println!("  Export default:  {}", default_export_dir().display());
}

/// Sync from device (CLI mode)
fn cmd_sync(config: &Config, db_path: &str, device_index: Option<&String>) -> Result<(), AccuChekError> {
    // On Unix, check for root privileges (not needed on Windows with proper driver)
    #[cfg(unix)]
    check_root_privileges()?;

    info!("Starting Accu-Chek downloader");

    let device_index: Option<usize> = device_index.and_then(|s| s.parse().ok());

    // Initialize libusb context
    let context = rusb::Context::new()?;
    
    // Find and operate the device
    let readings = find_and_operate_accuchek(&context, config, device_index)?;

    // Save to database
    let storage = Storage::new(db_path)?;
    let new_count = storage.import_readings(&readings)?;
    let total_count = storage.count()?;
    let skipped_count = readings.len() - new_count;
    
    info!("Imported {} new readings ({} from device, {} total in database)", 
          new_count, readings.len(), total_count);

    // Always print summary (not just in debug mode)
    eprintln!("Downloaded {} readings from device", readings.len());
    eprintln!("  New entries:     {}", new_count);
    eprintln!("  Duplicates:      {} (skipped)", skipped_count);
    eprintln!("  Total in DB:     {}", total_count);
    eprintln!("Saved to: {}", db_path);

    // Output readings as JSON
    let json = serde_json::to_string_pretty(&readings)?;
    println!("{}", json);

    eprintln!("Export complete!");
    Ok(())
}

fn print_help() {
    eprintln!("Accu-Chek USB Data Downloader v{}", env!("CARGO_PKG_VERSION"));
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  accuchek                    Launch GUI application");
    eprintln!("  accuchek sync [device_idx]  Download from device (CLI mode)");
    eprintln!("  accuchek path               Show data file locations");
    eprintln!("  accuchek help               Show this help");
    eprintln!();
    eprintln!("ENVIRONMENT:");
    eprintln!("  ACCUCHEK_DBG=1              Enable debug output");
    eprintln!();
    eprintln!("DATA LOCATIONS:");
    eprintln!("  Database:  {}", default_database_path().display());
    eprintln!("  Config:    {}", config_file_path().display());
}

#[cfg(unix)]
fn check_root_privileges() -> Result<(), AccuChekError> {
    // Use nix crate or raw syscall to check euid
    // For simplicity, we'll skip this check and let USB access fail with appropriate error
    // Users can run with sudo if needed
    Ok(())
}
