//! SQLite storage for glucose readings with notes support

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::device::GlucoseReading;
use crate::units::Thresholds;
use crate::stats::{ReadingData, BasicStats, TimeInRange, DailyStats, HourlyStats, TimeBinStats, HistogramBin, CalendarDay};

/// Extended reading with notes and tags for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredReading {
    pub id: i64,
    pub epoch: i64,
    pub timestamp: String,
    #[serde(rename = "mg/dL")]
    pub mg_dl: u16,
    #[serde(rename = "mmol/L")]
    pub mmol_l: f64,
    pub note: Option<String>,
    pub tags: Option<String>,
    pub imported_at: String,
}

impl ReadingData for StoredReading {
    fn mg_dl(&self) -> u16 {
        self.mg_dl
    }
    
    fn mmol_l(&self) -> f64 {
        self.mmol_l
    }
    
    fn timestamp(&self) -> &str {
        &self.timestamp
    }
}

/// SQLite database for storing readings
pub struct Storage {
    conn: Connection,
}

impl Storage {
    /// Create or open a database at the given path
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS readings (
                id INTEGER PRIMARY KEY,
                epoch INTEGER NOT NULL UNIQUE,
                timestamp TEXT NOT NULL,
                mg_dl INTEGER NOT NULL,
                mmol_l REAL NOT NULL,
                note TEXT,
                tags TEXT,
                imported_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            
            CREATE INDEX IF NOT EXISTS idx_readings_epoch 
                ON readings(epoch);
            
            CREATE INDEX IF NOT EXISTS idx_readings_mg_dl 
                ON readings(mg_dl);
                
            CREATE INDEX IF NOT EXISTS idx_readings_timestamp 
                ON readings(timestamp);"
        )?;
        
        Ok(Self { conn })
    }

    /// Insert a reading, ignoring duplicates based on epoch timestamp
    pub fn insert_reading(&self, reading: &GlucoseReading) -> Result<Option<i64>> {
        let result = self.conn.execute(
            "INSERT OR IGNORE INTO readings (epoch, timestamp, mg_dl, mmol_l) 
             VALUES (?1, ?2, ?3, ?4)",
            params![
                reading.epoch,
                reading.timestamp,
                reading.mg_dl,
                reading.mmol_l,
            ],
        )?;
        
        if result > 0 {
            Ok(Some(self.conn.last_insert_rowid()))
        } else {
            Ok(None) // Duplicate, not inserted
        }
    }

    /// Bulk import readings, returns count of new entries
    pub fn import_readings(&self, readings: &[GlucoseReading]) -> Result<usize> {
        let mut count = 0;
        for reading in readings {
            if self.insert_reading(reading)?.is_some() {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Update note for a reading by database ID
    pub fn update_note(&self, id: i64, note: &str) -> Result<usize> {
        let updated = self.conn.execute(
            "UPDATE readings SET note = ?1 WHERE id = ?2",
            params![note, id],
        )?;
        Ok(updated)
    }

    /// Add tags to a reading
    pub fn update_tags(&self, id: i64, tags: &str) -> Result<usize> {
        let updated = self.conn.execute(
            "UPDATE readings SET tags = ?1 WHERE id = ?2",
            params![tags, id],
        )?;
        Ok(updated)
    }

    /// Get all readings
    pub fn get_all_readings(&self) -> Result<Vec<StoredReading>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, epoch, timestamp, mg_dl, mmol_l, note, tags, imported_at 
             FROM readings ORDER BY epoch"
        )?;

        let readings = stmt.query_map([], Self::row_to_stored_reading)?
            .collect::<Result<Vec<_>>>()?;

        Ok(readings)
    }

    /// Get total reading count
    pub fn count(&self) -> Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM readings", [], |row| row.get(0))
    }

    /// Get all mg/dL values as a vector
    pub fn get_all_values(&self) -> Result<Vec<u16>> {
        let mut stmt = self.conn.prepare("SELECT mg_dl FROM readings ORDER BY epoch")?;
        let values = stmt.query_map([], |row| row.get::<_, u16>(0))?
            .collect::<Result<Vec<_>>>()?;
        Ok(values)
    }

    /// Get all values in both units
    pub fn get_all_values_both(&self) -> Result<(Vec<u16>, Vec<f64>)> {
        let mut stmt = self.conn.prepare("SELECT mg_dl, mmol_l FROM readings ORDER BY epoch")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, u16>(0)?, row.get::<_, f64>(1)?))
        })?;
        
        let mut mgdl: Vec<u16> = Vec::new();
        let mut mmol: Vec<f64> = Vec::new();
        for row in rows {
            let (mg, mm) = row?;
            mgdl.push(mg);
            mmol.push(mm);
        }
        Ok((mgdl, mmol))
    }

    /// Get basic statistics
    pub fn get_basic_stats(&self) -> Result<Option<BasicStats>> {
        let (mgdl, mmol) = self.get_all_values_both()?;
        Ok(BasicStats::from_values(&mgdl, &mmol))
    }

    /// Get time-in-range statistics
    pub fn get_time_in_range(&self, thresholds: Thresholds) -> Result<TimeInRange> {
        let values = self.get_all_values()?;
        Ok(TimeInRange::from_values(&values, thresholds))
    }

    /// Get daily statistics
    pub fn get_daily_stats(&self, thresholds: Thresholds) -> Result<Vec<DailyStats>> {
        let readings = self.get_all_readings()?;
        
        use std::collections::BTreeMap;
        let mut daily_readings: BTreeMap<String, (Vec<u16>, Vec<f64>)> = BTreeMap::new();

        for reading in &readings {
            if let Some(date) = reading.timestamp.get(0..10) {
                let entry = daily_readings.entry(date.to_string()).or_default();
                entry.0.push(reading.mg_dl);
                entry.1.push(reading.mmol_l);
            }
        }

        Ok(daily_readings.into_iter()
            .map(|(date, (mgdl, mmol))| DailyStats::new(date, &mgdl, &mmol, thresholds))
            .collect())
    }

    /// Get hourly statistics
    pub fn get_hourly_stats(&self) -> Result<Vec<HourlyStats>> {
        let readings = self.get_all_readings()?;
        let mut hourly_data: Vec<(Vec<u16>, Vec<f64>)> = vec![(Vec::new(), Vec::new()); 24];

        for reading in &readings {
            if let Some(hour_str) = reading.timestamp.get(11..13) {
                if let Ok(hour) = hour_str.parse::<usize>() {
                    if hour < 24 {
                        hourly_data[hour].0.push(reading.mg_dl);
                        hourly_data[hour].1.push(reading.mmol_l);
                    }
                }
            }
        }

        Ok(hourly_data.into_iter()
            .enumerate()
            .map(|(hour, (mgdl, mmol))| HourlyStats::new(hour as u8, mgdl, mmol))
            .collect())
    }

    /// Get time bin statistics
    pub fn get_time_bin_stats(&self) -> Result<Vec<TimeBinStats>> {
        let readings = self.get_all_readings()?;
        
        let bins = [
            ("Overnight", "12AM-6AM", 0u8, 6u8),
            ("Fasting/Morning", "6AM-9AM", 6, 9),
            ("Mid-Morning", "9AM-12PM", 9, 12),
            ("Afternoon", "12PM-6PM", 12, 18),
            ("Evening", "6PM-9PM", 18, 21),
            ("Night", "9PM-12AM", 21, 24),
        ];

        Ok(bins.iter().map(|(name, desc, start, end)| {
            let filtered: Vec<_> = readings.iter()
                .filter(|r| {
                    if let Some(hour_str) = r.timestamp.get(11..13) {
                        if let Ok(hour) = hour_str.parse::<u8>() {
                            return hour >= *start && hour < *end;
                        }
                    }
                    false
                })
                .collect();
            let mgdl: Vec<u16> = filtered.iter().map(|r| r.mg_dl).collect();
            let mmol: Vec<f64> = filtered.iter().map(|r| r.mmol_l).collect();
            TimeBinStats::new(name, desc, *start, *end, mgdl, mmol)
        }).collect())
    }

    /// Get histogram bins
    pub fn get_histogram(&self, bin_width: u16) -> Result<Vec<HistogramBin>> {
        let readings = self.get_all_readings()?;
        if readings.is_empty() {
            return Ok(Vec::new());
        }

        let mut bins: Vec<HistogramBin> = Vec::new();
        let mut start = 40u16;
        let total = readings.len();

        while start < 400 {
            let end = start + bin_width;
            let count = readings.iter().filter(|r| r.mg_dl >= start && r.mg_dl < end).count();
            bins.push(HistogramBin {
                range_start: start,
                range_end: end,
                count,
                percentage: (count as f64 / total as f64) * 100.0,
            });
            start = end;
        }

        Ok(bins)
    }

    /// Get calendar data
    pub fn get_calendar_data(&self, thresholds: Thresholds) -> Result<Vec<CalendarDay>> {
        let readings = self.get_all_readings()?;
        if readings.is_empty() {
            return Ok(Vec::new());
        }

        use std::collections::BTreeMap;
        let mut daily_readings: BTreeMap<String, Vec<(u8, u16, f64)>> = BTreeMap::new();

        for reading in &readings {
            if let (Some(date), Some(hour_str)) = (reading.timestamp.get(0..10), reading.timestamp.get(11..13)) {
                if let Ok(hour) = hour_str.parse::<u8>() {
                    daily_readings.entry(date.to_string())
                        .or_default()
                        .push((hour, reading.mg_dl, reading.mmol_l));
                }
            }
        }

        let results: Vec<CalendarDay> = daily_readings.into_iter().map(|(date, readings)| {
            let (day_of_week, week_of_year) = parse_date_info(&date);
            CalendarDay::new(date, day_of_week, week_of_year, readings, thresholds)
        }).collect();

        Ok(results)
    }

    fn row_to_stored_reading(row: &rusqlite::Row) -> Result<StoredReading> {
        Ok(StoredReading {
            id: row.get(0)?,
            epoch: row.get(1)?,
            timestamp: row.get(2)?,
            mg_dl: row.get(3)?,
            mmol_l: row.get(4)?,
            note: row.get(5)?,
            tags: row.get(6)?,
            imported_at: row.get(7)?,
        })
    }
}

/// Parse date string to get day of week and week of year
fn parse_date_info(date: &str) -> (u8, u32) {
    // Try YYYY-MM-DD format
    if let Ok(parsed) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        use chrono::Datelike;
        return (parsed.weekday().num_days_from_monday() as u8, parsed.iso_week().week());
    }
    // Try YYYY/MM/DD format
    if let Ok(parsed) = chrono::NaiveDate::parse_from_str(date, "%Y/%m/%d") {
        use chrono::Datelike;
        return (parsed.weekday().num_days_from_monday() as u8, parsed.iso_week().week());
    }
    (0, 0)
}
