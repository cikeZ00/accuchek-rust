//! Statistics calculations for glucose readings
//!
//! This module computes statistics in BOTH mg/dL and mmol/L independently,
//! using the direct device values without any conversion.

use serde::{Deserialize, Serialize};
use crate::units::{Thresholds, GlucoseRange, GlucoseUnit};

/// Statistical measures for mg/dL values (integer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MgDlStats {
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: u16,
    pub max: u16,
    pub median: u16,
    pub q1: u16,  // 25th percentile
    pub q3: u16,  // 75th percentile
}

/// Statistical measures for mmol/L values (float)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmolLStats {
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub median: f64,
    pub q1: f64,  // 25th percentile
    pub q3: f64,  // 75th percentile
}

/// Basic statistical measures in both units
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicStats {
    pub mgdl: MgDlStats,
    pub mmol: MmolLStats,
}

impl MgDlStats {
    /// Calculate statistics from mg/dL values
    pub fn from_values(values: &[u16]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }

        let count = values.len();
        let mean = values.iter().map(|&v| v as f64).sum::<f64>() / count as f64;
        let std_dev = calculate_std_dev_u16(values, mean);
        
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        
        Some(Self {
            count,
            mean,
            std_dev,
            min: sorted[0],
            max: sorted[count - 1],
            median: percentile_u16(&sorted, 50.0),
            q1: percentile_u16(&sorted, 25.0),
            q3: percentile_u16(&sorted, 75.0),
        })
    }

    /// Calculate 95% confidence interval for the mean
    pub fn confidence_interval_95(&self) -> (f64, f64) {
        if self.count < 2 {
            return (self.mean, self.mean);
        }
        let se = self.std_dev / (self.count as f64).sqrt();
        (self.mean - 1.96 * se, self.mean + 1.96 * se)
    }
}

impl MmolLStats {
    /// Calculate statistics from mmol/L values
    pub fn from_values(values: &[f64]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }

        let count = values.len();
        let mean = values.iter().sum::<f64>() / count as f64;
        let std_dev = calculate_std_dev_f64(values, mean);
        
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        Some(Self {
            count,
            mean,
            std_dev,
            min: sorted[0],
            max: sorted[count - 1],
            median: percentile_f64(&sorted, 50.0),
            q1: percentile_f64(&sorted, 25.0),
            q3: percentile_f64(&sorted, 75.0),
        })
    }

    /// Calculate 95% confidence interval for the mean
    pub fn confidence_interval_95(&self) -> (f64, f64) {
        if self.count < 2 {
            return (self.mean, self.mean);
        }
        let se = self.std_dev / (self.count as f64).sqrt();
        (self.mean - 1.96 * se, self.mean + 1.96 * se)
    }
}

#[allow(dead_code)]
impl BasicStats {
    /// Calculate basic statistics from parallel mg/dL and mmol/L values
    pub fn from_values(mgdl_values: &[u16], mmol_values: &[f64]) -> Option<Self> {
        let mgdl = MgDlStats::from_values(mgdl_values)?;
        let mmol = MmolLStats::from_values(mmol_values)?;
        Some(Self { mgdl, mmol })
    }

    /// Get count (same for both units)
    pub fn count(&self) -> usize {
        self.mgdl.count
    }

    /// Get mean in user's preferred unit
    pub fn mean(&self, unit: GlucoseUnit) -> f64 {
        match unit {
            GlucoseUnit::MgDl => self.mgdl.mean,
            GlucoseUnit::MmolL => self.mmol.mean,
        }
    }

    /// Get min in user's preferred unit
    pub fn min(&self, unit: GlucoseUnit) -> f64 {
        match unit {
            GlucoseUnit::MgDl => self.mgdl.min as f64,
            GlucoseUnit::MmolL => self.mmol.min,
        }
    }

    /// Get max in user's preferred unit
    pub fn max(&self, unit: GlucoseUnit) -> f64 {
        match unit {
            GlucoseUnit::MgDl => self.mgdl.max as f64,
            GlucoseUnit::MmolL => self.mmol.max,
        }
    }

    /// Get median in user's preferred unit
    pub fn median(&self, unit: GlucoseUnit) -> f64 {
        match unit {
            GlucoseUnit::MgDl => self.mgdl.median as f64,
            GlucoseUnit::MmolL => self.mmol.median,
        }
    }

    /// Get std_dev in user's preferred unit
    pub fn std_dev(&self, unit: GlucoseUnit) -> f64 {
        match unit {
            GlucoseUnit::MgDl => self.mgdl.std_dev,
            GlucoseUnit::MmolL => self.mmol.std_dev,
        }
    }

    /// Get 95% confidence interval in user's preferred unit
    pub fn confidence_interval_95(&self, unit: GlucoseUnit) -> (f64, f64) {
        match unit {
            GlucoseUnit::MgDl => self.mgdl.confidence_interval_95(),
            GlucoseUnit::MmolL => self.mmol.confidence_interval_95(),
        }
    }

    /// Format mean with unit
    pub fn format_mean(&self, unit: GlucoseUnit) -> String {
        match unit {
            GlucoseUnit::MgDl => format!("{:.0} mg/dL", self.mgdl.mean),
            GlucoseUnit::MmolL => format!("{:.1} mmol/L", self.mmol.mean),
        }
    }

    /// Format min with unit
    pub fn format_min(&self, unit: GlucoseUnit) -> String {
        match unit {
            GlucoseUnit::MgDl => format!("{} mg/dL", self.mgdl.min),
            GlucoseUnit::MmolL => format!("{:.1} mmol/L", self.mmol.min),
        }
    }

    /// Format max with unit
    pub fn format_max(&self, unit: GlucoseUnit) -> String {
        match unit {
            GlucoseUnit::MgDl => format!("{} mg/dL", self.mgdl.max),
            GlucoseUnit::MmolL => format!("{:.1} mmol/L", self.mmol.max),
        }
    }

    /// Format median with unit
    pub fn format_median(&self, unit: GlucoseUnit) -> String {
        match unit {
            GlucoseUnit::MgDl => format!("{} mg/dL", self.mgdl.median),
            GlucoseUnit::MmolL => format!("{:.1} mmol/L", self.mmol.median),
        }
    }

    /// Format value (generic) in user's preferred unit
    pub fn format_value(&self, unit: GlucoseUnit, value: f64) -> String {
        match unit {
            GlucoseUnit::MgDl => format!("{:.0}", value),
            GlucoseUnit::MmolL => format!("{:.1}", value),
        }
    }
}

/// Time-in-range statistics (standard diabetes metric)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeInRange {
    pub total: usize,
    pub very_low: usize,
    pub low: usize,
    pub in_range: usize,
    pub high: usize,
    pub very_high: usize,
}

impl TimeInRange {
    /// Calculate TIR from mg/dL values using given thresholds
    pub fn from_values(values: &[u16], thresholds: Thresholds) -> Self {
        let mut tir = Self {
            total: values.len(),
            very_low: 0,
            low: 0,
            in_range: 0,
            high: 0,
            very_high: 0,
        };

        for &v in values {
            match thresholds.classify(v) {
                GlucoseRange::VeryLow => tir.very_low += 1,
                GlucoseRange::Low => tir.low += 1,
                GlucoseRange::InRange => tir.in_range += 1,
                GlucoseRange::High => tir.high += 1,
                GlucoseRange::VeryHigh => tir.very_high += 1,
            }
        }

        tir
    }

    /// Get percentage for a given range
    pub fn percentage(&self, range: GlucoseRange) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        let count = match range {
            GlucoseRange::VeryLow => self.very_low,
            GlucoseRange::Low => self.low,
            GlucoseRange::InRange => self.in_range,
            GlucoseRange::High => self.high,
            GlucoseRange::VeryHigh => self.very_high,
        };
        (count as f64 / self.total as f64) * 100.0
    }

    /// Get total low (very low + low)
    pub fn total_low(&self) -> usize {
        self.very_low + self.low
    }

    /// Get total high (high + very high)
    pub fn total_high(&self) -> usize {
        self.high + self.very_high
    }

    /// Get low percentage (combined very low + low)
    pub fn low_percent(&self) -> f64 {
        if self.total == 0 { 0.0 } else {
            (self.total_low() as f64 / self.total as f64) * 100.0
        }
    }

    /// Get in-range percentage
    pub fn in_range_percent(&self) -> f64 {
        self.percentage(GlucoseRange::InRange)
    }

    /// Get high percentage (combined high + very high)
    pub fn high_percent(&self) -> f64 {
        if self.total == 0 { 0.0 } else {
            (self.total_high() as f64 / self.total as f64) * 100.0
        }
    }
}

/// Daily statistics for trend analysis (both units)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
    pub date: String,
    pub count: usize,
    pub avg_mgdl: f64,
    pub avg_mmol: f64,
    pub min_mgdl: u16,
    pub min_mmol: f64,
    pub max_mgdl: u16,
    pub max_mmol: f64,
    pub tir: TimeInRange,
}

#[allow(dead_code)]
impl DailyStats {
    /// Create daily stats from parallel values
    pub fn new(date: String, mgdl_values: &[u16], mmol_values: &[f64], thresholds: Thresholds) -> Self {
        let count = mgdl_values.len();
        
        let avg_mgdl = if count > 0 {
            mgdl_values.iter().map(|&v| v as f64).sum::<f64>() / count as f64
        } else {
            0.0
        };
        
        let avg_mmol = if count > 0 {
            mmol_values.iter().sum::<f64>() / count as f64
        } else {
            0.0
        };
        
        Self {
            date,
            count,
            avg_mgdl,
            avg_mmol,
            min_mgdl: mgdl_values.iter().copied().min().unwrap_or(0),
            min_mmol: mmol_values.iter().copied().fold(f64::INFINITY, f64::min),
            max_mgdl: mgdl_values.iter().copied().max().unwrap_or(0),
            max_mmol: mmol_values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            tir: TimeInRange::from_values(mgdl_values, thresholds),
        }
    }

    /// Get average in user's preferred unit
    pub fn avg(&self, unit: GlucoseUnit) -> f64 {
        match unit {
            GlucoseUnit::MgDl => self.avg_mgdl,
            GlucoseUnit::MmolL => self.avg_mmol,
        }
    }
}

/// Hourly statistics for time-of-day analysis (both units)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyStats {
    pub hour: u8,
    pub mgdl_readings: Vec<u16>,
    pub mmol_readings: Vec<f64>,
    pub stats: Option<BasicStats>,
}

impl HourlyStats {
    /// Create hourly stats for a given hour
    pub fn new(hour: u8, mgdl_readings: Vec<u16>, mmol_readings: Vec<f64>) -> Self {
        let stats = BasicStats::from_values(&mgdl_readings, &mmol_readings);
        Self { hour, mgdl_readings, mmol_readings, stats }
    }

    pub fn count(&self) -> usize {
        self.mgdl_readings.len()
    }
}

/// Clinical time bin statistics for boxplots (both units)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBinStats {
    pub name: String,
    pub description: String,
    pub hour_start: u8,
    pub hour_end: u8,
    pub mgdl_readings: Vec<u16>,
    pub mmol_readings: Vec<f64>,
    pub stats: Option<BasicStats>,
}

impl TimeBinStats {
    /// Create time bin stats
    pub fn new(name: &str, description: &str, hour_start: u8, hour_end: u8, mgdl_readings: Vec<u16>, mmol_readings: Vec<f64>) -> Self {
        let stats = BasicStats::from_values(&mgdl_readings, &mmol_readings);
        Self {
            name: name.to_string(),
            description: description.to_string(),
            hour_start,
            hour_end,
            mgdl_readings,
            mmol_readings,
            stats,
        }
    }
}

/// Histogram bin for distribution analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramBin {
    pub range_start: u16,
    pub range_end: u16,
    pub count: usize,
    pub percentage: f64,
}

/// Calendar day data for small multiples view (both units)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarDay {
    pub date: String,
    pub day_of_week: u8,  // 0=Monday, 6=Sunday
    pub week_of_year: u32,
    pub readings: Vec<(u8, u16, f64)>,  // (hour, mg_dl, mmol_l)
    pub stats: Option<BasicStats>,
    pub tir: TimeInRange,
}

impl CalendarDay {
    /// Create calendar day data
    pub fn new(date: String, day_of_week: u8, week_of_year: u32, readings: Vec<(u8, u16, f64)>, thresholds: Thresholds) -> Self {
        let mgdl_values: Vec<u16> = readings.iter().map(|(_, v, _)| *v).collect();
        let mmol_values: Vec<f64> = readings.iter().map(|(_, _, v)| *v).collect();
        let stats = BasicStats::from_values(&mgdl_values, &mmol_values);
        let tir = TimeInRange::from_values(&mgdl_values, thresholds);
        
        Self {
            date,
            day_of_week,
            week_of_year,
            readings,
            stats,
            tir,
        }
    }

    pub fn count(&self) -> usize {
        self.readings.len()
    }

    pub fn mean(&self, unit: GlucoseUnit) -> f64 {
        self.stats.as_ref().map(|s| s.mean(unit)).unwrap_or(0.0)
    }

    pub fn in_range_percent(&self) -> f64 {
        self.tir.in_range_percent()
    }
}

// ============= Helper Functions =============

/// Calculate percentile from sorted u16 values
fn percentile_u16(sorted_values: &[u16], pct: f64) -> u16 {
    if sorted_values.is_empty() {
        return 0;
    }
    let idx = ((sorted_values.len() as f64 - 1.0) * pct / 100.0).round() as usize;
    sorted_values[idx.min(sorted_values.len() - 1)]
}

/// Calculate percentile from sorted f64 values
fn percentile_f64(sorted_values: &[f64], pct: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let idx = ((sorted_values.len() as f64 - 1.0) * pct / 100.0).round() as usize;
    sorted_values[idx.min(sorted_values.len() - 1)]
}

/// Calculate standard deviation for u16 values
fn calculate_std_dev_u16(values: &[u16], mean: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let variance: f64 = values.iter()
        .map(|&v| (v as f64 - mean).powi(2))
        .sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

/// Calculate standard deviation for f64 values
fn calculate_std_dev_f64(values: &[f64], mean: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let variance: f64 = values.iter()
        .map(|&v| (v - mean).powi(2))
        .sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

// ============= Export Statistics (separate from in-app) =============

/// Statistics specifically formatted for PDF export
#[derive(Debug, Clone)]
pub struct ExportStatistics {
    pub basic: BasicStats,
    pub tir: TimeInRange,
    pub daily: Vec<DailyStats>,
    pub hourly: Vec<HourlyStats>,
    pub time_bins: Vec<TimeBinStats>,
    pub histogram: Vec<HistogramBin>,
}

impl ExportStatistics {
    /// Generate all export statistics from stored readings
    pub fn generate<R>(readings: &[R], thresholds: Thresholds) -> Self 
    where
        R: ReadingData,
    {
        let mgdl_values: Vec<u16> = readings.iter().map(|r| r.mg_dl()).collect();
        let mmol_values: Vec<f64> = readings.iter().map(|r| r.mmol_l()).collect();
        
        let basic = BasicStats::from_values(&mgdl_values, &mmol_values).unwrap_or_else(|| {
            BasicStats {
                mgdl: MgDlStats { count: 0, mean: 0.0, std_dev: 0.0, min: 0, max: 0, median: 0, q1: 0, q3: 0 },
                mmol: MmolLStats { count: 0, mean: 0.0, std_dev: 0.0, min: 0.0, max: 0.0, median: 0.0, q1: 0.0, q3: 0.0 },
            }
        });
        let tir = TimeInRange::from_values(&mgdl_values, thresholds);
        
        let daily = Self::calculate_daily(readings, thresholds);
        let hourly = Self::calculate_hourly(readings);
        let time_bins = Self::calculate_time_bins(readings);
        let histogram = Self::calculate_histogram(&mgdl_values);
        
        Self { basic, tir, daily, hourly, time_bins, histogram }
    }

    fn calculate_daily<R: ReadingData>(readings: &[R], thresholds: Thresholds) -> Vec<DailyStats> {
        use std::collections::BTreeMap;
        let mut daily_readings: BTreeMap<String, (Vec<u16>, Vec<f64>)> = BTreeMap::new();

        for reading in readings {
            if let Some(date) = reading.timestamp().get(0..10) {
                let entry = daily_readings.entry(date.to_string()).or_default();
                entry.0.push(reading.mg_dl());
                entry.1.push(reading.mmol_l());
            }
        }

        daily_readings.into_iter()
            .map(|(date, (mgdl, mmol))| DailyStats::new(date, &mgdl, &mmol, thresholds))
            .collect()
    }

    fn calculate_hourly<R: ReadingData>(readings: &[R]) -> Vec<HourlyStats> {
        let mut hourly_data: Vec<(Vec<u16>, Vec<f64>)> = vec![(Vec::new(), Vec::new()); 24];

        for reading in readings {
            if let Some(hour_str) = reading.timestamp().get(11..13) {
                if let Ok(hour) = hour_str.parse::<usize>() {
                    if hour < 24 {
                        hourly_data[hour].0.push(reading.mg_dl());
                        hourly_data[hour].1.push(reading.mmol_l());
                    }
                }
            }
        }

        hourly_data.into_iter()
            .enumerate()
            .map(|(hour, (mgdl, mmol))| HourlyStats::new(hour as u8, mgdl, mmol))
            .collect()
    }

    fn calculate_time_bins<R: ReadingData>(readings: &[R]) -> Vec<TimeBinStats> {
        let bins = [
            ("Overnight", "12AM-6AM", 0u8, 6u8),
            ("Fasting/Morning", "6AM-9AM", 6, 9),
            ("Mid-Morning", "9AM-12PM", 9, 12),
            ("Afternoon", "12PM-6PM", 12, 18),
            ("Evening", "6PM-9PM", 18, 21),
            ("Night", "9PM-12AM", 21, 24),
        ];

        bins.iter().map(|(name, desc, start, end)| {
            let filtered: Vec<_> = readings.iter()
                .filter(|r| {
                    if let Some(hour_str) = r.timestamp().get(11..13) {
                        if let Ok(hour) = hour_str.parse::<u8>() {
                            return hour >= *start && hour < *end;
                        }
                    }
                    false
                })
                .collect();
            let mgdl: Vec<u16> = filtered.iter().map(|r| r.mg_dl()).collect();
            let mmol: Vec<f64> = filtered.iter().map(|r| r.mmol_l()).collect();
            TimeBinStats::new(name, desc, *start, *end, mgdl, mmol)
        }).collect()
    }

    fn calculate_histogram(values: &[u16]) -> Vec<HistogramBin> {
        let bin_width = 20u16;
        let mut bins: Vec<HistogramBin> = Vec::new();
        let mut start = 40u16;
        let total = values.len();

        while start < 400 {
            let end = start + bin_width;
            let count = values.iter().filter(|&&v| v >= start && v < end).count();
            bins.push(HistogramBin {
                range_start: start,
                range_end: end,
                count,
                percentage: if total > 0 { (count as f64 / total as f64) * 100.0 } else { 0.0 },
            });
            start = end;
        }

        bins
    }
}

/// Trait for reading data access (both units)
pub trait ReadingData {
    fn mg_dl(&self) -> u16;
    fn mmol_l(&self) -> f64;
    fn timestamp(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_stats() {
        let mgdl_values = vec![100, 120, 140, 160, 180];
        let mmol_values = vec![5.6, 6.7, 7.8, 8.9, 10.0];
        let stats = BasicStats::from_values(&mgdl_values, &mmol_values).unwrap();
        
        // mg/dL stats
        assert_eq!(stats.mgdl.count, 5);
        assert!((stats.mgdl.mean - 140.0).abs() < 0.01);
        assert_eq!(stats.mgdl.min, 100);
        assert_eq!(stats.mgdl.max, 180);
        
        // mmol/L stats
        assert_eq!(stats.mmol.count, 5);
        assert!((stats.mmol.mean - 7.8).abs() < 0.01);
        assert!((stats.mmol.min - 5.6).abs() < 0.01);
        assert!((stats.mmol.max - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_time_in_range() {
        let values = vec![50, 65, 100, 150, 200, 300];
        let tir = TimeInRange::from_values(&values, Thresholds::default());
        assert_eq!(tir.very_low, 1);
        assert_eq!(tir.low, 1);
        assert_eq!(tir.in_range, 2);
        assert_eq!(tir.high, 1);
        assert_eq!(tir.very_high, 1);
    }
}
