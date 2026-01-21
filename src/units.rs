//! Glucose unit types and formatting
//!
//! This module provides separate types for mg/dL and mmol/L glucose values.
//! Since the device provides both values directly, no conversion is needed -
//! we simply reference the appropriate value based on the user's display preference.
//!
//! Thresholds are also stored in both units independently.

use serde::{Deserialize, Serialize};

/// Glucose value in mg/dL (milligrams per deciliter)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MgDl(pub u16);

/// Glucose value in mmol/L (millimoles per liter)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct MmolL(pub f64);

impl MgDl {
    /// Format the value with unit suffix
    pub fn format(self) -> String {
        format!("{} mg/dL", self.0)
    }

    /// Format just the value without unit suffix
    pub fn format_value(self) -> String {
        format!("{}", self.0)
    }

    /// Get the unit label
    pub fn unit_label() -> &'static str {
        "mg/dL"
    }
}

impl MmolL {
    /// Format the value with unit suffix
    pub fn format(self) -> String {
        format!("{:.1} mmol/L", self.0)
    }

    /// Format just the value without unit suffix
    pub fn format_value(self) -> String {
        format!("{:.1}", self.0)
    }

    /// Get the unit label
    pub fn unit_label() -> &'static str {
        "mmol/L"
    }
}

impl From<u16> for MgDl {
    fn from(value: u16) -> Self {
        MgDl(value)
    }
}

impl From<f64> for MmolL {
    fn from(value: f64) -> Self {
        MmolL(value)
    }
}

/// User's preferred display unit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GlucoseUnit {
    #[serde(rename = "mg/dL")]
    #[default]
    MgDl,
    #[serde(rename = "mmol/L")]
    MmolL,
}

impl GlucoseUnit {
    /// Format glucose values using the user's preferred unit
    /// Takes both values directly from the device (no conversion needed)
    pub fn format(self, mg_dl: u16, mmol_l: f64) -> String {
        match self {
            GlucoseUnit::MgDl => MgDl(mg_dl).format(),
            GlucoseUnit::MmolL => MmolL(mmol_l).format(),
        }
    }

    /// Format glucose value without unit suffix
    /// Takes both values directly from the device (no conversion needed)
    pub fn format_value(self, mg_dl: u16, mmol_l: f64) -> String {
        match self {
            GlucoseUnit::MgDl => MgDl(mg_dl).format_value(),
            GlucoseUnit::MmolL => MmolL(mmol_l).format_value(),
        }
    }

    /// Get the display value in the user's preferred unit
    /// Takes both values directly from the device (no conversion needed)
    #[allow(dead_code)]
    pub fn display_value(self, mg_dl: u16, mmol_l: f64) -> f64 {
        match self {
            GlucoseUnit::MgDl => mg_dl as f64,
            GlucoseUnit::MmolL => mmol_l,
        }
    }

    /// Get the unit label
    pub fn label(self) -> &'static str {
        match self {
            GlucoseUnit::MgDl => MgDl::unit_label(),
            GlucoseUnit::MmolL => MmolL::unit_label(),
        }
    }
}

// Conversion functions are no longer needed since we get both units from the device
// and compute stats in both units independently.

/// Clinical threshold ranges for glucose levels
/// Stores both mg/dL and mmol/L values independently (no conversion)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// Low threshold in mg/dL - default 70
    pub low_mgdl: u16,
    /// High threshold in mg/dL - default 180
    pub high_mgdl: u16,
    /// Low threshold in mmol/L - default 3.9
    pub low_mmol: f64,
    /// High threshold in mmol/L - default 10.0
    pub high_mmol: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            low_mgdl: 70,
            high_mgdl: 180,
            low_mmol: 3.9,
            high_mmol: 10.0,
        }
    }
}

impl Thresholds {
    /// Clinical constant: severe hypoglycemia threshold
    pub const VERY_LOW_MGDL: u16 = 54;
    pub const VERY_LOW_MMOL: f64 = 3.0;
    
    /// Clinical constant: severe hyperglycemia threshold
    pub const VERY_HIGH_MGDL: u16 = 250;
    pub const VERY_HIGH_MMOL: f64 = 13.9;

    /// Classify a reading using mg/dL value
    pub fn classify(&self, mg_dl: u16) -> GlucoseRange {
        if mg_dl < Self::VERY_LOW_MGDL {
            GlucoseRange::VeryLow
        } else if mg_dl < self.low_mgdl {
            GlucoseRange::Low
        } else if mg_dl <= self.high_mgdl {
            GlucoseRange::InRange
        } else if mg_dl <= Self::VERY_HIGH_MGDL {
            GlucoseRange::High
        } else {
            GlucoseRange::VeryHigh
        }
    }

    /// Get threshold display string for the user's unit
    pub fn format_range(&self, unit: GlucoseUnit) -> String {
        match unit {
            GlucoseUnit::MgDl => format!("{}-{} mg/dL", self.low_mgdl, self.high_mgdl),
            GlucoseUnit::MmolL => format!("{:.1}-{:.1} mmol/L", self.low_mmol, self.high_mmol),
        }
    }

    /// Get low threshold display value for the user's unit
    pub fn low_display(&self, unit: GlucoseUnit) -> String {
        match unit {
            GlucoseUnit::MgDl => format!("{}", self.low_mgdl),
            GlucoseUnit::MmolL => format!("{:.1}", self.low_mmol),
        }
    }

    /// Get high threshold display value for the user's unit
    pub fn high_display(&self, unit: GlucoseUnit) -> String {
        match unit {
            GlucoseUnit::MgDl => format!("{}", self.high_mgdl),
            GlucoseUnit::MmolL => format!("{:.1}", self.high_mmol),
        }
    }
}

/// Classification of glucose value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlucoseRange {
    VeryLow,  // < 54 mg/dL (3.0 mmol/L) - severe hypoglycemia
    Low,      // 54 to low_threshold
    InRange,  // low_threshold to high_threshold
    High,     // high_threshold to 250 (13.9 mmol/L)
    VeryHigh, // > 250 mg/dL - risk of ketoacidosis
}

impl GlucoseRange {
    /// Get a display label for the range
    pub fn label(self) -> &'static str {
        match self {
            GlucoseRange::VeryLow => "Very Low",
            GlucoseRange::Low => "Low",
            GlucoseRange::InRange => "In Range",
            GlucoseRange::High => "High",
            GlucoseRange::VeryHigh => "Very High",
        }
    }

    /// Get a short status text
    pub fn status(self) -> &'static str {
        match self {
            GlucoseRange::VeryLow => "VERY LOW",
            GlucoseRange::Low => "LOW",
            GlucoseRange::InRange => "OK",
            GlucoseRange::High => "HIGH",
            GlucoseRange::VeryHigh => "VERY HIGH",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mgdl_formatting() {
        let mgdl = MgDl(180);
        assert_eq!(mgdl.format(), "180 mg/dL");
        assert_eq!(mgdl.format_value(), "180");
    }

    #[test]
    fn test_mmol_formatting() {
        let mmol = MmolL(10.0);
        assert_eq!(mmol.format(), "10.0 mmol/L");
        assert_eq!(mmol.format_value(), "10.0");
    }

    #[test]
    fn test_glucose_unit_format() {
        // Using device-provided values directly
        let mg_dl = 180;
        let mmol_l = 10.0;
        
        assert_eq!(GlucoseUnit::MgDl.format(mg_dl, mmol_l), "180 mg/dL");
        assert_eq!(GlucoseUnit::MmolL.format(mg_dl, mmol_l), "10.0 mmol/L");
    }

    #[test]
    fn test_thresholds_classification() {
        let thresholds = Thresholds::default();
        
        assert_eq!(thresholds.classify(50), GlucoseRange::VeryLow);
        assert_eq!(thresholds.classify(60), GlucoseRange::Low);
        assert_eq!(thresholds.classify(100), GlucoseRange::InRange);
        assert_eq!(thresholds.classify(200), GlucoseRange::High);
        assert_eq!(thresholds.classify(300), GlucoseRange::VeryHigh);
    }

    #[test]
    fn test_thresholds_display() {
        let thresholds = Thresholds::default();
        
        assert_eq!(thresholds.format_range(GlucoseUnit::MgDl), "70-180 mg/dL");
        assert_eq!(thresholds.format_range(GlucoseUnit::MmolL), "3.9-10.0 mmol/L");
    }
}
