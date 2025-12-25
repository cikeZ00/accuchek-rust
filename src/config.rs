//! Configuration file parsing

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use crate::error::AccuChekError;

/// Configuration loaded from config.txt
#[derive(Debug, Default)]
pub struct Config {
    /// Map of "vendor_0xXXXX_device_0xYYYY" -> enabled flag
    pub devices: HashMap<String, bool>,
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
                config.devices.insert(key.to_string(), value == "1");
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
}
