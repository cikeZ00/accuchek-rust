//! SQLite storage for glucose readings with notes support

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::device::GlucoseReading;

/// Extended reading with notes and tags for storage
#[allow(dead_code)]
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
    pub tags: Option<String>,  // e.g., "before_meal,fasting,exercise"
    pub imported_at: String,
}

/// SQLite database for storing readings
pub struct Storage {
    conn: Connection,
}

#[allow(dead_code)]
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

    /// Update note for a reading by epoch timestamp
    pub fn update_note_by_epoch(&self, epoch: i64, note: &str) -> Result<usize> {
        let updated = self.conn.execute(
            "UPDATE readings SET note = ?1 WHERE epoch = ?2",
            params![note, epoch],
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

    /// Get readings in a date range (by epoch) - useful for visualizations
    pub fn get_readings_in_range(
        &self,
        start_epoch: i64,
        end_epoch: i64,
    ) -> Result<Vec<StoredReading>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, epoch, timestamp, mg_dl, mmol_l, note, tags, imported_at 
             FROM readings 
             WHERE epoch BETWEEN ?1 AND ?2 
             ORDER BY epoch"
        )?;

        let readings = stmt.query_map(
            params![start_epoch, end_epoch],
            |row| Self::row_to_stored_reading(row),
        )?.collect::<Result<Vec<_>>>()?;

        Ok(readings)
    }

    /// Get all readings
    pub fn get_all_readings(&self) -> Result<Vec<StoredReading>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, epoch, timestamp, mg_dl, mmol_l, note, tags, imported_at 
             FROM readings ORDER BY epoch"
        )?;

        let readings = stmt.query_map([], |row| Self::row_to_stored_reading(row))?
            .collect::<Result<Vec<_>>>()?;

        Ok(readings)
    }

    /// Get daily averages (for line charts)
    pub fn get_daily_averages(&self) -> Result<Vec<DailyStats>> {
        let mut stmt = self.conn.prepare(
            "SELECT 
                date(timestamp) as day,
                AVG(mg_dl) as avg_mg_dl,
                MIN(mg_dl) as min_mg_dl,
                MAX(mg_dl) as max_mg_dl,
                COUNT(*) as count
             FROM readings 
             GROUP BY day 
             ORDER BY day"
        )?;

        let averages = stmt.query_map([], |row| {
            Ok(DailyStats {
                date: row.get(0)?,
                avg_mg_dl: row.get(1)?,
                min_mg_dl: row.get(2)?,
                max_mg_dl: row.get(3)?,
                count: row.get(4)?,
            })
        })?.collect::<Result<Vec<_>>>()?;

        Ok(averages)
    }

    /// Get time-in-range statistics (standard diabetes metrics)
    /// Low: <70 mg/dL, Normal: 70-180 mg/dL, High: >180 mg/dL
    pub fn get_time_in_range(&self) -> Result<TimeInRange> {
        let mut stmt = self.conn.prepare(
            "SELECT 
                COUNT(*) as total,
                SUM(CASE WHEN mg_dl < 70 THEN 1 ELSE 0 END) as low,
                SUM(CASE WHEN mg_dl >= 70 AND mg_dl <= 180 THEN 1 ELSE 0 END) as normal,
                SUM(CASE WHEN mg_dl > 180 THEN 1 ELSE 0 END) as high
             FROM readings"
        )?;

        let result = stmt.query_row([], |row| {
            let total: i64 = row.get(0)?;
            let low: i64 = row.get(1)?;
            let normal: i64 = row.get(2)?;
            let high: i64 = row.get(3)?;
            
            Ok(TimeInRange {
                total,
                low,
                normal,
                high,
                low_percent: if total > 0 { (low as f64 / total as f64) * 100.0 } else { 0.0 },
                normal_percent: if total > 0 { (normal as f64 / total as f64) * 100.0 } else { 0.0 },
                high_percent: if total > 0 { (high as f64 / total as f64) * 100.0 } else { 0.0 },
            })
        })?;

        Ok(result)
    }

    /// Get readings filtered by tag
    pub fn get_readings_by_tag(&self, tag: &str) -> Result<Vec<StoredReading>> {
        let pattern = format!("%{}%", tag);
        let mut stmt = self.conn.prepare(
            "SELECT id, epoch, timestamp, mg_dl, mmol_l, note, tags, imported_at 
             FROM readings 
             WHERE tags LIKE ?1
             ORDER BY epoch"
        )?;

        let readings = stmt.query_map([pattern], |row| Self::row_to_stored_reading(row))?
            .collect::<Result<Vec<_>>>()?;

        Ok(readings)
    }

    /// Get total reading count
    pub fn count(&self) -> Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM readings", [], |row| row.get(0))
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

// Statistical helper functions
fn calculate_percentile(sorted_values: &[u16], percentile: f64) -> u16 {
    if sorted_values.is_empty() {
        return 0;
    }
    let idx = ((sorted_values.len() as f64 - 1.0) * percentile / 100.0).round() as usize;
    sorted_values[idx.min(sorted_values.len() - 1)]
}

fn calculate_std_dev(values: &[u16], mean: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let variance: f64 = values.iter()
        .map(|&v| (v as f64 - mean).powi(2))
        .sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

/// Analysis functions for visualizations
impl Storage {
    /// Get histogram bins for glucose distribution
    pub fn get_histogram(&self, bin_width: u16, _low_threshold: u16, _high_threshold: u16) -> Result<Vec<HistogramBin>> {
        let readings = self.get_all_readings()?;
        if readings.is_empty() {
            return Ok(Vec::new());
        }

        // Define bins from 40 to 400 mg/dL
        let mut bins: Vec<HistogramBin> = Vec::new();
        let mut start = 40u16;
        while start < 400 {
            let end = start + bin_width;
            let count = readings.iter().filter(|r| r.mg_dl >= start && r.mg_dl < end).count();
            bins.push(HistogramBin {
                range_start: start,
                range_end: end,
                count,
                percentage: (count as f64 / readings.len() as f64) * 100.0,
            });
            start = end;
        }

        Ok(bins)
    }

    /// Get hourly statistics for time-of-day analysis
    pub fn get_hourly_stats(&self) -> Result<Vec<HourlyStats>> {
        let readings = self.get_all_readings()?;
        if readings.is_empty() {
            return Ok(Vec::new());
        }

        let mut hourly_data: Vec<Vec<u16>> = vec![Vec::new(); 24];

        for reading in &readings {
            // Parse hour from timestamp (format: "YYYY-MM-DD HH:MM:SS")
            if let Some(hour_str) = reading.timestamp.get(11..13) {
                if let Ok(hour) = hour_str.parse::<usize>() {
                    if hour < 24 {
                        hourly_data[hour].push(reading.mg_dl);
                    }
                }
            }
        }

        let mut stats: Vec<HourlyStats> = Vec::new();
        for hour in 0..24 {
            let mut values = hourly_data[hour].clone();
            if values.is_empty() {
                stats.push(HourlyStats {
                    hour: hour as u8,
                    readings: Vec::new(),
                    count: 0,
                    mean: 0.0,
                    std_dev: 0.0,
                    min: 0,
                    max: 0,
                    median: 0,
                    q1: 0,
                    q3: 0,
                });
                continue;
            }

            values.sort();
            let count = values.len();
            let mean = values.iter().map(|&v| v as f64).sum::<f64>() / count as f64;
            let std_dev = calculate_std_dev(&values, mean);

            stats.push(HourlyStats {
                hour: hour as u8,
                readings: values.clone(),
                count,
                mean,
                std_dev,
                min: values[0],
                max: values[count - 1],
                median: calculate_percentile(&values, 50.0),
                q1: calculate_percentile(&values, 25.0),
                q3: calculate_percentile(&values, 75.0),
            });
        }

        Ok(stats)
    }

    /// Get clinical time bin statistics for boxplots
    pub fn get_time_bin_stats(&self, _low_threshold: u16, _high_threshold: u16) -> Result<Vec<TimeBinStats>> {
        let readings = self.get_all_readings()?;
        if readings.is_empty() {
            return Ok(Vec::new());
        }

        // Clinical time bins
        let bins = [
            ("Overnight", "12AM-6AM", 0, 6),
            ("Fasting/Morning", "6AM-9AM", 6, 9),
            ("Mid-Morning", "9AM-12PM", 9, 12),
            ("Afternoon", "12PM-6PM", 12, 18),
            ("Evening", "6PM-9PM", 18, 21),
            ("Night", "9PM-12AM", 21, 24),
        ];

        let mut stats: Vec<TimeBinStats> = Vec::new();

        for (name, desc, start, end) in bins {
            let mut values: Vec<u16> = readings.iter()
                .filter(|r| {
                    if let Some(hour_str) = r.timestamp.get(11..13) {
                        if let Ok(hour) = hour_str.parse::<u8>() {
                            return hour >= start && hour < end;
                        }
                    }
                    false
                })
                .map(|r| r.mg_dl)
                .collect();

            if values.is_empty() {
                stats.push(TimeBinStats {
                    name: name.to_string(),
                    description: desc.to_string(),
                    hour_start: start,
                    hour_end: end,
                    count: 0,
                    mean: 0.0,
                    std_dev: 0.0,
                    min: 0,
                    max: 0,
                    median: 0,
                    q1: 0,
                    q3: 0,
                    readings: Vec::new(),
                });
                continue;
            }

            values.sort();
            let count = values.len();
            let mean = values.iter().map(|&v| v as f64).sum::<f64>() / count as f64;
            let std_dev = calculate_std_dev(&values, mean);

            stats.push(TimeBinStats {
                name: name.to_string(),
                description: desc.to_string(),
                hour_start: start,
                hour_end: end,
                count,
                mean,
                std_dev,
                min: values[0],
                max: values[count - 1],
                median: calculate_percentile(&values, 50.0),
                q1: calculate_percentile(&values, 25.0),
                q3: calculate_percentile(&values, 75.0),
                readings: values,
            });
        }

        Ok(stats)
    }

    /// Get daily time-in-range for trend analysis
    pub fn get_daily_tir(&self, low_threshold: u16, high_threshold: u16) -> Result<Vec<DailyTIR>> {
        let readings = self.get_all_readings()?;
        if readings.is_empty() {
            return Ok(Vec::new());
        }

        use std::collections::BTreeMap;
        let mut daily_readings: BTreeMap<String, Vec<u16>> = BTreeMap::new();

        for reading in &readings {
            if let Some(date) = reading.timestamp.get(0..10) {
                daily_readings.entry(date.to_string())
                    .or_insert_with(Vec::new)
                    .push(reading.mg_dl);
            }
        }

        let mut results: Vec<DailyTIR> = Vec::new();
        for (date, values) in daily_readings {
            let total = values.len();
            let low_count = values.iter().filter(|&&v| v < low_threshold).count();
            let in_range_count = values.iter().filter(|&&v| v >= low_threshold && v <= high_threshold).count();
            let high_count = values.iter().filter(|&&v| v > high_threshold).count();

            results.push(DailyTIR {
                date,
                total,
                low_count,
                in_range_count,
                high_count,
                low_pct: (low_count as f64 / total as f64) * 100.0,
                in_range_pct: (in_range_count as f64 / total as f64) * 100.0,
                high_pct: (high_count as f64 / total as f64) * 100.0,
            });
        }

        Ok(results)
    }

    /// Get calendar data for small multiples view
    pub fn get_calendar_data(&self, low_threshold: u16, high_threshold: u16) -> Result<Vec<CalendarDay>> {
        let readings = self.get_all_readings()?;
        if readings.is_empty() {
            return Ok(Vec::new());
        }

        use std::collections::BTreeMap;
        let mut daily_readings: BTreeMap<String, Vec<(u8, u16)>> = BTreeMap::new();

        for reading in &readings {
            if let (Some(date), Some(hour_str)) = (reading.timestamp.get(0..10), reading.timestamp.get(11..13)) {
                if let Ok(hour) = hour_str.parse::<u8>() {
                    daily_readings.entry(date.to_string())
                        .or_insert_with(Vec::new)
                        .push((hour, reading.mg_dl));
                }
            }
        }

        let mut results: Vec<CalendarDay> = Vec::new();
        for (date, readings) in daily_readings {
            let count = readings.len();
            let values: Vec<u16> = readings.iter().map(|&(_, v)| v).collect();
            let mean = values.iter().map(|&v| v as f64).sum::<f64>() / count as f64;
            let min = values.iter().min().copied().unwrap_or(0);
            let max = values.iter().max().copied().unwrap_or(0);
            let in_range = values.iter().filter(|&&v| v >= low_threshold && v <= high_threshold).count();
            let in_range_pct = (in_range as f64 / count as f64) * 100.0;

            // Parse date to get day of week and week of year
            let (day_of_week, week_of_year) = if let Ok(parsed) = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d") {
                use chrono::Datelike;
                (parsed.weekday().num_days_from_monday() as u8, parsed.iso_week().week())
            } else {
                (0, 0)
            };

            results.push(CalendarDay {
                date,
                day_of_week,
                week_of_year,
                readings,
                count,
                mean,
                min,
                max,
                in_range_pct,
            });
        }

        Ok(results)
    }
}

/// Daily statistics for visualization
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
    pub date: String,
    pub avg_mg_dl: f64,
    pub min_mg_dl: u16,
    pub max_mg_dl: u16,
    pub count: i64,
}

/// Time-in-range statistics (standard diabetes management metric)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeInRange {
    pub total: i64,
    pub low: i64,        // <70 mg/dL
    pub normal: i64,     // 70-180 mg/dL
    pub high: i64,       // >180 mg/dL
    pub low_percent: f64,
    pub normal_percent: f64,
    pub high_percent: f64,
}

/// Histogram bin for glucose distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramBin {
    pub range_start: u16,
    pub range_end: u16,
    pub count: usize,
    pub percentage: f64,
}

/// Hour-of-day statistics for scatter/hexbin visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyStats {
    pub hour: u8,
    pub readings: Vec<u16>,  // All glucose values for this hour
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: u16,
    pub max: u16,
    pub median: u16,
    pub q1: u16,  // 25th percentile
    pub q3: u16,  // 75th percentile
}

/// Clinical time bin statistics (for boxplots)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBinStats {
    pub name: String,
    pub description: String,
    pub hour_start: u8,
    pub hour_end: u8,
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: u16,
    pub max: u16,
    pub median: u16,
    pub q1: u16,
    pub q3: u16,
    pub readings: Vec<u16>,
}

/// Daily Time-in-Range for trend analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyTIR {
    pub date: String,
    pub total: usize,
    pub low_count: usize,
    pub in_range_count: usize,
    pub high_count: usize,
    pub low_pct: f64,
    pub in_range_pct: f64,
    pub high_pct: f64,
}

/// Calendar day data for small multiples view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarDay {
    pub date: String,
    pub day_of_week: u8,  // 0=Monday, 6=Sunday
    pub week_of_year: u32,
    pub readings: Vec<(u8, u16)>,  // (hour, mg_dl)
    pub count: usize,
    pub mean: f64,
    pub min: u16,
    pub max: u16,
    pub in_range_pct: f64,
}
