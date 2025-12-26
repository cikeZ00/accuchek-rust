//! PDF Export functionality for glucose readings

use printpdf::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::storage::{StoredReading, TimeInRange, DailyStats, HourlyStats, TimeBinStats, DailyTIR, HistogramBin};

/// PDF document dimensions (A4)
const PAGE_WIDTH_MM: f32 = 210.0;
const PAGE_HEIGHT_MM: f32 = 297.0;
const MARGIN_MM: f32 = 20.0;

/// Colors
const COLOR_RED: Color = Color::Rgb(Rgb { r: 0.9, g: 0.3, b: 0.3, icc_profile: None });
const COLOR_GREEN: Color = Color::Rgb(Rgb { r: 0.3, g: 0.7, b: 0.3, icc_profile: None });
const COLOR_ORANGE: Color = Color::Rgb(Rgb { r: 0.9, g: 0.6, b: 0.3, icc_profile: None });
const COLOR_BLUE: Color = Color::Rgb(Rgb { r: 0.3, g: 0.5, b: 0.8, icc_profile: None });
const COLOR_BLACK: Color = Color::Rgb(Rgb { r: 0.0, g: 0.0, b: 0.0, icc_profile: None });
const COLOR_GRAY: Color = Color::Rgb(Rgb { r: 0.5, g: 0.5, b: 0.5, icc_profile: None });
const COLOR_LIGHT_GRAY: Color = Color::Rgb(Rgb { r: 0.9, g: 0.9, b: 0.9, icc_profile: None });

fn color_tuple(r: f32, g: f32, b: f32) -> Color {
    Color::Rgb(Rgb { r, g, b, icc_profile: None })
}

/// Export readings to PDF
pub fn export_to_pdf<P: AsRef<Path>>(
    path: P,
    readings: &[StoredReading],
    time_in_range: Option<&TimeInRange>,
    daily_stats: &[DailyStats],
    low_threshold: u16,
    high_threshold: u16,
    hourly_stats: &[HourlyStats],
    time_bin_stats: &[TimeBinStats],
    daily_tir: &[DailyTIR],
    histogram_bins: &[HistogramBin],
) -> Result<(), String> {
    let mut doc = PdfDocument::new("Accu-Chek Glucose Report");

    // Page 1: Summary
    let summary_ops = build_summary_page(readings, time_in_range, low_threshold, high_threshold);
    let summary_page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), summary_ops);

    // Page 2: Distribution Histogram
    let histogram_ops = build_histogram_page(readings, histogram_bins, low_threshold, high_threshold);
    let histogram_page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), histogram_ops);

    // Page 3: Time-of-Day Analysis
    let hourly_ops = build_hourly_page(hourly_stats, low_threshold, high_threshold);
    let hourly_page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), hourly_ops);

    // Page 4: Time Bins Boxplot
    let time_bins_ops = build_time_bins_page(time_bin_stats, low_threshold, high_threshold);
    let time_bins_page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), time_bins_ops);

    // Page 5: Daily TIR Trend
    let daily_tir_ops = build_daily_tir_page(daily_tir, low_threshold, high_threshold);
    let daily_tir_page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), daily_tir_ops);

    // Page 6: Glucose Trend Chart
    let chart_ops = build_chart_page(readings, daily_stats, low_threshold, high_threshold);
    let chart_page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), chart_ops);

    let mut pages = vec![summary_page, histogram_page, hourly_page, time_bins_page, daily_tir_page, chart_page];

    // Data pages
    let readings_per_page = 35;
    let total_pages = (readings.len() + readings_per_page - 1) / readings_per_page;

    for page_num in 0..total_pages {
        let start_idx = page_num * readings_per_page;
        let end_idx = std::cmp::min(start_idx + readings_per_page, readings.len());
        let page_readings = &readings[start_idx..end_idx];

        let data_ops = build_data_page(page_readings, page_num + 1, total_pages, low_threshold, high_threshold);
        let data_page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), data_ops);
        pages.push(data_page);
    }

    doc.with_pages(pages);

    // Save the PDF
    let mut warnings = Vec::new();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
    
    let mut file = File::create(path.as_ref())
        .map_err(|e| format!("Failed to create file: {}", e))?;
    file.write_all(&bytes)
        .map_err(|e| format!("Failed to write PDF: {}", e))?;

    Ok(())
}

// Helper to create text operations
fn text_ops(text: &str, size: f32, x: f32, y: f32, font: BuiltinFont, color: Color) -> Vec<Op> {
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

fn line_ops(x1: f32, y1: f32, x2: f32, y2: f32, color: Color, width: f32) -> Vec<Op> {
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

fn rect_fill_ops(x: f32, y: f32, width: f32, height: f32, color: Color) -> Vec<Op> {
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

fn rect_stroke_ops(x: f32, y: f32, width: f32, height: f32, color: Color, stroke_width: f32) -> Vec<Op> {
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

fn bar_ops(x: f32, y: f32, width: f32, height: f32, fill_pct: f32, fill_color: Color, bg_color: Color) -> Vec<Op> {
    let mut ops = Vec::new();
    // Background
    ops.extend(rect_fill_ops(x, y, width, height, bg_color));
    // Filled portion
    if fill_pct > 0.0 {
        ops.extend(rect_fill_ops(x, y, width * fill_pct.min(1.0), height, fill_color));
    }
    // Border
    ops.extend(rect_stroke_ops(x, y, width, height, COLOR_GRAY, 0.3));
    ops
}

fn point_ops(x: f32, y: f32, radius: f32, color: Color) -> Vec<Op> {
    rect_fill_ops(x - radius, y - radius, radius * 2.0, radius * 2.0, color)
}

fn get_reading_color(mg_dl: u16, low_threshold: u16, high_threshold: u16) -> Color {
    if mg_dl < low_threshold {
        COLOR_RED
    } else if mg_dl > high_threshold {
        COLOR_ORANGE
    } else {
        COLOR_GREEN
    }
}

fn build_summary_page(
    readings: &[StoredReading],
    time_in_range: Option<&TimeInRange>,
    low_threshold: u16,
    high_threshold: u16,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    ops.extend(text_ops("Accu-Chek Glucose Report", 24.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 10.0;

    // Date
    let date_str = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    ops.extend(text_ops(&format!("Generated: {}", date_str), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
    y -= 15.0;

    // Horizontal line
    ops.extend(line_ops(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y, COLOR_GRAY, 0.5));
    y -= 15.0;

    // Summary Statistics
    ops.extend(text_ops("Summary Statistics", 14.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 10.0;

    if !readings.is_empty() {
        let total = readings.len();
        let avg: f64 = readings.iter().map(|r| r.mg_dl as f64).sum::<f64>() / total as f64;
        let min = readings.iter().map(|r| r.mg_dl).min().unwrap_or(0);
        let max = readings.iter().map(|r| r.mg_dl).max().unwrap_or(0);

        let first_date = readings.first().map(|r| r.timestamp.as_str()).unwrap_or("N/A");
        let last_date = readings.last().map(|r| r.timestamp.as_str()).unwrap_or("N/A");

        ops.extend(text_ops(&format!("Total Readings: {}", total), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, COLOR_BLACK));
        y -= 7.0;
        ops.extend(text_ops(&format!("Date Range: {} to {}", first_date, last_date), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, COLOR_BLACK));
        y -= 7.0;
        ops.extend(text_ops(&format!("Average: {:.1} mg/dL ({:.2} mmol/L)", avg, avg / 18.0), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, COLOR_BLACK));
        y -= 7.0;
        
        let min_color = get_reading_color(min, low_threshold, high_threshold);
        ops.extend(text_ops(&format!("Minimum: {} mg/dL", min), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, min_color));
        y -= 7.0;
        
        let max_color = get_reading_color(max, low_threshold, high_threshold);
        ops.extend(text_ops(&format!("Maximum: {} mg/dL", max), 11.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, max_color));
        y -= 15.0;
    }

    // Time in Range section
    ops.extend(text_ops(&format!("Time in Range ({}-{} mg/dL)", low_threshold, high_threshold), 14.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 12.0;

    if let Some(tir) = time_in_range {
        let label_width = 45.0;
        let bar_width = 80.0;
        let bar_height = 12.0;
        let bar_x = MARGIN_MM + label_width;

        // Low
        ops.extend(text_ops("Low:", 10.0, MARGIN_MM, y - 3.0, BuiltinFont::Helvetica, COLOR_BLACK));
        ops.extend(bar_ops(bar_x, y - 5.0, bar_width, bar_height, tir.low_percent as f32 / 100.0, COLOR_RED, COLOR_LIGHT_GRAY));
        ops.extend(text_ops(&format!("{:.1}% ({} readings)", tir.low_percent, tir.low), 9.0, bar_x + bar_width + 3.0, y - 3.0, BuiltinFont::Helvetica, COLOR_BLACK));
        y -= 15.0;

        // In Range
        ops.extend(text_ops("In Range:", 10.0, MARGIN_MM, y - 3.0, BuiltinFont::Helvetica, COLOR_BLACK));
        ops.extend(bar_ops(bar_x, y - 5.0, bar_width, bar_height, tir.normal_percent as f32 / 100.0, COLOR_GREEN, COLOR_LIGHT_GRAY));
        ops.extend(text_ops(&format!("{:.1}% ({} readings)", tir.normal_percent, tir.normal), 9.0, bar_x + bar_width + 3.0, y - 3.0, BuiltinFont::Helvetica, COLOR_BLACK));
        y -= 15.0;

        // High
        ops.extend(text_ops("High:", 10.0, MARGIN_MM, y - 3.0, BuiltinFont::Helvetica, COLOR_BLACK));
        ops.extend(bar_ops(bar_x, y - 5.0, bar_width, bar_height, tir.high_percent as f32 / 100.0, COLOR_ORANGE, COLOR_LIGHT_GRAY));
        ops.extend(text_ops(&format!("{:.1}% ({} readings)", tir.high_percent, tir.high), 9.0, bar_x + bar_width + 3.0, y - 3.0, BuiltinFont::Helvetica, COLOR_BLACK));
        y -= 20.0;
    }

    // Distribution section
    ops.extend(text_ops("Reading Distribution", 14.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 12.0;

    if !readings.is_empty() {
        // Clinical thresholds for dangerous levels (fixed)
        const VERY_LOW_THRESHOLD: u16 = 54;  // Severe hypoglycemia
        const VERY_HIGH_THRESHOLD: u16 = 250; // Risk of ketoacidosis
        
        let very_low = readings.iter().filter(|r| r.mg_dl < VERY_LOW_THRESHOLD).count();
        let low = readings.iter().filter(|r| r.mg_dl >= VERY_LOW_THRESHOLD && r.mg_dl < low_threshold).count();
        let normal = readings.iter().filter(|r| r.mg_dl >= low_threshold && r.mg_dl <= high_threshold).count();
        let high = readings.iter().filter(|r| r.mg_dl > high_threshold && r.mg_dl <= VERY_HIGH_THRESHOLD).count();
        let very_high = readings.iter().filter(|r| r.mg_dl > VERY_HIGH_THRESHOLD).count();
        let total = readings.len() as f32;

        let ranges: Vec<(String, usize, Color)> = vec![
            (format!("< {} (Very Low)", VERY_LOW_THRESHOLD), very_low, color_tuple(0.8, 0.2, 0.2)),
            (format!("{}-{} (Low)", VERY_LOW_THRESHOLD, low_threshold - 1), low, COLOR_RED),
            (format!("{}-{} (Target)", low_threshold, high_threshold), normal, COLOR_GREEN),
            (format!("{}-{} (High)", high_threshold + 1, VERY_HIGH_THRESHOLD), high, COLOR_ORANGE),
            (format!("> {} (Very High)", VERY_HIGH_THRESHOLD), very_high, color_tuple(0.9, 0.3, 0.2)),
        ];

        let label_width = 55.0;
        let bar_width = 70.0;
        let bar_x = MARGIN_MM + label_width;

        for (label, count, color) in ranges {
            let pct = if total > 0.0 { count as f32 / total } else { 0.0 };
            ops.extend(text_ops(&label, 9.0, MARGIN_MM, y - 2.0, BuiltinFont::Helvetica, COLOR_BLACK));
            ops.extend(bar_ops(bar_x, y - 4.0, bar_width, 8.0, pct, color, COLOR_LIGHT_GRAY));
            ops.extend(text_ops(&format!("{} ({:.1}%)", count, pct * 100.0), 9.0, bar_x + bar_width + 3.0, y - 2.0, BuiltinFont::Helvetica, COLOR_BLACK));
            y -= 10.0;
        }
    }

    // Footer
    ops.extend(text_ops("Page 1 - Summary", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, COLOR_GRAY));

    ops
}

fn build_chart_page(
    readings: &[StoredReading],
    _daily_stats: &[DailyStats],
    low_threshold: u16,
    high_threshold: u16,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    ops.extend(text_ops("Glucose Trend Chart", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 20.0;

    if readings.is_empty() {
        ops.extend(text_ops("No data to display", 12.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
        return ops;
    }

    // Chart area
    let chart_x = MARGIN_MM + 15.0;
    let chart_y = y - 120.0;
    let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
    let chart_height = 100.0;

    // Draw chart background
    ops.extend(rect_fill_ops(chart_x, chart_y, chart_width, chart_height, COLOR_LIGHT_GRAY));

    // Draw chart border
    ops.extend(rect_stroke_ops(chart_x, chart_y, chart_width, chart_height, COLOR_BLACK, 0.5));

    // Y-axis labels and grid
    let y_min: f32 = 40.0;
    let y_max: f32 = 300.0;
    let y_range = y_max - y_min;

    for mg_dl in [50, 100, 150, 200, 250, 300].iter() {
        let y_pos = chart_y + ((*mg_dl as f32 - y_min) / y_range) * chart_height;
        if y_pos >= chart_y && y_pos <= chart_y + chart_height {
            // Grid line
            ops.extend(line_ops(chart_x, y_pos, chart_x + chart_width, y_pos, color_tuple(0.8, 0.8, 0.8), 0.3));
            // Label
            ops.extend(text_ops(&format!("{}", mg_dl), 7.0, MARGIN_MM, y_pos - 1.5, BuiltinFont::Helvetica, COLOR_GRAY));
        }
    }

    // Draw threshold lines
    let low_y = chart_y + ((low_threshold as f32 - y_min) / y_range) * chart_height;
    let high_y = chart_y + ((high_threshold as f32 - y_min) / y_range) * chart_height;
    
    ops.extend(line_ops(chart_x, low_y, chart_x + chart_width, low_y, COLOR_RED, 0.8));
    ops.extend(line_ops(chart_x, high_y, chart_x + chart_width, high_y, COLOR_ORANGE, 0.8));

    // Draw data points and lines
    let n = readings.len();
    if n > 1 {
        let x_step = chart_width / (n - 1) as f32;
        
        // Draw connecting lines
        for i in 0..n - 1 {
            let x1 = chart_x + i as f32 * x_step;
            let x2 = chart_x + (i + 1) as f32 * x_step;
            let y1 = chart_y + ((readings[i].mg_dl as f32 - y_min) / y_range) * chart_height;
            let y2 = chart_y + ((readings[i + 1].mg_dl as f32 - y_min) / y_range) * chart_height;
            
            let y1_clamped = y1.max(chart_y).min(chart_y + chart_height);
            let y2_clamped = y2.max(chart_y).min(chart_y + chart_height);
            
            ops.extend(line_ops(x1, y1_clamped, x2, y2_clamped, COLOR_BLUE, 0.8));
        }

        // Draw points
        for i in 0..n {
            let x = chart_x + i as f32 * x_step;
            let y_val = ((readings[i].mg_dl as f32 - y_min) / y_range) * chart_height;
            let y_pos = (chart_y + y_val).max(chart_y).min(chart_y + chart_height);
            let color = get_reading_color(readings[i].mg_dl, low_threshold, high_threshold);
            ops.extend(point_ops(x, y_pos, 1.5, color));
        }
    }

    y = chart_y - 15.0;

    // Legend
    ops.extend(text_ops("Legend:", 10.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 8.0;
    
    ops.extend(line_ops(MARGIN_MM, y + 2.0, MARGIN_MM + 12.0, y + 2.0, COLOR_BLUE, 1.0));
    ops.extend(text_ops("Glucose readings", 9.0, MARGIN_MM + 15.0, y, BuiltinFont::Helvetica, COLOR_BLACK));
    
    ops.extend(line_ops(MARGIN_MM + 70.0, y + 2.0, MARGIN_MM + 82.0, y + 2.0, COLOR_RED, 1.0));
    ops.extend(text_ops(&format!("Low ({})", low_threshold), 9.0, MARGIN_MM + 85.0, y, BuiltinFont::Helvetica, COLOR_BLACK));
    
    ops.extend(line_ops(MARGIN_MM + 120.0, y + 2.0, MARGIN_MM + 132.0, y + 2.0, COLOR_ORANGE, 1.0));
    ops.extend(text_ops(&format!("High ({})", high_threshold), 9.0, MARGIN_MM + 135.0, y, BuiltinFont::Helvetica, COLOR_BLACK));

    // Footer
    ops.extend(text_ops("Page 6 - Glucose Trend Chart", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, COLOR_GRAY));

    ops
}

fn build_data_page(
    readings: &[StoredReading],
    page_num: usize,
    total_pages: usize,
    low_threshold: u16,
    high_threshold: u16,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    ops.extend(text_ops("Glucose Readings", 14.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 15.0;

    // Table header - 6 columns: Date/Time, mg/dL, mmol/L, Status, Notes, Tags
    let col_x = [MARGIN_MM, MARGIN_MM + 32.0, MARGIN_MM + 48.0, MARGIN_MM + 64.0, MARGIN_MM + 80.0, MARGIN_MM + 130.0];

    // Header background
    ops.extend(rect_fill_ops(MARGIN_MM, y - 6.0, PAGE_WIDTH_MM - 2.0 * MARGIN_MM, 8.0, COLOR_LIGHT_GRAY));

    ops.extend(text_ops("Date/Time", 8.0, col_x[0], y - 4.0, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("mg/dL", 8.0, col_x[1], y - 4.0, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("mmol/L", 8.0, col_x[2], y - 4.0, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Status", 8.0, col_x[3], y - 4.0, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Notes", 8.0, col_x[4], y - 4.0, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Tags", 8.0, col_x[5], y - 4.0, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 10.0;

    // Horizontal line
    ops.extend(line_ops(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y, COLOR_GRAY, 0.5));
    y -= 2.0;

    // Data rows
    for (row_idx, reading) in readings.iter().enumerate() {
        let (status_text, status_color) = if reading.mg_dl < low_threshold {
            ("LOW", COLOR_RED)
        } else if reading.mg_dl > high_threshold {
            ("HIGH", COLOR_ORANGE)
        } else {
            ("OK", COLOR_GREEN)
        };

        // Notes column - calculate lines needed first
        let note = reading.note.as_deref().unwrap_or("-");
        let note_display = if note.is_empty() { "-" } else { note };
        let max_note_chars = 35;
        
        // Calculate how many lines the note will take
        let note_lines: Vec<&str> = if note_display.len() <= max_note_chars {
            vec![note_display]
        } else {
            let mut lines = Vec::new();
            let mut remaining = note_display;
            while !remaining.is_empty() {
                let (line, rest) = if remaining.len() <= max_note_chars {
                    (remaining, "")
                } else {
                    let break_at = remaining[..max_note_chars]
                        .rfind(' ')
                        .unwrap_or(max_note_chars);
                    (&remaining[..break_at], remaining[break_at..].trim_start())
                };
                lines.push(line);
                remaining = rest;
            }
            lines
        };
        
        let num_lines = note_lines.len();
        let line_height = 3.5_f32;
        let row_height = if num_lines > 1 {
            6.0 + (num_lines as f32 - 1.0) * line_height
        } else {
            6.0
        };
        
        y -= row_height;
        
        // Draw alternating row background
        if row_idx % 2 == 1 {
            ops.extend(rect_fill_ops(MARGIN_MM, y - 1.5, PAGE_WIDTH_MM - 2.0 * MARGIN_MM, row_height + 1.0, color_tuple(0.95, 0.95, 0.95)));
        }
        
        // Calculate vertical center offset for single-line columns
        let center_offset = if num_lines > 1 {
            ((num_lines as f32 - 1.0) * line_height) / 2.0
        } else {
            0.0
        };
        
        // Draw single-line columns centered vertically
        let text_y = y + center_offset;
        let value_color = get_reading_color(reading.mg_dl, low_threshold, high_threshold);
        
        ops.extend(text_ops(&reading.timestamp, 7.0, col_x[0], text_y, BuiltinFont::Helvetica, COLOR_BLACK));
        ops.extend(text_ops(&format!("{}", reading.mg_dl), 7.0, col_x[1], text_y, BuiltinFont::Helvetica, value_color));
        ops.extend(text_ops(&format!("{:.2}", reading.mmol_l), 7.0, col_x[2], text_y, BuiltinFont::Helvetica, COLOR_BLACK));
        ops.extend(text_ops(status_text, 7.0, col_x[3], text_y, BuiltinFont::Helvetica, status_color));
        
        // Draw notes - multiple lines from top
        let notes_start_y = y + (num_lines as f32 - 1.0) * line_height;
        for (i, line) in note_lines.iter().enumerate() {
            let line_y = notes_start_y - (i as f32 * line_height);
            let font_size = if num_lines > 1 { 6.0 } else { 7.0 };
            ops.extend(text_ops(line, font_size, col_x[4], line_y, BuiltinFont::Helvetica, COLOR_BLACK));
        }
        
        // Tags column - centered vertically
        let tags = reading.tags.as_deref().unwrap_or("-");
        let tags_display = if tags.is_empty() { "-" } else { tags };
        ops.extend(text_ops(tags_display, 7.0, col_x[5], text_y, BuiltinFont::Helvetica, COLOR_GRAY));
    }

    // Footer
    ops.extend(text_ops(&format!("Page {} of {} - Data", page_num + 6, total_pages + 6), 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, COLOR_GRAY));

    ops
}

// ============= NEW VISUALIZATION PAGES =============

fn build_histogram_page(
    readings: &[StoredReading],
    histogram_bins: &[HistogramBin],
    low_threshold: u16,
    high_threshold: u16,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    ops.extend(text_ops("Glucose Distribution Histogram", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 8.0;
    ops.extend(text_ops(&format!("n = {} readings | bin width = 20 mg/dL", readings.len()), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
    y -= 15.0;

    if histogram_bins.is_empty() || readings.is_empty() {
        ops.extend(text_ops("No data available", 12.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
        return ops;
    }

    // Chart area
    let chart_x = MARGIN_MM + 15.0;
    let chart_y = y - 100.0;
    let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
    let chart_height = 80.0;

    // Draw chart background
    ops.extend(rect_fill_ops(chart_x, chart_y, chart_width, chart_height, COLOR_LIGHT_GRAY));
    ops.extend(rect_stroke_ops(chart_x, chart_y, chart_width, chart_height, COLOR_BLACK, 0.5));

    // Find max count for scaling
    let max_count = histogram_bins.iter().map(|b| b.count).max().unwrap_or(1) as f32;
    let num_bins = histogram_bins.len() as f32;
    let bar_width = (chart_width - 10.0) / num_bins;

    // Draw histogram bars
    for (i, bin) in histogram_bins.iter().enumerate() {
        let bar_height = (bin.count as f32 / max_count) * (chart_height - 10.0);
        let bar_x = chart_x + 5.0 + i as f32 * bar_width;
        let bar_y = chart_y + 5.0;

        let color = if bin.range_end <= low_threshold {
            COLOR_RED
        } else if bin.range_start >= high_threshold {
            COLOR_ORANGE
        } else {
            COLOR_GREEN
        };

        if bin.count > 0 {
            ops.extend(rect_fill_ops(bar_x, bar_y, bar_width * 0.9, bar_height, color));
            ops.extend(rect_stroke_ops(bar_x, bar_y, bar_width * 0.9, bar_height, COLOR_BLACK, 0.3));
        }
    }

    // X-axis labels (every 4th bin)
    y = chart_y - 5.0;
    for (i, bin) in histogram_bins.iter().enumerate() {
        if i % 4 == 0 {
            let label_x = chart_x + 5.0 + i as f32 * bar_width;
            ops.extend(text_ops(&format!("{}", bin.range_start), 6.0, label_x, y, BuiltinFont::Helvetica, COLOR_BLACK));
        }
    }
    ops.extend(text_ops("mg/dL", 8.0, chart_x + chart_width / 2.0 - 10.0, y - 8.0, BuiltinFont::Helvetica, COLOR_BLACK));
    
    y -= 25.0;

    // Statistics summary
    ops.extend(text_ops("Distribution Statistics", 12.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 10.0;

    let values: Vec<f64> = readings.iter().map(|r| r.mg_dl as f64).collect();
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let mut sorted = values.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    let variance: f64 = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    let std_dev = variance.sqrt();
    let se = std_dev / (values.len() as f64).sqrt();
    let ci_low = mean - 1.96 * se;
    let ci_high = mean + 1.96 * se;

    ops.extend(text_ops(&format!("Mean: {:.1} mg/dL", mean), 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, COLOR_BLACK));
    y -= 6.0;
    ops.extend(text_ops(&format!("95% CI: {:.1} - {:.1} mg/dL", ci_low, ci_high), 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, COLOR_BLACK));
    y -= 6.0;
    ops.extend(text_ops(&format!("Median: {:.1} mg/dL", median), 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, COLOR_BLACK));
    y -= 6.0;
    ops.extend(text_ops(&format!("Standard Deviation: {:.1} mg/dL", std_dev), 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, COLOR_BLACK));
    y -= 6.0;
    ops.extend(text_ops(&format!("Range: {} - {} mg/dL", sorted[0] as u16, sorted[sorted.len()-1] as u16), 10.0, MARGIN_MM + 5.0, y, BuiltinFont::Helvetica, COLOR_BLACK));

    // Footer
    ops.extend(text_ops("Page 2 - Distribution Histogram", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, COLOR_GRAY));

    ops
}

fn build_hourly_page(
    hourly_stats: &[HourlyStats],
    low_threshold: u16,
    high_threshold: u16,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    ops.extend(text_ops("Glucose by Hour of Day", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 8.0;
    
    let total_readings: usize = hourly_stats.iter().map(|h| h.count).sum();
    ops.extend(text_ops(&format!("n = {} readings across 24 hours", total_readings), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
    y -= 15.0;

    if hourly_stats.is_empty() {
        ops.extend(text_ops("No hourly data available", 12.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
        return ops;
    }

    // Chart area for boxplot
    let chart_x = MARGIN_MM + 15.0;
    let chart_y = y - 90.0;
    let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
    let chart_height = 70.0;

    // Draw chart background
    ops.extend(rect_fill_ops(chart_x, chart_y, chart_width, chart_height, COLOR_LIGHT_GRAY));
    ops.extend(rect_stroke_ops(chart_x, chart_y, chart_width, chart_height, COLOR_BLACK, 0.5));

    // Y-axis scale
    let y_min = 40.0_f32;
    let y_max = 300.0_f32;
    let y_range = y_max - y_min;

    // Draw threshold lines
    let low_y_pos = chart_y + ((low_threshold as f32 - y_min) / y_range) * chart_height;
    let high_y_pos = chart_y + ((high_threshold as f32 - y_min) / y_range) * chart_height;
    ops.extend(line_ops(chart_x, low_y_pos, chart_x + chart_width, low_y_pos, COLOR_RED, 0.5));
    ops.extend(line_ops(chart_x, high_y_pos, chart_x + chart_width, high_y_pos, COLOR_ORANGE, 0.5));

    // Y-axis labels
    for val in [50, 100, 150, 200, 250].iter() {
        let label_y = chart_y + ((*val as f32 - y_min) / y_range) * chart_height;
        ops.extend(text_ops(&format!("{}", val), 6.0, MARGIN_MM, label_y - 1.5, BuiltinFont::Helvetica, COLOR_GRAY));
    }

    // Draw boxplots for each hour
    let box_width = chart_width / 26.0;
    for stat in hourly_stats.iter() {
        if stat.count == 0 {
            continue;
        }

        let box_x = chart_x + (stat.hour as f32 + 1.0) * (chart_width / 25.0) - box_width / 2.0;
        
        // Calculate y positions
        let min_y = chart_y + ((stat.min as f32 - y_min) / y_range) * chart_height;
        let q1_y = chart_y + ((stat.q1 as f32 - y_min) / y_range) * chart_height;
        let median_y = chart_y + ((stat.median as f32 - y_min) / y_range) * chart_height;
        let q3_y = chart_y + ((stat.q3 as f32 - y_min) / y_range) * chart_height;
        let max_y = chart_y + ((stat.max as f32 - y_min) / y_range) * chart_height;

        // Whiskers
        let whisker_x = box_x + box_width / 2.0;
        ops.extend(line_ops(whisker_x, min_y, whisker_x, q1_y, COLOR_BLACK, 0.3));
        ops.extend(line_ops(whisker_x, q3_y, whisker_x, max_y, COLOR_BLACK, 0.3));

        // Box
        let box_height = q3_y - q1_y;
        let box_color = if stat.mean < low_threshold as f64 {
            COLOR_RED
        } else if stat.mean > high_threshold as f64 {
            COLOR_ORANGE
        } else {
            COLOR_GREEN
        };
        ops.extend(rect_fill_ops(box_x, q1_y, box_width, box_height.max(1.0), box_color));
        ops.extend(rect_stroke_ops(box_x, q1_y, box_width, box_height.max(1.0), COLOR_BLACK, 0.3));

        // Median line
        ops.extend(line_ops(box_x, median_y, box_x + box_width, median_y, COLOR_BLACK, 0.8));
    }

    // X-axis labels (every 3 hours)
    y = chart_y - 5.0;
    for hour in (0..24).step_by(3) {
        let label_x = chart_x + (hour as f32 + 1.0) * (chart_width / 25.0) - 3.0;
        ops.extend(text_ops(&format!("{:02}:00", hour), 6.0, label_x, y, BuiltinFont::Helvetica, COLOR_BLACK));
    }

    y -= 20.0;

    // Statistics table
    ops.extend(text_ops("Hourly Statistics Summary", 12.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 10.0;

    // Header
    let col_x = [MARGIN_MM, MARGIN_MM + 20.0, MARGIN_MM + 40.0, MARGIN_MM + 70.0, MARGIN_MM + 100.0, MARGIN_MM + 130.0];
    ops.extend(text_ops("Hour", 8.0, col_x[0], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("n", 8.0, col_x[1], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Mean+/-SD", 8.0, col_x[2], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Median", 8.0, col_x[3], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("IQR", 8.0, col_x[4], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Range", 8.0, col_x[5], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 5.0;
    ops.extend(line_ops(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y, COLOR_GRAY, 0.3));
    y -= 5.0;

    // Data rows (show hours with data)
    for stat in hourly_stats.iter() {
        if stat.count > 0 && y > MARGIN_MM + 15.0 {
            ops.extend(text_ops(&format!("{:02}:00", stat.hour), 7.0, col_x[0], y, BuiltinFont::Helvetica, COLOR_BLACK));
            ops.extend(text_ops(&format!("{}", stat.count), 7.0, col_x[1], y, BuiltinFont::Helvetica, COLOR_BLACK));
            ops.extend(text_ops(&format!("{:.0}+/-{:.0}", stat.mean, stat.std_dev), 7.0, col_x[2], y, BuiltinFont::Helvetica, COLOR_BLACK));
            ops.extend(text_ops(&format!("{}", stat.median), 7.0, col_x[3], y, BuiltinFont::Helvetica, COLOR_BLACK));
            ops.extend(text_ops(&format!("{}-{}", stat.q1, stat.q3), 7.0, col_x[4], y, BuiltinFont::Helvetica, COLOR_BLACK));
            ops.extend(text_ops(&format!("{}-{}", stat.min, stat.max), 7.0, col_x[5], y, BuiltinFont::Helvetica, COLOR_BLACK));
            y -= 5.0;
        }
    }

    // Footer
    ops.extend(text_ops("Page 3 - Time of Day Analysis", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, COLOR_GRAY));

    ops
}

fn build_time_bins_page(
    time_bin_stats: &[TimeBinStats],
    low_threshold: u16,
    high_threshold: u16,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    ops.extend(text_ops("Glucose by Clinical Time Periods", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 8.0;
    ops.extend(text_ops("Boxplot analysis of clinically meaningful time windows", 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
    y -= 15.0;

    if time_bin_stats.is_empty() {
        ops.extend(text_ops("No time bin data available", 12.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
        return ops;
    }

    // Chart area
    let chart_x = MARGIN_MM + 15.0;
    let chart_y = y - 90.0;
    let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
    let chart_height = 70.0;

    ops.extend(rect_fill_ops(chart_x, chart_y, chart_width, chart_height, COLOR_LIGHT_GRAY));
    ops.extend(rect_stroke_ops(chart_x, chart_y, chart_width, chart_height, COLOR_BLACK, 0.5));

    let y_min = 40.0_f32;
    let y_max = 300.0_f32;
    let y_range = y_max - y_min;

    // Threshold lines
    let low_y_pos = chart_y + ((low_threshold as f32 - y_min) / y_range) * chart_height;
    let high_y_pos = chart_y + ((high_threshold as f32 - y_min) / y_range) * chart_height;
    ops.extend(line_ops(chart_x, low_y_pos, chart_x + chart_width, low_y_pos, COLOR_RED, 0.5));
    ops.extend(line_ops(chart_x, high_y_pos, chart_x + chart_width, high_y_pos, COLOR_ORANGE, 0.5));

    // Y-axis labels
    for val in [50, 100, 150, 200, 250].iter() {
        let label_y = chart_y + ((*val as f32 - y_min) / y_range) * chart_height;
        ops.extend(text_ops(&format!("{}", val), 6.0, MARGIN_MM, label_y - 1.5, BuiltinFont::Helvetica, COLOR_GRAY));
    }

    // Draw boxplots
    let num_bins = time_bin_stats.len();
    let box_width = chart_width / (num_bins + 2) as f32;

    for (i, stat) in time_bin_stats.iter().enumerate() {
        if stat.count == 0 {
            continue;
        }

        let box_x = chart_x + (i as f32 + 1.0) * box_width;
        
        let min_y = chart_y + ((stat.min as f32 - y_min) / y_range) * chart_height;
        let q1_y = chart_y + ((stat.q1 as f32 - y_min) / y_range) * chart_height;
        let median_y = chart_y + ((stat.median as f32 - y_min) / y_range) * chart_height;
        let q3_y = chart_y + ((stat.q3 as f32 - y_min) / y_range) * chart_height;
        let max_y = chart_y + ((stat.max as f32 - y_min) / y_range) * chart_height;

        // Whiskers
        let whisker_x = box_x + box_width / 2.0;
        ops.extend(line_ops(whisker_x, min_y, whisker_x, q1_y, COLOR_BLACK, 0.3));
        ops.extend(line_ops(whisker_x, q3_y, whisker_x, max_y, COLOR_BLACK, 0.3));

        // Box
        let box_height = q3_y - q1_y;
        let box_color = if stat.mean < low_threshold as f64 {
            COLOR_RED
        } else if stat.mean > high_threshold as f64 {
            COLOR_ORANGE
        } else {
            COLOR_GREEN
        };
        ops.extend(rect_fill_ops(box_x, q1_y, box_width * 0.8, box_height.max(1.0), box_color));
        ops.extend(rect_stroke_ops(box_x, q1_y, box_width * 0.8, box_height.max(1.0), COLOR_BLACK, 0.3));

        // Median line
        ops.extend(line_ops(box_x, median_y, box_x + box_width * 0.8, median_y, COLOR_BLACK, 0.8));
    }

    // X-axis labels
    y = chart_y - 5.0;
    for (i, stat) in time_bin_stats.iter().enumerate() {
        let label_x = chart_x + (i as f32 + 1.0) * box_width;
        ops.extend(text_ops(&stat.name, 6.0, label_x, y, BuiltinFont::Helvetica, COLOR_BLACK));
    }

    y -= 20.0;

    // Statistics table
    ops.extend(text_ops("Time Period Statistics (with 95% CI)", 12.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 10.0;

    // Header
    let col_x = [MARGIN_MM, MARGIN_MM + 30.0, MARGIN_MM + 50.0, MARGIN_MM + 65.0, MARGIN_MM + 95.0, MARGIN_MM + 120.0, MARGIN_MM + 145.0];
    ops.extend(text_ops("Period", 8.0, col_x[0], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Hours", 8.0, col_x[1], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("n", 8.0, col_x[2], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Mean+/-SD", 8.0, col_x[3], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Median", 8.0, col_x[4], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("IQR", 8.0, col_x[5], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("95% CI", 8.0, col_x[6], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 5.0;
    ops.extend(line_ops(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y, COLOR_GRAY, 0.3));
    y -= 6.0;

    for stat in time_bin_stats {
        let value_color = get_reading_color(stat.mean as u16, low_threshold, high_threshold);
        ops.extend(text_ops(&stat.name, 7.0, col_x[0], y, BuiltinFont::Helvetica, COLOR_BLACK));
        ops.extend(text_ops(&stat.description, 7.0, col_x[1], y, BuiltinFont::Helvetica, COLOR_GRAY));
        ops.extend(text_ops(&format!("{}", stat.count), 7.0, col_x[2], y, BuiltinFont::Helvetica, COLOR_BLACK));
        
        if stat.count > 0 {
            ops.extend(text_ops(&format!("{:.0}+/-{:.0}", stat.mean, stat.std_dev), 7.0, col_x[3], y, BuiltinFont::Helvetica, value_color));
            ops.extend(text_ops(&format!("{}", stat.median), 7.0, col_x[4], y, BuiltinFont::Helvetica, COLOR_BLACK));
            ops.extend(text_ops(&format!("{}-{}", stat.q1, stat.q3), 7.0, col_x[5], y, BuiltinFont::Helvetica, COLOR_BLACK));
            
            if stat.count > 1 {
                let se = stat.std_dev / (stat.count as f64).sqrt();
                let ci_low = stat.mean - 1.96 * se;
                let ci_high = stat.mean + 1.96 * se;
                ops.extend(text_ops(&format!("{:.0}-{:.0}", ci_low, ci_high), 7.0, col_x[6], y, BuiltinFont::Helvetica, COLOR_BLACK));
            } else {
                ops.extend(text_ops("-", 7.0, col_x[6], y, BuiltinFont::Helvetica, COLOR_GRAY));
            }
        } else {
            ops.extend(text_ops("-", 7.0, col_x[3], y, BuiltinFont::Helvetica, COLOR_GRAY));
            ops.extend(text_ops("-", 7.0, col_x[4], y, BuiltinFont::Helvetica, COLOR_GRAY));
            ops.extend(text_ops("-", 7.0, col_x[5], y, BuiltinFont::Helvetica, COLOR_GRAY));
            ops.extend(text_ops("-", 7.0, col_x[6], y, BuiltinFont::Helvetica, COLOR_GRAY));
        }
        y -= 7.0;
    }

    // Footer
    ops.extend(text_ops("Page 4 - Clinical Time Periods", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, COLOR_GRAY));

    ops
}

fn build_daily_tir_page(
    daily_tir: &[DailyTIR],
    low_threshold: u16,
    high_threshold: u16,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    ops.extend(text_ops("Daily Time-in-Range Trend", 16.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 8.0;
    ops.extend(text_ops(&format!("Target range: {}-{} mg/dL | n = {} days", low_threshold, high_threshold, daily_tir.len()), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
    y -= 15.0;

    if daily_tir.is_empty() {
        ops.extend(text_ops("No daily TIR data available", 12.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_GRAY));
        return ops;
    }

    // TIR trend chart
    let chart_x = MARGIN_MM + 15.0;
    let chart_y = y - 70.0;
    let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
    let chart_height = 50.0;

    ops.extend(rect_fill_ops(chart_x, chart_y, chart_width, chart_height, COLOR_LIGHT_GRAY));
    ops.extend(rect_stroke_ops(chart_x, chart_y, chart_width, chart_height, COLOR_BLACK, 0.5));

    // 70% goal line
    let goal_y = chart_y + 0.7 * chart_height;
    ops.extend(line_ops(chart_x, goal_y, chart_x + chart_width, goal_y, COLOR_GRAY, 0.5));
    ops.extend(text_ops("70%", 6.0, MARGIN_MM, goal_y - 1.5, BuiltinFont::Helvetica, COLOR_GRAY));

    // Y-axis labels
    ops.extend(text_ops("0%", 6.0, MARGIN_MM, chart_y - 1.5, BuiltinFont::Helvetica, COLOR_GRAY));
    ops.extend(text_ops("100%", 6.0, MARGIN_MM, chart_y + chart_height - 1.5, BuiltinFont::Helvetica, COLOR_GRAY));

    // Draw TIR line
    let n = daily_tir.len();
    if n > 1 {
        let x_step = chart_width / (n - 1) as f32;
        for i in 0..n - 1 {
            let x1 = chart_x + i as f32 * x_step;
            let x2 = chart_x + (i + 1) as f32 * x_step;
            let y1 = chart_y + (daily_tir[i].in_range_pct as f32 / 100.0) * chart_height;
            let y2 = chart_y + (daily_tir[i + 1].in_range_pct as f32 / 100.0) * chart_height;
            ops.extend(line_ops(x1, y1, x2, y2, COLOR_GREEN, 1.0));
        }
        
        // Draw points
        for i in 0..n {
            let x = chart_x + i as f32 * x_step;
            let y_pos = chart_y + (daily_tir[i].in_range_pct as f32 / 100.0) * chart_height;
            let color = if daily_tir[i].in_range_pct >= 70.0 { COLOR_GREEN } else { COLOR_ORANGE };
            ops.extend(point_ops(x, y_pos, 1.5, color));
        }
    }

    y = chart_y - 10.0;

    // Summary stats
    let avg_tir: f64 = daily_tir.iter().map(|d| d.in_range_pct).sum::<f64>() / daily_tir.len() as f64;
    let days_at_goal = daily_tir.iter().filter(|d| d.in_range_pct >= 70.0).count();
    
    ops.extend(text_ops(&format!("Average TIR: {:.1}%", avg_tir), 10.0, MARGIN_MM, y, BuiltinFont::Helvetica, COLOR_BLACK));
    ops.extend(text_ops(&format!("Days at >=70% goal: {}/{} ({:.1}%)", days_at_goal, daily_tir.len(), (days_at_goal as f64 / daily_tir.len() as f64) * 100.0), 10.0, MARGIN_MM + 60.0, y, BuiltinFont::Helvetica, COLOR_BLACK));

    y -= 15.0;

    // Daily details table
    ops.extend(text_ops("Daily Time-in-Range Details", 12.0, MARGIN_MM, y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 10.0;

    let col_x = [MARGIN_MM, MARGIN_MM + 30.0, MARGIN_MM + 50.0, MARGIN_MM + 80.0, MARGIN_MM + 110.0, MARGIN_MM + 140.0];
    ops.extend(text_ops("Date", 8.0, col_x[0], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Count", 8.0, col_x[1], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Low %", 8.0, col_x[2], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("In Range %", 8.0, col_x[3], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("High %", 8.0, col_x[4], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    ops.extend(text_ops("Goal", 8.0, col_x[5], y, BuiltinFont::HelveticaBold, COLOR_BLACK));
    y -= 5.0;
    ops.extend(line_ops(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y, COLOR_GRAY, 0.3));
    y -= 5.0;

    for day in daily_tir.iter().rev().take(25) {  // Show most recent 25 days
        if y < MARGIN_MM + 15.0 {
            break;
        }
        
        ops.extend(text_ops(&day.date, 7.0, col_x[0], y, BuiltinFont::Helvetica, COLOR_BLACK));
        ops.extend(text_ops(&format!("{}", day.total), 7.0, col_x[1], y, BuiltinFont::Helvetica, COLOR_BLACK));
        ops.extend(text_ops(&format!("{:.1}%", day.low_pct), 7.0, col_x[2], y, BuiltinFont::Helvetica, COLOR_RED));
        ops.extend(text_ops(&format!("{:.1}%", day.in_range_pct), 7.0, col_x[3], y, BuiltinFont::Helvetica, COLOR_GREEN));
        ops.extend(text_ops(&format!("{:.1}%", day.high_pct), 7.0, col_x[4], y, BuiltinFont::Helvetica, COLOR_ORANGE));
        
        let goal_text = if day.in_range_pct >= 70.0 { "Yes" } else { "-" };
        let goal_color = if day.in_range_pct >= 70.0 { COLOR_GREEN } else { COLOR_GRAY };
        ops.extend(text_ops(goal_text, 7.0, col_x[5], y, BuiltinFont::Helvetica, goal_color));
        
        y -= 5.0;
    }

    // Footer
    ops.extend(text_ops("Page 5 - Daily TIR Trend", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, COLOR_GRAY));

    ops
}
