//! PDF Export functionality for glucose readings

use printpdf::*;
use printpdf::path::{PaintMode, WindingOrder};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::storage::{StoredReading, TimeInRange, DailyStats};

/// PDF document dimensions (A4)
const PAGE_WIDTH_MM: f32 = 210.0;
const PAGE_HEIGHT_MM: f32 = 297.0;
const MARGIN_MM: f32 = 20.0;

/// Colors
const COLOR_RED: (f32, f32, f32) = (0.9, 0.3, 0.3);
const COLOR_GREEN: (f32, f32, f32) = (0.3, 0.7, 0.3);
const COLOR_ORANGE: (f32, f32, f32) = (0.9, 0.6, 0.3);
const COLOR_BLUE: (f32, f32, f32) = (0.3, 0.5, 0.8);
const COLOR_BLACK: (f32, f32, f32) = (0.0, 0.0, 0.0);
const COLOR_GRAY: (f32, f32, f32) = (0.5, 0.5, 0.5);
const COLOR_LIGHT_GRAY: (f32, f32, f32) = (0.9, 0.9, 0.9);

/// Export readings to PDF
pub fn export_to_pdf<P: AsRef<Path>>(
    path: P,
    readings: &[StoredReading],
    time_in_range: Option<&TimeInRange>,
    daily_stats: &[DailyStats],
    low_threshold: u16,
    high_threshold: u16,
) -> Result<(), String> {
    let (doc, page1, layer1) = PdfDocument::new(
        "Accu-Chek Glucose Report",
        Mm(PAGE_WIDTH_MM),
        Mm(PAGE_HEIGHT_MM),
        "Summary",
    );

    let font = doc.add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("Failed to load font: {}", e))?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| format!("Failed to load bold font: {}", e))?;

    let current_layer = doc.get_page(page1).get_layer(layer1);

    // Draw first page: Summary
    draw_summary_page(
        &current_layer,
        &font,
        &font_bold,
        readings,
        time_in_range,
        low_threshold,
        high_threshold,
    );

    // Second page: Chart
    let (page2, layer2) = doc.add_page(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), "Chart");
    let chart_layer = doc.get_page(page2).get_layer(layer2);
    draw_chart_page(
        &chart_layer,
        &font,
        &font_bold,
        readings,
        daily_stats,
        low_threshold,
        high_threshold,
    );

    // Remaining pages: Data table
    let readings_per_page = 35;
    let total_pages = (readings.len() + readings_per_page - 1) / readings_per_page;

    for page_num in 0..total_pages {
        let start_idx = page_num * readings_per_page;
        let end_idx = std::cmp::min(start_idx + readings_per_page, readings.len());
        let page_readings = &readings[start_idx..end_idx];

        let (page, layer) = doc.add_page(
            Mm(PAGE_WIDTH_MM),
            Mm(PAGE_HEIGHT_MM),
            &format!("Data Page {}", page_num + 1),
        );
        let data_layer = doc.get_page(page).get_layer(layer);
        draw_data_page(
            &data_layer,
            &font,
            &font_bold,
            page_readings,
            page_num + 1,
            total_pages,
            low_threshold,
            high_threshold,
        );
    }

    // Save the PDF
    let file = File::create(path.as_ref())
        .map_err(|e| format!("Failed to create file: {}", e))?;
    let mut writer = BufWriter::new(file);
    doc.save(&mut writer)
        .map_err(|e| format!("Failed to save PDF: {}", e))?;

    Ok(())
}

fn draw_summary_page(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    font_bold: &IndirectFontRef,
    readings: &[StoredReading],
    time_in_range: Option<&TimeInRange>,
    low_threshold: u16,
    high_threshold: u16,
) {
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    draw_text(layer, font_bold, 24.0, MARGIN_MM, y, "Accu-Chek Glucose Report", COLOR_BLACK);
    y -= 10.0;

    // Date
    let date_str = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    draw_text(layer, font, 10.0, MARGIN_MM, y, &format!("Generated: {}", date_str), COLOR_GRAY);
    y -= 15.0;

    // Horizontal line
    draw_line(layer, MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y, COLOR_GRAY, 0.5);
    y -= 15.0;

    // Summary Statistics
    draw_text(layer, font_bold, 14.0, MARGIN_MM, y, "Summary Statistics", COLOR_BLACK);
    y -= 10.0;

    if !readings.is_empty() {
        let total = readings.len();
        let avg: f64 = readings.iter().map(|r| r.mg_dl as f64).sum::<f64>() / total as f64;
        let min = readings.iter().map(|r| r.mg_dl).min().unwrap_or(0);
        let max = readings.iter().map(|r| r.mg_dl).max().unwrap_or(0);

        // Get date range
        let first_date = readings.first().map(|r| r.timestamp.as_str()).unwrap_or("N/A");
        let last_date = readings.last().map(|r| r.timestamp.as_str()).unwrap_or("N/A");

        draw_text(layer, font, 11.0, MARGIN_MM + 5.0, y, &format!("Total Readings: {}", total), COLOR_BLACK);
        y -= 7.0;
        draw_text(layer, font, 11.0, MARGIN_MM + 5.0, y, &format!("Date Range: {} to {}", first_date, last_date), COLOR_BLACK);
        y -= 7.0;
        draw_text(layer, font, 11.0, MARGIN_MM + 5.0, y, &format!("Average: {:.1} mg/dL ({:.2} mmol/L)", avg, avg / 18.0), COLOR_BLACK);
        y -= 7.0;
        
        let min_color = get_reading_color(min, low_threshold, high_threshold);
        draw_text(layer, font, 11.0, MARGIN_MM + 5.0, y, &format!("Minimum: {} mg/dL", min), min_color);
        y -= 7.0;
        
        let max_color = get_reading_color(max, low_threshold, high_threshold);
        draw_text(layer, font, 11.0, MARGIN_MM + 5.0, y, &format!("Maximum: {} mg/dL", max), max_color);
        y -= 15.0;
    }

    // Time in Range section
    draw_text(layer, font_bold, 14.0, MARGIN_MM, y, &format!("Time in Range ({}-{} mg/dL)", low_threshold, high_threshold), COLOR_BLACK);
    y -= 12.0;

    if let Some(tir) = time_in_range {
        // Draw TIR bars - centered within margins
        let label_width = 45.0;
        let bar_width = 80.0;
        let bar_height = 12.0;
        let bar_x = MARGIN_MM + label_width;

        // Low
        draw_text(layer, font, 10.0, MARGIN_MM, y - 3.0, "Low:", COLOR_BLACK);
        draw_bar(layer, bar_x, y - 5.0, bar_width, bar_height, tir.low_percent as f32 / 100.0, COLOR_RED, COLOR_LIGHT_GRAY);
        draw_text(layer, font, 9.0, bar_x + bar_width + 3.0, y - 3.0, &format!("{:.1}% ({} readings)", tir.low_percent, tir.low), COLOR_BLACK);
        y -= 15.0;

        // In Range
        draw_text(layer, font, 10.0, MARGIN_MM, y - 3.0, "In Range:", COLOR_BLACK);
        draw_bar(layer, bar_x, y - 5.0, bar_width, bar_height, tir.normal_percent as f32 / 100.0, COLOR_GREEN, COLOR_LIGHT_GRAY);
        draw_text(layer, font, 9.0, bar_x + bar_width + 3.0, y - 3.0, &format!("{:.1}% ({} readings)", tir.normal_percent, tir.normal), COLOR_BLACK);
        y -= 15.0;

        // High
        draw_text(layer, font, 10.0, MARGIN_MM, y - 3.0, "High:", COLOR_BLACK);
        draw_bar(layer, bar_x, y - 5.0, bar_width, bar_height, tir.high_percent as f32 / 100.0, COLOR_ORANGE, COLOR_LIGHT_GRAY);
        draw_text(layer, font, 9.0, bar_x + bar_width + 3.0, y - 3.0, &format!("{:.1}% ({} readings)", tir.high_percent, tir.high), COLOR_BLACK);
        y -= 20.0;
    }

    // Distribution section
    draw_text(layer, font_bold, 14.0, MARGIN_MM, y, "Reading Distribution", COLOR_BLACK);
    y -= 12.0;

    if !readings.is_empty() {
        let very_low = readings.iter().filter(|r| r.mg_dl < 54).count();
        let low = readings.iter().filter(|r| r.mg_dl >= 54 && r.mg_dl < 70).count();
        let normal = readings.iter().filter(|r| r.mg_dl >= 70 && r.mg_dl <= 180).count();
        let high = readings.iter().filter(|r| r.mg_dl > 180 && r.mg_dl <= 250).count();
        let very_high = readings.iter().filter(|r| r.mg_dl > 250).count();
        let total = readings.len() as f32;

        let ranges = [
            ("< 54 (Very Low)", very_low, (0.8, 0.2, 0.2)),
            ("54-70 (Low)", low, COLOR_RED),
            ("70-180 (Target)", normal, COLOR_GREEN),
            ("180-250 (High)", high, COLOR_ORANGE),
            ("> 250 (Very High)", very_high, (0.9, 0.3, 0.2)),
        ];

        let label_width = 55.0;
        let bar_width = 70.0;
        let bar_x = MARGIN_MM + label_width;

        for (label, count, color) in ranges {
            let pct = if total > 0.0 { count as f32 / total } else { 0.0 };
            draw_text(layer, font, 9.0, MARGIN_MM, y - 2.0, label, COLOR_BLACK);
            draw_bar(layer, bar_x, y - 4.0, bar_width, 8.0, pct, color, COLOR_LIGHT_GRAY);
            draw_text(layer, font, 9.0, bar_x + bar_width + 3.0, y - 2.0, &format!("{} ({:.1}%)", count, pct * 100.0), COLOR_BLACK);
            y -= 10.0;
        }
    }

    // Footer
    draw_text(layer, font, 8.0, MARGIN_MM, MARGIN_MM, "Page 1 - Summary", COLOR_GRAY);
}

fn draw_chart_page(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    font_bold: &IndirectFontRef,
    readings: &[StoredReading],
    _daily_stats: &[DailyStats],
    low_threshold: u16,
    high_threshold: u16,
) {
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    draw_text(layer, font_bold, 16.0, MARGIN_MM, y, "Glucose Trend Chart", COLOR_BLACK);
    y -= 20.0;

    if readings.is_empty() {
        draw_text(layer, font, 12.0, MARGIN_MM, y, "No data to display", COLOR_GRAY);
        return;
    }

    // Chart area
    let chart_x = MARGIN_MM + 15.0;
    let chart_y = y - 120.0;
    let chart_width = PAGE_WIDTH_MM - 2.0 * MARGIN_MM - 20.0;
    let chart_height = 100.0;

    // Draw chart background
    draw_rect_fill(layer, chart_x, chart_y, chart_width, chart_height, COLOR_LIGHT_GRAY);

    // Draw chart border
    draw_rect_stroke(layer, chart_x, chart_y, chart_width, chart_height, COLOR_BLACK, 0.5);

    // Y-axis labels and grid
    let y_min: f32 = 40.0;
    let y_max: f32 = 300.0;
    let y_range = y_max - y_min;

    for mg_dl in [50, 100, 150, 200, 250, 300].iter() {
        let y_pos = chart_y + ((*mg_dl as f32 - y_min) / y_range) * chart_height;
        if y_pos >= chart_y && y_pos <= chart_y + chart_height {
            // Grid line
            draw_line(layer, chart_x, y_pos, chart_x + chart_width, y_pos, (0.8, 0.8, 0.8), 0.3);
            // Label
            draw_text(layer, font, 7.0, MARGIN_MM, y_pos - 1.5, &format!("{}", mg_dl), COLOR_GRAY);
        }
    }

    // Draw threshold lines
    let low_y = chart_y + ((low_threshold as f32 - y_min) / y_range) * chart_height;
    let high_y = chart_y + ((high_threshold as f32 - y_min) / y_range) * chart_height;
    
    draw_line(layer, chart_x, low_y, chart_x + chart_width, low_y, COLOR_RED, 0.8);
    draw_line(layer, chart_x, high_y, chart_x + chart_width, high_y, COLOR_ORANGE, 0.8);

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
            
            draw_line(layer, x1, y1_clamped, x2, y2_clamped, COLOR_BLUE, 0.8);
        }

        // Draw points
        for i in 0..n {
            let x = chart_x + i as f32 * x_step;
            let y_val = ((readings[i].mg_dl as f32 - y_min) / y_range) * chart_height;
            let y_pos = (chart_y + y_val).max(chart_y).min(chart_y + chart_height);
            let color = get_reading_color(readings[i].mg_dl, low_threshold, high_threshold);
            draw_point(layer, x, y_pos, 1.5, color);
        }
    }

    y = chart_y - 15.0;

    // Legend - spread across available width
    draw_text(layer, font_bold, 10.0, MARGIN_MM, y, "Legend:", COLOR_BLACK);
    y -= 8.0;
    
    // First row of legend
    draw_line(layer, MARGIN_MM, y + 2.0, MARGIN_MM + 12.0, y + 2.0, COLOR_BLUE, 1.0);
    draw_text(layer, font, 9.0, MARGIN_MM + 15.0, y, "Glucose readings", COLOR_BLACK);
    
    draw_line(layer, MARGIN_MM + 70.0, y + 2.0, MARGIN_MM + 82.0, y + 2.0, COLOR_RED, 1.0);
    draw_text(layer, font, 9.0, MARGIN_MM + 85.0, y, &format!("Low ({})", low_threshold), COLOR_BLACK);
    
    draw_line(layer, MARGIN_MM + 120.0, y + 2.0, MARGIN_MM + 132.0, y + 2.0, COLOR_ORANGE, 1.0);
    draw_text(layer, font, 9.0, MARGIN_MM + 135.0, y, &format!("High ({})", high_threshold), COLOR_BLACK);

    // Footer
    draw_text(layer, font, 8.0, MARGIN_MM, MARGIN_MM, "Page 2 - Chart", COLOR_GRAY);
}

fn draw_data_page(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    font_bold: &IndirectFontRef,
    readings: &[StoredReading],
    page_num: usize,
    total_pages: usize,
    low_threshold: u16,
    high_threshold: u16,
) {
    let mut y = PAGE_HEIGHT_MM - MARGIN_MM;

    // Title
    draw_text(layer, font_bold, 14.0, MARGIN_MM, y, "Glucose Readings", COLOR_BLACK);
    y -= 15.0;

    // Table header
    let col_x = [MARGIN_MM, MARGIN_MM + 35.0, MARGIN_MM + 55.0, MARGIN_MM + 75.0, MARGIN_MM + 95.0, MARGIN_MM + 135.0];

    // Header background
    draw_rect_fill(layer, MARGIN_MM, y - 6.0, PAGE_WIDTH_MM - 2.0 * MARGIN_MM, 8.0, COLOR_LIGHT_GRAY);

    draw_text(layer, font_bold, 8.0, col_x[0], y - 4.0, "Date/Time", COLOR_BLACK);
    draw_text(layer, font_bold, 8.0, col_x[1], y - 4.0, "mg/dL", COLOR_BLACK);
    draw_text(layer, font_bold, 8.0, col_x[2], y - 4.0, "mmol/L", COLOR_BLACK);
    draw_text(layer, font_bold, 8.0, col_x[3], y - 4.0, "Status", COLOR_BLACK);
    draw_text(layer, font_bold, 8.0, col_x[4], y - 4.0, "Notes", COLOR_BLACK);
    draw_text(layer, font_bold, 8.0, col_x[5], y - 4.0, "Tags", COLOR_BLACK);
    y -= 10.0;

    // Horizontal line
    draw_line(layer, MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y, COLOR_GRAY, 0.5);
    y -= 2.0;

    // Data rows
    for reading in readings {
        y -= 6.0;

        let status = if reading.mg_dl < low_threshold {
            ("LOW", COLOR_RED)
        } else if reading.mg_dl > high_threshold {
            ("HIGH", COLOR_ORANGE)
        } else {
            ("OK", COLOR_GREEN)
        };

        draw_text(layer, font, 7.0, col_x[0], y, &reading.timestamp, COLOR_BLACK);
        draw_text(layer, font, 7.0, col_x[1], y, &format!("{}", reading.mg_dl), get_reading_color(reading.mg_dl, low_threshold, high_threshold));
        draw_text(layer, font, 7.0, col_x[2], y, &format!("{:.2}", reading.mmol_l), COLOR_BLACK);
        draw_text(layer, font, 7.0, col_x[3], y, status.0, status.1);
        
        // Truncate notes if too long
        let note = reading.note.as_deref().unwrap_or("-");
        let note_display = if note.len() > 25 {
            format!("{}...", &note[..22])
        } else {
            note.to_string()
        };
        draw_text(layer, font, 7.0, col_x[4], y, &note_display, COLOR_BLACK);
        
        // Tags column
        let tags = reading.tags.as_deref().unwrap_or("-");
        let tags_display = if tags.len() > 20 {
            format!("{}...", &tags[..17])
        } else {
            tags.to_string()
        };
        draw_text(layer, font, 7.0, col_x[5], y, &tags_display, COLOR_GRAY);
    }

    // Footer
    draw_text(layer, font, 8.0, MARGIN_MM, MARGIN_MM, &format!("Page {} of {} - Data", page_num + 2, total_pages + 2), COLOR_GRAY);
}

// Helper functions

fn draw_text(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    size: f32,
    x: f32,
    y: f32,
    text: &str,
    color: (f32, f32, f32),
) {
    layer.set_fill_color(Color::Rgb(Rgb::new(color.0, color.1, color.2, None)));
    layer.use_text(text, size as f32, Mm(x), Mm(y), font);
}

fn draw_line(
    layer: &PdfLayerReference,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: (f32, f32, f32),
    width: f32,
) {
    layer.set_outline_color(Color::Rgb(Rgb::new(color.0, color.1, color.2, None)));
    layer.set_outline_thickness(width);
    
    let line = Line {
        points: vec![
            (Point::new(Mm(x1), Mm(y1)), false),
            (Point::new(Mm(x2), Mm(y2)), false),
        ],
        is_closed: false,
    };
    layer.add_line(line);
}

fn draw_rect_fill(
    layer: &PdfLayerReference,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: (f32, f32, f32),
) {
    layer.set_fill_color(Color::Rgb(Rgb::new(color.0, color.1, color.2, None)));
    
    let rect = Polygon {
        rings: vec![vec![
            (Point::new(Mm(x), Mm(y)), false),
            (Point::new(Mm(x + width), Mm(y)), false),
            (Point::new(Mm(x + width), Mm(y + height)), false),
            (Point::new(Mm(x), Mm(y + height)), false),
        ]],
        mode: PaintMode::Fill,
        winding_order: WindingOrder::NonZero,
    };
    layer.add_polygon(rect);
}

fn draw_rect_stroke(
    layer: &PdfLayerReference,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: (f32, f32, f32),
    stroke_width: f32,
) {
    layer.set_outline_color(Color::Rgb(Rgb::new(color.0, color.1, color.2, None)));
    layer.set_outline_thickness(stroke_width);
    
    let rect = Polygon {
        rings: vec![vec![
            (Point::new(Mm(x), Mm(y)), false),
            (Point::new(Mm(x + width), Mm(y)), false),
            (Point::new(Mm(x + width), Mm(y + height)), false),
            (Point::new(Mm(x), Mm(y + height)), false),
        ]],
        mode: PaintMode::Stroke,
        winding_order: WindingOrder::NonZero,
    };
    layer.add_polygon(rect);
}

fn draw_bar(
    layer: &PdfLayerReference,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    fill_pct: f32,
    fill_color: (f32, f32, f32),
    bg_color: (f32, f32, f32),
) {
    // Background
    draw_rect_fill(layer, x, y, width, height, bg_color);
    // Filled portion
    if fill_pct > 0.0 {
        draw_rect_fill(layer, x, y, width * fill_pct.min(1.0), height, fill_color);
    }
    // Border
    draw_rect_stroke(layer, x, y, width, height, COLOR_GRAY, 0.3);
}

fn draw_point(
    layer: &PdfLayerReference,
    x: f32,
    y: f32,
    radius: f32,
    color: (f32, f32, f32),
) {
    // Approximate a circle with a small filled rectangle
    draw_rect_fill(layer, x - radius, y - radius, radius * 2.0, radius * 2.0, color);
}

fn get_reading_color(mg_dl: u16, low_threshold: u16, high_threshold: u16) -> (f32, f32, f32) {
    if mg_dl < low_threshold {
        COLOR_RED
    } else if mg_dl > high_threshold {
        COLOR_ORANGE
    } else {
        COLOR_GREEN
    }
}
