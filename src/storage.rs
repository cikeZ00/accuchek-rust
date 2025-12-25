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
