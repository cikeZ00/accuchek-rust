//! Configuration file parsing

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use crate::error::AccuChekError;

/// Get the application data directory (OS-specific)
/// - Windows: C:\Users\<user>\AppData\Roaming\accuchek
/// - Linux: ~/.local/share/accuchek
/// - macOS: ~/Library/Application Support/accuchek
pub fn get_data_dir() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."));
    
    base.join("accuchek")
}

/// Get the default database path
pub fn default_database_path() -> PathBuf {
    get_data_dir().join("accuchek.db")
}

/// Get the default export directory (Documents folder)
pub fn default_export_dir() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Ensure the data directory exists
pub fn ensure_data_dir() -> io::Result<PathBuf> {
    let dir = get_data_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Get the config file path (in data directory)
pub fn config_file_path() -> PathBuf {
    get_data_dir().join("config.txt")
}

/// Configuration loaded from config.txt
#[derive(Debug, Default)]
pub struct Config {
    /// Map of "vendor_0xXXXX_device_0xYYYY" -> enabled flag
    pub devices: HashMap<String, bool>,
    /// Path to SQLite database file (default: accuchek.db)
    pub database_path: Option<String>,
}

impl Config {
    /// Load configuration from a file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, AccuChekError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut config = Config::default();

        for line in reader.lines() {
            let line = line?;
            
            // Skip empty lines and comments
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse "key value" or "key value # comment"
            if let Some((key, rest)) = Self::parse_line(line) {
                // Extract value before any comment
                let value = rest.split('#').next().unwrap_or("").trim();
                
                // Handle special config keys
                if key == "database_path" {
                    config.database_path = Some(value.to_string());
                } else {
                    config.devices.insert(key.to_string(), value == "1");
                }
            }
        }

        Ok(config)
    }

    /// Parse a single config line, returning (key, value)
    fn parse_line(line: &str) -> Option<(&str, &str)> {
        // Find first whitespace to separate key from value
        let mut parts = line.splitn(2, |c: char| c.is_whitespace());
        let key = parts.next()?.trim();
        let value = parts.next()?.trim();
        
        if key.is_empty() || value.is_empty() {
            return None;
        }

        Some((key, value))
    }

    /// Check if a specific vendor/device combination is whitelisted
    pub fn is_device_valid(&self, vendor_id: u16, device_id: u16) -> bool {
        let key = format!("vendor_0x{:04x}_device_0x{:04x}", vendor_id, device_id);
        *self.devices.get(&key).unwrap_or(&false)
    }
    
    /// Create a default config file at the given path
    pub fn create_default<P: AsRef<Path>>(path: P) -> io::Result<()> {
        use std::io::Write;
        
        let contents = r#"# Accu-Chek Configuration File
# 
# Device whitelist: vendor_0xXXXX_device_0xYYYY 1
# Use 1 to enable, 0 to disable

# Roche Accu-Chek devices
vendor_0x173a_device_0x21d5 1  # Accu-Chek model 929
vendor_0x173a_device_0x21d7 1  # Accu-Chek model (product id 0x21d7)
vendor_0x173a_device_0x21d8 1  # Relion Platinum model 982

# Optional: Custom database path (uncomment to override default)
# database_path C:\path\to\custom\accuchek.db
"#;
        
        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        
        let mut file = File::create(path)?;
        file.write_all(contents.as_bytes())?;
        Ok(())
    }
}
