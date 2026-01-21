//! PDF Export functionality for glucose readings
//!
//! This module provides a clean, modular PDF exporter that generates
//! comprehensive glucose reports with multiple visualization pages.

use printpdf::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::storage::StoredReading;
use crate::units::{GlucoseUnit, Thresholds, GlucoseRange};
use crate::stats::ExportStatistics;

// ============= Chart Axis Ranges =============

/// Axis ranges for mg/dL charts
const MGDL_Y_MIN: f32 = 40.0;
const MGDL_Y_MAX: f32 = 300.0;
const MGDL_AXIS_LABELS: [u16; 5] = [50, 100, 150, 200, 250];

/// Axis ranges for mmol/L charts
const MMOL_Y_MIN: f32 = 2.0;
const MMOL_Y_MAX: f32 = 17.0;
const MMOL_AXIS_LABELS: [f32; 6] = [3.0, 5.0, 7.0, 10.0, 13.0, 16.0];

// ============= Constants =============

const PAGE_WIDTH_MM: f32 = 210.0;
const PAGE_HEIGHT_MM: f32 = 297.0;
const MARGIN_MM: f32 = 20.0;

// ============= Colors =============

struct PdfColors;

impl PdfColors {
    fn red() -> Color {
        Color::Rgb(Rgb { r: 0.9, g: 0.3, b: 0.3, icc_profile: None })
    }
    
    fn green() -> Color {
        Color::Rgb(Rgb { r: 0.3, g: 0.7, b: 0.3, icc_profile: None })
    }
    
    fn orange() -> Color {
        Color::Rgb(Rgb { r: 0.9, g: 0.6, b: 0.3, icc_profile: None })
    }
    
    fn blue() -> Color {
        Color::Rgb(Rgb { r: 0.3, g: 0.5, b: 0.8, icc_profile: None })
    }
    
    fn black() -> Color {
        Color::Rgb(Rgb { r: 0.0, g: 0.0, b: 0.0, icc_profile: None })
    }
    
    fn gray() -> Color {
        Color::Rgb(Rgb { r: 0.5, g: 0.5, b: 0.5, icc_profile: None })
    }
    
    fn light_gray() -> Color {
        Color::Rgb(Rgb { r: 0.9, g: 0.9, b: 0.9, icc_profile: None })
    }
    
    fn for_range(range: GlucoseRange) -> Color {
        match range {
            GlucoseRange::VeryLow => Color::Rgb(Rgb { r: 0.8, g: 0.2, b: 0.2, icc_profile: None }),
            GlucoseRange::Low => Self::red(),
            GlucoseRange::InRange => Self::green(),
            GlucoseRange::High => Self::orange(),
            GlucoseRange::VeryHigh => Color::Rgb(Rgb { r: 0.9, g: 0.3, b: 0.2, icc_profile: None }),
        }
    }
    
    fn for_value(mg_dl: u16, thresholds: &Thresholds) -> Color {
        Self::for_range(thresholds.classify(mg_dl))
    }

    fn for_mmol_value(mmol: f64, thresholds: &Thresholds) -> Color {
        // Convert mmol thresholds comparison
        if mmol < Thresholds::VERY_LOW_MMOL {
            Self::for_range(GlucoseRange::VeryLow)
        } else if mmol < thresholds.low_mmol {
            Self::for_range(GlucoseRange::Low)
        } else if mmol <= thresholds.high_mmol {
            Self::for_range(GlucoseRange::InRange)
        } else if mmol <= Thresholds::VERY_HIGH_MMOL {
            Self::for_range(GlucoseRange::High)
        } else {
            Self::for_range(GlucoseRange::VeryHigh)
        }
    }
}

// ============= PDF Drawing Helpers =============

struct PdfOps;

impl PdfOps {
    fn text(text: &str, size: f32, x: f32, y: f32, font: BuiltinFont, color: Color) -> Vec<Op> {
        vec![
            Op::SetFillColor { col: color },
            Op::StartTextSection,
            Op::SetFontSizeBuiltinFont { size: Pt(size), font },
            Op::SetTextCursor { pos: Point::new(Mm(x), Mm(y)) },
            Op::WriteTextBuiltinFont { 
                items: vec![TextItem::Text(text.to_string())],
                font,
            },
            Op::EndTextSection,
        ]
    }

    fn line(x1: f32, y1: f32, x2: f32, y2: f32, color: Color, width: f32) -> Vec<Op> {
        vec![
            Op::SetOutlineColor { col: color },
            Op::SetOutlineThickness { pt: Pt(width) },
            Op::DrawLine {
                line: Line {
                    points: vec![
                        LinePoint { p: Point::new(Mm(x1), Mm(y1)), bezier: false },
                        LinePoint { p: Point::new(Mm(x2), Mm(y2)), bezier: false },
                    ],
                    is_closed: false,
                },
            },
        ]
    }

    fn rect_fill(x: f32, y: f32, width: f32, height: f32, color: Color) -> Vec<Op> {
        vec![
            Op::SetFillColor { col: color },
            Op::DrawPolygon {
                polygon: Polygon {
                    rings: vec![PolygonRing {
                        points: vec![
                            LinePoint { p: Point::new(Mm(x), Mm(y)), bezier: false },
                            LinePoint { p: Point::new(Mm(x + width), Mm(y)), bezier: false },
                            LinePoint { p: Point::new(Mm(x + width), Mm(y + height)), bezier: false },
                            LinePoint { p: Point::new(Mm(x), Mm(y + height)), bezier: false },
                        ],
                    }],
                    mode: PaintMode::Fill,
                    winding_order: WindingOrder::NonZero,
                },
            },
        ]
    }

    fn rect_stroke(x: f32, y: f32, width: f32, height: f32, color: Color, stroke_width: f32) -> Vec<Op> {
        vec![
            Op::SetOutlineColor { col: color },
            Op::SetOutlineThickness { pt: Pt(stroke_width) },
            Op::DrawPolygon {
                polygon: Polygon {
                    rings: vec![PolygonRing {
                        points: vec![
                            LinePoint { p: Point::new(Mm(x), Mm(y)), bezier: false },
                            LinePoint { p: Point::new(Mm(x + width), Mm(y)), bezier: false },
                            LinePoint { p: Point::new(Mm(x + width), Mm(y + height)), bezier: false },
                            LinePoint { p: Point::new(Mm(x), Mm(y + height)), bezier: false },
                        ],
                    }],
                    mode: PaintMode::Stroke,
                    winding_order: WindingOrder::NonZero,
                },
            },
        ]
    }

    fn progress_bar(x: f32, y: f32, width: f32, height: f32, fill_pct: f32, fill_color: Color) -> Vec<Op> {
        let mut ops = Vec::new();
        ops.extend(Self::rect_fill(x, y, width, height, PdfColors::light_gray()));
        if fill_pct > 0.0 {
            ops.extend(Self::rect_fill(x, y, width * fill_pct.min(1.0), height, fill_color));
        }
        ops.extend(Self::rect_stroke(x, y, width, height, PdfColors::gray(), 0.3));
        ops
    }

    fn point(x: f32, y: f32, radius: f32, color: Color) -> Vec<Op> {
        Self::rect_fill(x - radius, y - radius, radius * 2.0, radius * 2.0, color)
    }
}

// ============= PDF Exporter =============

pub struct PdfExporter<'a> {
    readings: &'a [StoredReading],
    stats: &'a ExportStatistics,
    thresholds: Thresholds,
    unit: GlucoseUnit,
}

impl<'a> PdfExporter<'a> {
    pub fn new(
        readings: &'a [StoredReading],
        stats: &'a ExportStatistics,
        thresholds: Thresholds,
        unit: GlucoseUnit,
    ) -> Self {
        Self { readings, stats, thresholds, unit }
    }

    /// Get Y-axis range for charts based on unit
    fn y_range(&self) -> (f32, f32) {
        match self.unit {
            GlucoseUnit::MgDl => (MGDL_Y_MIN, MGDL_Y_MAX),
            GlucoseUnit::MmolL => (MMOL_Y_MIN, MMOL_Y_MAX),
        }
    }

    /// Get low threshold in current unit
    fn threshold_low(&self) -> f32 {
        match self.unit {
            GlucoseUnit::MgDl => self.thresholds.low_mgdl as f32,
            GlucoseUnit::MmolL => self.thresholds.low_mmol as f32,
        }
    }

    /// Get high threshold in current unit
    fn threshold_high(&self) -> f32 {
        match self.unit {
            GlucoseUnit::MgDl => self.thresholds.high_mgdl as f32,
            GlucoseUnit::MmolL => self.thresholds.high_mmol as f32,
        }
    }

    /// Get glucose value from reading in current unit
    fn reading_value(&self, reading: &StoredReading) -> f32 {
        match self.unit {
            GlucoseUnit::MgDl => reading.mg_dl as f32,
            GlucoseUnit::MmolL => reading.mmol_l as f32,
        }
    }

    /// Get color for a value in the current unit
    fn value_color(&self, mg_dl: u16, mmol: f64) -> Color {
        match self.unit {
            GlucoseUnit::MgDl => PdfColors::for_value(mg_dl, &self.thresholds),
            GlucoseUnit::MmolL => PdfColors::for_mmol_value(mmol, &self.thresholds),
        }
    }

    pub fn export<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        let mut doc = PdfDocument::new("Accu-Chek Glucose Report");

        let mut pages = vec![
            self.build_summary_page(),
            self.build_histogram_page(),
            self.build_hourly_page(),
            self.build_time_bins_page(),
            self.build_daily_tir_page(),
            self.build_chart_page(),
        ];

        // Add data pages
        let readings_per_page = 35;
        let total_pages = self.readings.len().div_ceil(readings_per_page);

        for page_num in 0..total_pages {
            let start_idx = page_num * readings_per_page;
            let end_idx = std::cmp::min(start_idx + readings_per_page, self.readings.len());
            let page_readings = &self.readings[start_idx..end_idx];
            pages.push(self.build_data_page(page_readings, page_num + 1, total_pages));
        }

        doc.with_pages(pages);

        let mut warnings = Vec::new();
        let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
        
        let mut file = File::create(path.as_ref())
            .map_err(|e| format!("Failed to create file: {}", e))?;
        file.write_all(&bytes)
            .map_err(|e| format!("Failed to write PDF: {}", e))?;

        Ok(())
    }

    fn build_summary_page(&self) -> PdfPage {
        let mut ops = Vec::new();
        let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

        // Title
        ops.extend(PdfOps::text("Accu-Chek Glucose Report", 24.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 10.0;

        // Date
        let date_str = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        ops.extend(PdfOps::text(&format!("Generated: {}", date_str), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, PdfColors::gray()));
        y -= 15.0;

        ops.extend(PdfOps::line(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y, PdfColors::gray(), 0.5));
        y -= 15.0;

        // Summary Statistics
        ops.extend(PdfOps::text("Summary Statistics", 14.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 10.0;

        if !self.readings.is_empty() {
            let stats = &self.stats.basic;
            let first_date = self.readings.first().map(|r| r.timestamp.as_str()).unwrap_or("N/A");
            let last_date = self.readings.last().map(|r| r.timestamp.as_str()).unwrap_or("N/A");

            ops.extend(PdfOps::text(&format!("Total Readings: {}", stats.mgdl.count), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::black()));
            y -= 7.0;
            ops.extend(PdfOps::text(&format!("Date Range: {} to {}", first_date, last_date), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::black()));
            y -= 7.0;
            
            ops.extend(PdfOps::text(&format!("Average: {:.1} mg/dL ({:.2} mmol/L)", stats.mgdl.mean, stats.mmol.mean), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::black()));
            y -= 7.0;
            
            ops.extend(PdfOps::text(&format!("Minimum: {} mg/dL ({:.1} mmol/L)", stats.mgdl.min, stats.mmol.min), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::for_value(stats.mgdl.min, &self.thresholds)));
            y -= 7.0;
            
            ops.extend(PdfOps::text(&format!("Maximum: {} mg/dL ({:.1} mmol/L)", stats.mgdl.max, stats.mmol.max), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::for_value(stats.mgdl.max, &self.thresholds)));
            y -= 15.0;
        }

        // Time in Range section
        let range_label = match self.unit {
            GlucoseUnit::MgDl => format!("Time in Range ({}-{} mg/dL)", self.thresholds.low_mgdl, self.thresholds.high_mgdl),
            GlucoseUnit::MmolL => format!("Time in Range ({:.1}-{:.1} mmol/L)", self.thresholds.low_mmol, self.thresholds.high_mmol),
        };
        ops.extend(PdfOps::text(&range_label, 14.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 12.0;

        let tir = &self.stats.tir;
        let label_width = 45.0;
        let bar_width = 80.0;
        let bar_height = 12.0;
        let bar_x = MARGIN_MM + label_width;

        // Low
        ops.extend(PdfOps::text("Low:", 10.0, MARGIN_MM, y - 3.0, BuiltinFont::Helvetica, PdfColors::black()));
        ops.extend(PdfOps::progress_bar(bar_x, y - 5.0, bar_width, bar_height, tir.low_percent() as f32 / 100.0, PdfColors::red()));
        ops.extend(PdfOps::text(&format!("{:.1}% ({} readings)", tir.low_percent(), tir.total_low()), 9.0, bar_x + bar_width + 3.0, y - 3.0, BuiltinFont::Helvetica, PdfColors::black()));
        y -= 15.0;

        // In Range
        ops.extend(PdfOps::text("In Range:", 10.0, MARGIN_MM, y - 3.0, BuiltinFont::Helvetica, PdfColors::black()));
        ops.extend(PdfOps::progress_bar(bar_x, y - 5.0, bar_width, bar_height, tir.in_range_percent() as f32 / 100.0, PdfColors::green()));
        ops.extend(PdfOps::text(&format!("{:.1}% ({} readings)", tir.in_range_percent(), tir.in_range), 9.0, bar_x + bar_width + 3.0, y - 3.0, BuiltinFont::Helvetica, PdfColors::black()));
        y -= 15.0;

        // High
        ops.extend(PdfOps::text("High:", 10.0, MARGIN_MM, y - 3.0, BuiltinFont::Helvetica, PdfColors::black()));
        ops.extend(PdfOps::progress_bar(bar_x, y - 5.0, bar_width, bar_height, tir.high_percent() as f32 / 100.0, PdfColors::orange()));
        ops.extend(PdfOps::text(&format!("{:.1}% ({} readings)", tir.high_percent(), tir.total_high()), 9.0, bar_x + bar_width + 3.0, y - 3.0, BuiltinFont::Helvetica, PdfColors::black()));
        y -= 20.0;

        // Distribution section
        ops.extend(PdfOps::text("Reading Distribution", 14.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 12.0;

        let ranges = match self.unit {
            GlucoseUnit::MgDl => vec![
                (format!("< {} (Very Low)", Thresholds::VERY_LOW_MGDL), tir.very_low, GlucoseRange::VeryLow),
                (format!("{}-{} (Low)", Thresholds::VERY_LOW_MGDL, self.thresholds.low_mgdl - 1), tir.low, GlucoseRange::Low),
                (format!("{}-{} (Target)", self.thresholds.low_mgdl, self.thresholds.high_mgdl), tir.in_range, GlucoseRange::InRange),
                (format!("{}-{} (High)", self.thresholds.high_mgdl + 1, Thresholds::VERY_HIGH_MGDL), tir.high, GlucoseRange::High),
                (format!("> {} (Very High)", Thresholds::VERY_HIGH_MGDL), tir.very_high, GlucoseRange::VeryHigh),
            ],
            GlucoseUnit::MmolL => vec![
                (format!("< {:.1} (Very Low)", Thresholds::VERY_LOW_MMOL), tir.very_low, GlucoseRange::VeryLow),
                (format!("{:.1}-{:.1} (Low)", Thresholds::VERY_LOW_MMOL, self.thresholds.low_mmol - 0.1), tir.low, GlucoseRange::Low),
                (format!("{:.1}-{:.1} (Target)", self.thresholds.low_mmol, self.thresholds.high_mmol), tir.in_range, GlucoseRange::InRange),
                (format!("{:.1}-{:.1} (High)", self.thresholds.high_mmol + 0.1, Thresholds::VERY_HIGH_MMOL), tir.high, GlucoseRange::High),
                (format!("> {:.1} (Very High)", Thresholds::VERY_HIGH_MMOL), tir.very_high, GlucoseRange::VeryHigh),
            ],
        };

        let total = tir.total as f32;
        for (label, count, range) in ranges {
            let pct = if total > 0.0 { count as f32 / total } else { 0.0 };
            ops.extend(PdfOps::text(&label, 9.0, MARGIN_MM, y - 2.0, BuiltinFont::Helvetica, PdfColors::black()));
            ops.extend(PdfOps::progress_bar(bar_x, y - 4.0, 70.0, 8.0, pct, PdfColors::for_range(range)));
            ops.extend(PdfOps::text(&format!("{} ({:.1}%)", count, pct * 100.0), 9.0, bar_x + 73.0, y - 2.0, BuiltinFont::Helvetica, PdfColors::black()));
            y -= 10.0;
        }

        // Footer
        ops.extend(PdfOps::text("Page 1 - Summary", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));

        PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops)
    }

    fn build_histogram_page(&self) -> PdfPage {
        let mut ops = Vec::new();
        let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

        ops.extend(PdfOps::text("Glucose Distribution Histogram", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 8.0;
        let bin_desc = match self.unit {
            GlucoseUnit::MgDl => "bin width = 20 mg/dL".to_string(),
            GlucoseUnit::MmolL => "bin width ~1.1 mmol/L".to_string(),
        };
        ops.extend(PdfOps::text(&format!("n = {} readings | {}", self.readings.len(), bin_desc), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, PdfColors::gray()));
        y -= 15.0;

        if self.stats.histogram.is_empty() {
            ops.extend(PdfOps::text("No data available", 12.0, MARGIN_MM, y, BuiltinFont::Helvetica, PdfColors::gray()));
            ops.extend(PdfOps::text("Page 2 - Distribution Histogram", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));
            return PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops);
        }

        // Chart area
        let chart_x = MARGIN_MM + 15.0;
        let chart_y = y - 100.0;
        let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
        let chart_height = 80.0;

        ops.extend(PdfOps::rect_fill(chart_x, chart_y, chart_width, chart_height, PdfColors::light_gray()));
        ops.extend(PdfOps::rect_stroke(chart_x, chart_y, chart_width, chart_height, PdfColors::black(), 0.5));

        let max_count = self.stats.histogram.iter().map(|b| b.count).max().unwrap_or(1) as f32;
        let num_bins = self.stats.histogram.len() as f32;
        let bar_width = (chart_width - 10.0) / num_bins;

        for (i, bin) in self.stats.histogram.iter().enumerate() {
            let bar_height = (bin.count as f32 / max_count) * (chart_height - 10.0);
            let bar_x_pos = chart_x + 5.0 + i as f32 * bar_width;

            let color = if bin.range_end <= self.thresholds.low_mgdl {
                PdfColors::red()
            } else if bin.range_start >= self.thresholds.high_mgdl {
                PdfColors::orange()
            } else {
                PdfColors::green()
            };

            if bin.count > 0 {
                ops.extend(PdfOps::rect_fill(bar_x_pos, chart_y + 5.0, bar_width * 0.9, bar_height, color));
                ops.extend(PdfOps::rect_stroke(bar_x_pos, chart_y + 5.0, bar_width * 0.9, bar_height, PdfColors::black(), 0.3));
            }
        }

        // X-axis labels - show in user's preferred unit
        y = chart_y - 5.0;
        for (i, bin) in self.stats.histogram.iter().enumerate() {
            if i % 4 == 0 {
                let label_x = chart_x + 5.0 + i as f32 * bar_width;
                let label = match self.unit {
                    GlucoseUnit::MgDl => format!("{}", bin.range_start),
                    GlucoseUnit::MmolL => format!("{:.1}", bin.range_start as f64 / 18.0),
                };
                ops.extend(PdfOps::text(&label, 6.0, label_x, y, BuiltinFont::Helvetica, PdfColors::black()));
            }
        }
        ops.extend(PdfOps::text(self.unit.label(), 8.0, chart_x + chart_width / 2.0 - 10.0, y - 8.0, BuiltinFont::Helvetica, PdfColors::black()));
        
        y -= 25.0;

        // Statistics
        ops.extend(PdfOps::text("Distribution Statistics", 12.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 10.0;

        let stats = &self.stats.basic;
        let (mean_str, ci_str, median_str, sd_str, range_str) = match self.unit {
            GlucoseUnit::MgDl => {
                let (ci_low, ci_high) = stats.mgdl.confidence_interval_95();
                (
                    format!("Mean: {:.1} mg/dL", stats.mgdl.mean),
                    format!("95% CI: {:.1} - {:.1} mg/dL", ci_low, ci_high),
                    format!("Median: {} mg/dL", stats.mgdl.median),
                    format!("Standard Deviation: {:.1} mg/dL", stats.mgdl.std_dev),
                    format!("Range: {} - {} mg/dL", stats.mgdl.min, stats.mgdl.max),
                )
            }
            GlucoseUnit::MmolL => {
                let (ci_low, ci_high) = stats.mmol.confidence_interval_95();
                (
                    format!("Mean: {:.2} mmol/L", stats.mmol.mean),
                    format!("95% CI: {:.2} - {:.2} mmol/L", ci_low, ci_high),
                    format!("Median: {:.1} mmol/L", stats.mmol.median),
                    format!("Standard Deviation: {:.2} mmol/L", stats.mmol.std_dev),
                    format!("Range: {:.1} - {:.1} mmol/L", stats.mmol.min, stats.mmol.max),
                )
            }
        };

        ops.extend(PdfOps::text(&mean_str, 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::black()));
        y -= 6.0;
        ops.extend(PdfOps::text(&ci_str, 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::black()));
        y -= 6.0;
        ops.extend(PdfOps::text(&median_str, 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::black()));
        y -= 6.0;
        ops.extend(PdfOps::text(&sd_str, 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::black()));
        y -= 6.0;
        ops.extend(PdfOps::text(&range_str, 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, PdfColors::black()));

        ops.extend(PdfOps::text("Page 2 - Distribution Histogram", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));

        PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops)
    }

    fn build_hourly_page(&self) -> PdfPage {
        let mut ops = Vec::new();
        let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

        ops.extend(PdfOps::text("Glucose by Hour of Day", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 8.0;
        
        let total_readings: usize = self.stats.hourly.iter().map(|h| h.count()).sum();
        ops.extend(PdfOps::text(&format!("n = {} readings across 24 hours", total_readings), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, PdfColors::gray()));
        y -= 15.0;

        // Chart area for boxplot
        let chart_x = MARGIN_MM + 15.0;
        let chart_y = y - 90.0;
        let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
        let chart_height = 70.0;

        ops.extend(PdfOps::rect_fill(chart_x, chart_y, chart_width, chart_height, PdfColors::light_gray()));
        ops.extend(PdfOps::rect_stroke(chart_x, chart_y, chart_width, chart_height, PdfColors::black(), 0.5));

        let (y_min, y_max) = self.y_range();
        let y_range = y_max - y_min;

        // Threshold lines
        let low_y_pos = chart_y + ((self.threshold_low() - y_min) / y_range) * chart_height;
        let high_y_pos = chart_y + ((self.threshold_high() - y_min) / y_range) * chart_height;
        ops.extend(PdfOps::line(chart_x, low_y_pos, chart_x + chart_width, low_y_pos, PdfColors::red(), 0.5));
        ops.extend(PdfOps::line(chart_x, high_y_pos, chart_x + chart_width, high_y_pos, PdfColors::orange(), 0.5));

        // Y-axis labels
        match self.unit {
            GlucoseUnit::MgDl => {
                for val in MGDL_AXIS_LABELS {
                    let label_y = chart_y + ((val as f32 - y_min) / y_range) * chart_height;
                    ops.extend(PdfOps::text(&format!("{}", val), 6.0, MARGIN_MM, label_y - 1.5, BuiltinFont::Helvetica, PdfColors::gray()));
                }
            }
            GlucoseUnit::MmolL => {
                for val in MMOL_AXIS_LABELS {
                    let label_y = chart_y + ((val - y_min) / y_range) * chart_height;
                    ops.extend(PdfOps::text(&format!("{:.0}", val), 6.0, MARGIN_MM, label_y - 1.5, BuiltinFont::Helvetica, PdfColors::gray()));
                }
            }
        }

        // Boxplots
        let box_width = chart_width / 26.0;
        for stat in &self.stats.hourly {
            if let Some(ref s) = stat.stats {
                let box_x_pos = chart_x + (stat.hour as f32 + 1.0) * (chart_width / 25.0) - box_width / 2.0;
                
                let (min_val, q1_val, median_val, q3_val, max_val) = match self.unit {
                    GlucoseUnit::MgDl => (
                        s.mgdl.min as f32, s.mgdl.q1 as f32, s.mgdl.median as f32, 
                        s.mgdl.q3 as f32, s.mgdl.max as f32
                    ),
                    GlucoseUnit::MmolL => (
                        s.mmol.min as f32, s.mmol.q1 as f32, s.mmol.median as f32,
                        s.mmol.q3 as f32, s.mmol.max as f32
                    ),
                };
                
                let min_y = chart_y + ((min_val - y_min) / y_range) * chart_height;
                let q1_y = chart_y + ((q1_val - y_min) / y_range) * chart_height;
                let median_y = chart_y + ((median_val - y_min) / y_range) * chart_height;
                let q3_y = chart_y + ((q3_val - y_min) / y_range) * chart_height;
                let max_y = chart_y + ((max_val - y_min) / y_range) * chart_height;

                // Whiskers
                let whisker_x = box_x_pos + box_width / 2.0;
                ops.extend(PdfOps::line(whisker_x, min_y, whisker_x, q1_y, PdfColors::black(), 0.3));
                ops.extend(PdfOps::line(whisker_x, q3_y, whisker_x, max_y, PdfColors::black(), 0.3));

                // Box
                let box_height = (q3_y - q1_y).max(1.0);
                let box_color = self.value_color(s.mgdl.mean as u16, s.mmol.mean);
                ops.extend(PdfOps::rect_fill(box_x_pos, q1_y, box_width, box_height, box_color));
                ops.extend(PdfOps::rect_stroke(box_x_pos, q1_y, box_width, box_height, PdfColors::black(), 0.3));

                // Median line
                ops.extend(PdfOps::line(box_x_pos, median_y, box_x_pos + box_width, median_y, PdfColors::black(), 0.8));
            }
        }

        // X-axis labels
        y = chart_y - 5.0;
        for hour in (0..24).step_by(3) {
            let label_x = chart_x + (hour as f32 + 1.0) * (chart_width / 25.0) - 3.0;
            ops.extend(PdfOps::text(&format!("{:02}:00", hour), 6.0, label_x, y, BuiltinFont::Helvetica, PdfColors::black()));
        }

        ops.extend(PdfOps::text("Page 3 - Time of Day Analysis", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));

        PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops)
    }

    fn build_time_bins_page(&self) -> PdfPage {
        let mut ops = Vec::new();
        let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

        ops.extend(PdfOps::text("Glucose by Clinical Time Periods", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 8.0;
        ops.extend(PdfOps::text("Boxplot analysis of clinically meaningful time windows", 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, PdfColors::gray()));
        y -= 15.0;

        let chart_x = MARGIN_MM + 15.0;
        let chart_y = y - 90.0;
        let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
        let chart_height = 70.0;

        ops.extend(PdfOps::rect_fill(chart_x, chart_y, chart_width, chart_height, PdfColors::light_gray()));
        ops.extend(PdfOps::rect_stroke(chart_x, chart_y, chart_width, chart_height, PdfColors::black(), 0.5));

        let (y_min, y_max) = self.y_range();
        let y_range = y_max - y_min;

        // Threshold lines
        let low_y_pos = chart_y + ((self.threshold_low() - y_min) / y_range) * chart_height;
        let high_y_pos = chart_y + ((self.threshold_high() - y_min) / y_range) * chart_height;
        ops.extend(PdfOps::line(chart_x, low_y_pos, chart_x + chart_width, low_y_pos, PdfColors::red(), 0.5));
        ops.extend(PdfOps::line(chart_x, high_y_pos, chart_x + chart_width, high_y_pos, PdfColors::orange(), 0.5));

        // Y-axis labels
        match self.unit {
            GlucoseUnit::MgDl => {
                for val in MGDL_AXIS_LABELS {
                    let label_y = chart_y + ((val as f32 - y_min) / y_range) * chart_height;
                    ops.extend(PdfOps::text(&format!("{}", val), 6.0, MARGIN_MM, label_y - 1.5, BuiltinFont::Helvetica, PdfColors::gray()));
                }
            }
            GlucoseUnit::MmolL => {
                for val in MMOL_AXIS_LABELS {
                    let label_y = chart_y + ((val - y_min) / y_range) * chart_height;
                    ops.extend(PdfOps::text(&format!("{:.0}", val), 6.0, MARGIN_MM, label_y - 1.5, BuiltinFont::Helvetica, PdfColors::gray()));
                }
            }
        }

        // Boxplots
        let num_bins = self.stats.time_bins.len();
        let box_width = chart_width / (num_bins + 2) as f32;

        for (i, stat) in self.stats.time_bins.iter().enumerate() {
            if let Some(ref s) = stat.stats {
                let box_x_pos = chart_x + (i as f32 + 1.0) * box_width;
                
                let (min_val, q1_val, median_val, q3_val, max_val) = match self.unit {
                    GlucoseUnit::MgDl => (
                        s.mgdl.min as f32, s.mgdl.q1 as f32, s.mgdl.median as f32, 
                        s.mgdl.q3 as f32, s.mgdl.max as f32
                    ),
                    GlucoseUnit::MmolL => (
                        s.mmol.min as f32, s.mmol.q1 as f32, s.mmol.median as f32,
                        s.mmol.q3 as f32, s.mmol.max as f32
                    ),
                };
                
                let min_y = chart_y + ((min_val - y_min) / y_range) * chart_height;
                let q1_y = chart_y + ((q1_val - y_min) / y_range) * chart_height;
                let median_y = chart_y + ((median_val - y_min) / y_range) * chart_height;
                let q3_y = chart_y + ((q3_val - y_min) / y_range) * chart_height;
                let max_y = chart_y + ((max_val - y_min) / y_range) * chart_height;

                // Whiskers
                let whisker_x = box_x_pos + box_width / 2.0;
                ops.extend(PdfOps::line(whisker_x, min_y, whisker_x, q1_y, PdfColors::black(), 0.3));
                ops.extend(PdfOps::line(whisker_x, q3_y, whisker_x, max_y, PdfColors::black(), 0.3));

                // Box
                let box_height = (q3_y - q1_y).max(1.0);
                let box_color = self.value_color(s.mgdl.mean as u16, s.mmol.mean);
                ops.extend(PdfOps::rect_fill(box_x_pos, q1_y, box_width * 0.8, box_height, box_color));
                ops.extend(PdfOps::rect_stroke(box_x_pos, q1_y, box_width * 0.8, box_height, PdfColors::black(), 0.3));

                // Median line
                ops.extend(PdfOps::line(box_x_pos, median_y, box_x_pos + box_width * 0.8, median_y, PdfColors::black(), 0.8));
            }
        }

        // X-axis labels
        y = chart_y - 5.0;
        for (i, stat) in self.stats.time_bins.iter().enumerate() {
            let label_x = chart_x + (i as f32 + 1.0) * box_width;
            ops.extend(PdfOps::text(&stat.name, 6.0, label_x, y, BuiltinFont::Helvetica, PdfColors::black()));
        }

        ops.extend(PdfOps::text("Page 4 - Clinical Time Periods", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));

        PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops)
    }

    fn build_daily_tir_page(&self) -> PdfPage {
        let mut ops = Vec::new();
        let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

        ops.extend(PdfOps::text("Daily Time-in-Range Trend", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 8.0;
        
        let range_str = self.thresholds.format_range(self.unit);
        ops.extend(PdfOps::text(&format!("Target range: {} | n = {} days", range_str, self.stats.daily.len()), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, PdfColors::gray()));
        y -= 15.0;

        if self.stats.daily.is_empty() {
            ops.extend(PdfOps::text("No daily TIR data available", 12.0, MARGIN_MM, y, BuiltinFont::Helvetica, PdfColors::gray()));
            ops.extend(PdfOps::text("Page 5 - Daily TIR Trend", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));
            return PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops);
        }

        // TIR trend chart
        let chart_x = MARGIN_MM + 15.0;
        let chart_y = y - 70.0;
        let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
        let chart_height = 50.0;

        ops.extend(PdfOps::rect_fill(chart_x, chart_y, chart_width, chart_height, PdfColors::light_gray()));
        ops.extend(PdfOps::rect_stroke(chart_x, chart_y, chart_width, chart_height, PdfColors::black(), 0.5));

        // 70% goal line
        let goal_y = chart_y + 0.7 * chart_height;
        ops.extend(PdfOps::line(chart_x, goal_y, chart_x + chart_width, goal_y, PdfColors::gray(), 0.5));
        ops.extend(PdfOps::text("70%", 6.0, MARGIN_MM, goal_y - 1.5, BuiltinFont::Helvetica, PdfColors::gray()));

        // Y-axis labels
        ops.extend(PdfOps::text("0%", 6.0, MARGIN_MM, chart_y - 1.5, BuiltinFont::Helvetica, PdfColors::gray()));
        ops.extend(PdfOps::text("100%", 6.0, MARGIN_MM, chart_y + chart_height - 1.5, BuiltinFont::Helvetica, PdfColors::gray()));

        // Draw TIR line
        let n = self.stats.daily.len();
        if n > 1 {
            let x_step = chart_width / (n - 1) as f32;
            for i in 0..n - 1 {
                let x1 = chart_x + i as f32 * x_step;
                let x2 = chart_x + (i + 1) as f32 * x_step;
                let y1 = chart_y + (self.stats.daily[i].tir.in_range_percent() as f32 / 100.0) * chart_height;
                let y2 = chart_y + (self.stats.daily[i + 1].tir.in_range_percent() as f32 / 100.0) * chart_height;
                ops.extend(PdfOps::line(x1, y1, x2, y2, PdfColors::green(), 1.0));
            }
            
            for i in 0..n {
                let x = chart_x + i as f32 * x_step;
                let y_pos = chart_y + (self.stats.daily[i].tir.in_range_percent() as f32 / 100.0) * chart_height;
                let color = if self.stats.daily[i].tir.in_range_percent() >= 70.0 { PdfColors::green() } else { PdfColors::orange() };
                ops.extend(PdfOps::point(x, y_pos, 1.5, color));
            }
        }

        y = chart_y - 10.0;

        // Summary stats
        let avg_tir: f64 = self.stats.daily.iter().map(|d| d.tir.in_range_percent()).sum::<f64>() / self.stats.daily.len() as f64;
        let days_at_goal = self.stats.daily.iter().filter(|d| d.tir.in_range_percent() >= 70.0).count();
        
        ops.extend(PdfOps::text(&format!("Average TIR: {:.1}%", avg_tir), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, PdfColors::black()));
        ops.extend(PdfOps::text(&format!("Days at >=70% goal: {}/{} ({:.1}%)", days_at_goal, self.stats.daily.len(), (days_at_goal as f64 / self.stats.daily.len() as f64) * 100.0), 10.0, MARGIN_MM + 60.0, y, BuiltinFont::Helvetica, PdfColors::black()));

        ops.extend(PdfOps::text("Page 5 - Daily TIR Trend", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));

        PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops)
    }

    fn build_chart_page(&self) -> PdfPage {
        let mut ops = Vec::new();
        let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

        ops.extend(PdfOps::text("Glucose Trend Chart", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 20.0;

        if self.readings.is_empty() {
            ops.extend(PdfOps::text("No data to display", 12.0, MARGIN_MM, y, BuiltinFont::Helvetica, PdfColors::gray()));
            ops.extend(PdfOps::text("Page 6 - Glucose Trend Chart", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));
            return PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops);
        }

        let chart_x = MARGIN_MM + 15.0;
        let chart_y = y - 120.0;
        let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
        let chart_height = 100.0;

        ops.extend(PdfOps::rect_fill(chart_x, chart_y, chart_width, chart_height, PdfColors::light_gray()));
        ops.extend(PdfOps::rect_stroke(chart_x, chart_y, chart_width, chart_height, PdfColors::black(), 0.5));

        let (y_min, y_max) = self.y_range();
        let y_range = y_max - y_min;

        // Y-axis labels and grid
        match self.unit {
            GlucoseUnit::MgDl => {
                for mg_dl in [50u16, 100, 150, 200, 250, 300] {
                    let y_pos = chart_y + ((mg_dl as f32 - y_min) / y_range) * chart_height;
                    if y_pos >= chart_y && y_pos <= chart_y + chart_height {
                        ops.extend(PdfOps::line(chart_x, y_pos, chart_x + chart_width, y_pos, PdfColors::light_gray(), 0.3));
                        ops.extend(PdfOps::text(&format!("{}", mg_dl), 7.0, MARGIN_MM, y_pos - 1.5, BuiltinFont::Helvetica, PdfColors::gray()));
                    }
                }
            }
            GlucoseUnit::MmolL => {
                for mmol in [3.0f32, 5.0, 7.0, 10.0, 13.0, 16.0] {
                    let y_pos = chart_y + ((mmol - y_min) / y_range) * chart_height;
                    if y_pos >= chart_y && y_pos <= chart_y + chart_height {
                        ops.extend(PdfOps::line(chart_x, y_pos, chart_x + chart_width, y_pos, PdfColors::light_gray(), 0.3));
                        ops.extend(PdfOps::text(&format!("{:.0}", mmol), 7.0, MARGIN_MM, y_pos - 1.5, BuiltinFont::Helvetica, PdfColors::gray()));
                    }
                }
            }
        }

        // Threshold lines
        let low_y = chart_y + ((self.threshold_low() - y_min) / y_range) * chart_height;
        let high_y = chart_y + ((self.threshold_high() - y_min) / y_range) * chart_height;
        
        ops.extend(PdfOps::line(chart_x, low_y, chart_x + chart_width, low_y, PdfColors::red(), 0.8));
        ops.extend(PdfOps::line(chart_x, high_y, chart_x + chart_width, high_y, PdfColors::orange(), 0.8));

        // Data points and lines
        let n = self.readings.len();
        if n > 1 {
            let x_step = chart_width / (n - 1) as f32;
            
            for i in 0..n - 1 {
                let x1 = chart_x + i as f32 * x_step;
                let x2 = chart_x + (i + 1) as f32 * x_step;
                let val1 = self.reading_value(&self.readings[i]);
                let val2 = self.reading_value(&self.readings[i + 1]);
                let y1 = chart_y + ((val1 - y_min) / y_range) * chart_height;
                let y2 = chart_y + ((val2 - y_min) / y_range) * chart_height;
                
                let y1_clamped = y1.max(chart_y).min(chart_y + chart_height);
                let y2_clamped = y2.max(chart_y).min(chart_y + chart_height);
                
                ops.extend(PdfOps::line(x1, y1_clamped, x2, y2_clamped, PdfColors::blue(), 0.8));
            }

            for i in 0..n {
                let x = chart_x + i as f32 * x_step;
                let val = self.reading_value(&self.readings[i]);
                let y_val = ((val - y_min) / y_range) * chart_height;
                let y_pos = (chart_y + y_val).max(chart_y).min(chart_y + chart_height);
                let color = self.value_color(self.readings[i].mg_dl, self.readings[i].mmol_l);
                ops.extend(PdfOps::point(x, y_pos, 1.5, color));
            }
        }

        y = chart_y - 15.0;

        // Legend
        ops.extend(PdfOps::text("Legend:", 10.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 8.0;
        
        ops.extend(PdfOps::line(MARGIN_MM, y + 2.0, MARGIN_MM + 12.0, y + 2.0, PdfColors::blue(), 1.0));
        ops.extend(PdfOps::text("Glucose readings", 9.0, MARGIN_MM + 15.0, y, BuiltinFont::Helvetica, PdfColors::black()));
        
        let low_label = match self.unit {
            GlucoseUnit::MgDl => format!("Low ({})", self.thresholds.low_mgdl),
            GlucoseUnit::MmolL => format!("Low ({:.1})", self.thresholds.low_mmol),
        };
        let high_label = match self.unit {
            GlucoseUnit::MgDl => format!("High ({})", self.thresholds.high_mgdl),
            GlucoseUnit::MmolL => format!("High ({:.1})", self.thresholds.high_mmol),
        };
        
        ops.extend(PdfOps::line(MARGIN_MM + 70.0, y + 2.0, MARGIN_MM + 82.0, y + 2.0, PdfColors::red(), 1.0));
        ops.extend(PdfOps::text(&low_label, 9.0, MARGIN_MM + 85.0, y, BuiltinFont::Helvetica, PdfColors::black()));
        
        ops.extend(PdfOps::line(MARGIN_MM + 130.0, y + 2.0, MARGIN_MM + 142.0, y + 2.0, PdfColors::orange(), 1.0));
        ops.extend(PdfOps::text(&high_label, 9.0, MARGIN_MM + 145.0, y, BuiltinFont::Helvetica, PdfColors::black()));

        ops.extend(PdfOps::text("Page 6 - Glucose Trend Chart", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));

        PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops)
    }

    fn build_data_page(&self, readings: &[StoredReading], page_num: usize, total_pages: usize) -> PdfPage {
        let mut ops = Vec::new();
        let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

        ops.extend(PdfOps::text("Glucose Readings", 14.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 15.0;

        let col_x = [MARGIN_MM, MARGIN_MM + 32.0, MARGIN_MM + 48.0, MARGIN_MM + 64.0, MARGIN_MM + 80.0, MARGIN_MM + 130.0];

        // Header background
        ops.extend(PdfOps::rect_fill(MARGIN_MM, y - 6.0, PAGE_WIDTH_MM - 2.0 * MARGIN_MM, 8.0, PdfColors::light_gray()));

        // Column headers based on unit preference
        let (col1_label, col2_label) = match self.unit {
            GlucoseUnit::MgDl => ("mg/dL", "mmol/L"),
            GlucoseUnit::MmolL => ("mmol/L", "mg/dL"),
        };
        
        ops.extend(PdfOps::text("Date/Time", 8.0, col_x[0], y - 4.0, BuiltinFont::HelveticaBold, PdfColors::black()));
        ops.extend(PdfOps::text(col1_label, 8.0, col_x[1], y - 4.0, BuiltinFont::HelveticaBold, PdfColors::black()));
        ops.extend(PdfOps::text(col2_label, 8.0, col_x[2], y - 4.0, BuiltinFont::HelveticaBold, PdfColors::black()));
        ops.extend(PdfOps::text("Status", 8.0, col_x[3], y - 4.0, BuiltinFont::HelveticaBold, PdfColors::black()));
        ops.extend(PdfOps::text("Notes", 8.0, col_x[4], y - 4.0, BuiltinFont::HelveticaBold, PdfColors::black()));
        ops.extend(PdfOps::text("Tags", 8.0, col_x[5], y - 4.0, BuiltinFont::HelveticaBold, PdfColors::black()));
        y -= 10.0;

        ops.extend(PdfOps::line(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y, PdfColors::gray(), 0.5));
        y -= 2.0;

        // Data rows
        for (row_idx, reading) in readings.iter().enumerate() {
            let range = self.thresholds.classify(reading.mg_dl);
            let status_color = PdfColors::for_range(range);

            y -= 6.0;
            
            // Alternating row background
            if row_idx % 2 == 1 {
                ops.extend(PdfOps::rect_fill(MARGIN_MM, y - 1.5, PAGE_WIDTH_MM - 2.0 * MARGIN_MM, 7.0, PdfColors::light_gray()));
            }
            
            let value_color = self.value_color(reading.mg_dl, reading.mmol_l);
            
            // Values in preferred unit order
            let (val1, val2) = match self.unit {
                GlucoseUnit::MgDl => (format!("{}", reading.mg_dl), format!("{:.2}", reading.mmol_l)),
                GlucoseUnit::MmolL => (format!("{:.2}", reading.mmol_l), format!("{}", reading.mg_dl)),
            };
            
            ops.extend(PdfOps::text(&reading.timestamp, 7.0, col_x[0], y, BuiltinFont::Helvetica, PdfColors::black()));
            ops.extend(PdfOps::text(&val1, 7.0, col_x[1], y, BuiltinFont::Helvetica, value_color));
            ops.extend(PdfOps::text(&val2, 7.0, col_x[2], y, BuiltinFont::Helvetica, PdfColors::black()));
            ops.extend(PdfOps::text(range.status(), 7.0, col_x[3], y, BuiltinFont::Helvetica, status_color));
            
            let note = reading.note.as_deref().unwrap_or("-");
            let note_display = if note.is_empty() { "-" } else if note.len() > 30 { &note[..30] } else { note };
            ops.extend(PdfOps::text(note_display, 7.0, col_x[4], y, BuiltinFont::Helvetica, PdfColors::black()));
            
            let tags = reading.tags.as_deref().unwrap_or("-");
            let tags_display = if tags.is_empty() { "-" } else { tags };
            ops.extend(PdfOps::text(tags_display, 7.0, col_x[5], y, BuiltinFont::Helvetica, PdfColors::gray()));
        }

        ops.extend(PdfOps::text(&format!("Page {} of {} - Data", page_num + 6, total_pages + 6), 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, PdfColors::gray()));

        PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops)
    }
}
