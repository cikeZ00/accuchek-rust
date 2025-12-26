//! PDF Export functionality for glucose readings

use printpdf::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::storage::{StoredReading, TimeInRange, DailyStats};

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
) -> Result<(), String> {
    let mut doc = PdfDocument::new("Accu-Chek Glucose Report");

    // Page 1: Summary
    let summary_ops = build_summary_page(readings, time_in_range, low_threshold, high_threshold);
    let summary_page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), summary_ops);

    // Page 2: Chart
    let chart_ops = build_chart_page(readings, daily_stats, low_threshold, high_threshold);
    let chart_page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), chart_ops);

    let mut pages = vec![summary_page, chart_page];

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
    ops.extend(text_ops("Page 2 - Chart", 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, COLOR_GRAY));

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
    ops.extend(text_ops(&format!("Page {} of {} - Data", page_num + 2, total_pages + 2), 8.0, MARGIN_MM, MARGIN_MM, BuiltinFont::Helvetica, COLOR_GRAY));

    ops
}
