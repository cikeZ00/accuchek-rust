//! GUI for Accu-Chek Data Manager using egui
//!
//! This module provides a modern, modular GUI organized into components:
//! - Settings management
//! - Dashboard view with summary statistics
//! - Readings list with search and editing
//! - Charts with multiple visualization types
//! - Sync management with device
//! - PDF export functionality

use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints, Bar, BarChart, BoxElem, BoxPlot, BoxSpread, Points};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::fs;
use std::io::Write;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::device::find_and_operate_accuchek;
use crate::storage::{Storage, StoredReading};
use crate::units::{GlucoseUnit, Thresholds, GlucoseRange};
use crate::stats::{BasicStats, TimeInRange, DailyStats, HourlyStats, TimeBinStats, HistogramBin, CalendarDay, ExportStatistics};
use crate::export::PdfExporter;

/// Type alias for reading list items (index, label, selected, note, tags)
type ReadingListItem = (usize, String, bool, Option<String>, Option<String>);

// ============= Settings =============

/// Persistent user settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub thresholds: Thresholds,
    pub glucose_unit: GlucoseUnit,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            thresholds: Thresholds::default(),
            glucose_unit: GlucoseUnit::MgDl,
        }
    }
}

impl AppSettings {
    pub fn load() -> Self {
        let path = crate::config::settings_file_path();
        if path.exists() {
            if let Ok(contents) = fs::read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str(&contents) {
                    return settings;
                }
            }
        }
        Self::default()
    }
    
    pub fn save(&self) {
        let path = crate::config::settings_file_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if let Ok(mut file) = fs::File::create(&path) {
                let _ = file.write_all(json.as_bytes());
            }
        }
    }
}

// ============= Sync Management =============

pub enum SyncMessage {
    Started,
    Success { new_count: usize, total_from_device: usize },
    Error(String),
}

#[derive(PartialEq, Clone, Copy)]
enum SyncStatus {
    Idle,
    Syncing,
    Success,
    Error,
}

// ============= Notifications =============

#[derive(Clone)]
struct Notification {
    message: String,
    notification_type: NotificationType,
    created_at: std::time::Instant,
}

#[derive(Clone, Copy, PartialEq)]
enum NotificationType {
    Success,
    Error,
}

impl Notification {
    fn new(message: String, notification_type: NotificationType) -> Self {
        Self { message, notification_type, created_at: std::time::Instant::now() }
    }
    
    fn age(&self) -> f32 {
        self.created_at.elapsed().as_secs_f32()
    }
    
    fn should_dismiss(&self) -> bool {
        self.age() > 3.0
    }
}

// ============= UI State =============

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Dashboard,
    Readings,
    Charts,
}

#[derive(PartialEq, Clone, Copy)]
enum ChartView {
    Overview,
    Histogram,
    TimeOfDay,
    DailyTrend,
    TimeBins,
    Calendar,
}

#[derive(PartialEq, Clone, Copy)]
enum ExportStatus {
    Idle,
    Success,
    Error,
}

// ============= Data Container =============

struct AppData {
    readings: Vec<StoredReading>,
    basic_stats: Option<BasicStats>,
    time_in_range: Option<TimeInRange>,
    daily_stats: Vec<DailyStats>,
    hourly_stats: Vec<HourlyStats>,
    time_bin_stats: Vec<TimeBinStats>,
    histogram_bins: Vec<HistogramBin>,
    calendar_data: Vec<CalendarDay>,
}

impl AppData {
    fn empty() -> Self {
        Self {
            readings: Vec::new(),
            basic_stats: None,
            time_in_range: None,
            daily_stats: Vec::new(),
            hourly_stats: Vec::new(),
            time_bin_stats: Vec::new(),
            histogram_bins: Vec::new(),
            calendar_data: Vec::new(),
        }
    }

    fn load(storage: &Storage, thresholds: Thresholds) -> Self {
        Self {
            readings: storage.get_all_readings().unwrap_or_default(),
            basic_stats: storage.get_basic_stats().ok().flatten(),
            time_in_range: storage.get_time_in_range(thresholds).ok(),
            daily_stats: storage.get_daily_stats(thresholds).unwrap_or_default(),
            hourly_stats: storage.get_hourly_stats().unwrap_or_default(),
            time_bin_stats: storage.get_time_bin_stats().unwrap_or_default(),
            histogram_bins: storage.get_histogram(20).unwrap_or_default(),
            calendar_data: storage.get_calendar_data(thresholds).unwrap_or_default(),
        }
    }
}

// ============= Main Application =============

pub struct AccuChekApp {
    db_path: String,
    data: AppData,
    settings: AppSettings,
    
    // UI state
    current_tab: Tab,
    selected_reading: Option<usize>,
    note_edit_buffer: String,
    tag_edit_buffer: String,
    search_query: String,
    current_chart_view: ChartView,
    show_settings: bool,
    
    // Sync state
    sync_receiver: Option<Receiver<SyncMessage>>,
    sync_status: SyncStatus,
    last_sync_message: String,
    
    // Export state
    export_status: ExportStatus,
    exported_path: Option<std::path::PathBuf>,
    show_export_dialog: bool,
    
    // Notifications
    notifications: Vec<Notification>,
}

impl Default for AccuChekApp {
    fn default() -> Self {
        let settings = AppSettings::load();
        Self {
            db_path: "accuchek.db".to_string(),
            data: AppData::empty(),
            settings,
            current_tab: Tab::Dashboard,
            selected_reading: None,
            note_edit_buffer: String::new(),
            tag_edit_buffer: String::new(),
            search_query: String::new(),
            current_chart_view: ChartView::Overview,
            show_settings: false,
            sync_receiver: None,
            sync_status: SyncStatus::Idle,
            last_sync_message: String::new(),
            export_status: ExportStatus::Idle,
            exported_path: None,
            show_export_dialog: false,
            notifications: Vec::new(),
        }
    }
}

impl AccuChekApp {
    pub fn new(cc: &eframe::CreationContext<'_>, db_path: String) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(egui::Color32::from_gray(220));
        cc.egui_ctx.set_visuals(visuals);
        
        let settings = AppSettings::load();
        let mut app = Self {
            db_path,
            settings,
            ..Default::default()
        };
        
        app.refresh_data();
        app
    }
    
    fn refresh_data(&mut self) {
        if let Ok(storage) = Storage::new(&self.db_path) {
            self.data = AppData::load(&storage, self.settings.thresholds);
        }
    }
    
    fn start_sync(&mut self) {
        if self.sync_status == SyncStatus::Syncing {
            return;
        }
        
        let (tx, rx): (Sender<SyncMessage>, Receiver<SyncMessage>) = channel();
        self.sync_receiver = Some(rx);
        self.sync_status = SyncStatus::Syncing;
        self.last_sync_message = "Connecting to device...".to_string();
        
        let db_path = self.db_path.clone();
        
        thread::spawn(move || {
            let _ = tx.send(SyncMessage::Started);
            
            let config = Config::load(crate::config::config_file_path())
                .or_else(|_| Config::load("config.txt"))
                .unwrap_or_default();
            
            match rusb::Context::new() {
                Ok(context) => {
                    match find_and_operate_accuchek(&context, &config, None) {
                        Ok(readings) => {
                            let total = readings.len();
                            match Storage::new(&db_path) {
                                Ok(storage) => {
                                    match storage.import_readings(&readings) {
                                        Ok(new_count) => {
                                            let _ = tx.send(SyncMessage::Success { new_count, total_from_device: total });
                                        }
                                        Err(e) => {
                                            let _ = tx.send(SyncMessage::Error(format!("Database error: {}", e)));
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(SyncMessage::Error(format!("Cannot open database: {}", e)));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(SyncMessage::Error(format!("{}", e)));
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(SyncMessage::Error(format!("USB error: {}", e)));
                }
            }
        });
    }
    
    fn check_sync_status(&mut self) {
        let messages: Vec<SyncMessage> = if let Some(ref rx) = self.sync_receiver {
            let mut msgs = Vec::new();
            while let Ok(msg) = rx.try_recv() {
                msgs.push(msg);
            }
            msgs
        } else {
            Vec::new()
        };
        
        let mut should_refresh = false;
        let mut clear_receiver = false;
        
        for msg in messages {
            match msg {
                SyncMessage::Started => {
                    self.last_sync_message = "Syncing...".to_string();
                }
                SyncMessage::Success { new_count, total_from_device } => {
                    self.sync_status = SyncStatus::Success;
                    self.last_sync_message.clear();
                    self.notifications.push(Notification::new(
                        format!("✓ Synced! {} new readings ({} from device)", new_count, total_from_device),
                        NotificationType::Success
                    ));
                    should_refresh = true;
                    clear_receiver = true;
                }
                SyncMessage::Error(e) => {
                    self.sync_status = SyncStatus::Error;
                    self.last_sync_message.clear();
                    self.notifications.push(Notification::new(
                        format!("✗ Sync Error: {}", e),
                        NotificationType::Error
                    ));
                    clear_receiver = true;
                }
            }
        }
        
        if clear_receiver {
            self.sync_receiver = None;
        }
        if should_refresh {
            self.refresh_data();
        }
    }
    
    fn export_pdf(&mut self) {
        use crate::config::default_export_dir;
        
        let default_name = format!("glucose_report_{}.pdf", chrono::Local::now().format("%Y%m%d"));
        
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_directory(default_export_dir())
            .set_file_name(&default_name)
            .save_file()
        {
            let export_stats = ExportStatistics::generate(&self.data.readings, self.settings.thresholds);
            let exporter = PdfExporter::new(
                &self.data.readings,
                &export_stats,
                self.settings.thresholds,
                self.settings.glucose_unit,
            );
            
            match exporter.export(&path) {
                Ok(()) => {
                    self.export_status = ExportStatus::Success;
                    self.notifications.push(Notification::new(
                        format!("✓ PDF exported to {}", path.file_name().unwrap_or_default().to_string_lossy()),
                        NotificationType::Success
                    ));
                    self.exported_path = Some(path);
                    self.show_export_dialog = true;
                }
                Err(e) => {
                    self.export_status = ExportStatus::Error;
                    self.notifications.push(Notification::new(
                        format!("✗ Export failed: {}", e),
                        NotificationType::Error
                    ));
                    self.exported_path = None;
                }
            }
        }
    }
    
    fn save_note(&mut self, id: i64, note: &str) {
        if let Ok(storage) = Storage::new(&self.db_path) {
            let _ = storage.update_note(id, note);
            self.refresh_data();
        }
    }
    
    fn save_tags(&mut self, id: i64, tags: &str) {
        if let Ok(storage) = Storage::new(&self.db_path) {
            let _ = storage.update_tags(id, tags);
            self.refresh_data();
        }
    }
    
    fn get_reading_color(&self, mg_dl: u16) -> egui::Color32 {
        match self.settings.thresholds.classify(mg_dl) {
            GlucoseRange::VeryLow => egui::Color32::from_rgb(200, 50, 50),
            GlucoseRange::Low => egui::Color32::from_rgb(255, 100, 100),
            GlucoseRange::InRange => egui::Color32::from_rgb(100, 255, 100),
            GlucoseRange::High => egui::Color32::from_rgb(255, 180, 100),
            GlucoseRange::VeryHigh => egui::Color32::from_rgb(255, 100, 50),
        }
    }
    
    fn filtered_readings(&self) -> Vec<&StoredReading> {
        if self.search_query.is_empty() {
            self.data.readings.iter().collect()
        } else {
            let query = self.search_query.to_lowercase();
            self.data.readings.iter().filter(|r| {
                r.timestamp.to_lowercase().contains(&query) ||
                r.note.as_ref().map(|n| n.to_lowercase().contains(&query)).unwrap_or(false) ||
                r.tags.as_ref().map(|t| t.to_lowercase().contains(&query)).unwrap_or(false) ||
                r.mg_dl.to_string().contains(&query)
            }).collect()
        }
    }
    
    fn add_tag(&mut self, tag: &str) {
        if self.tag_edit_buffer.is_empty() {
            self.tag_edit_buffer = tag.to_string();
        } else if !self.tag_edit_buffer.contains(tag) {
            self.tag_edit_buffer.push(',');
            self.tag_edit_buffer.push_str(tag);
        }
    }
}

impl eframe::App for AccuChekApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_sync_status();
        
        if self.sync_status == SyncStatus::Syncing {
            ctx.request_repaint();
        }
        
        // Top panel
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Accu-Chek Data Manager");
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("[Settings]").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                    
                    if ui.button("Export PDF").clicked() {
                        self.export_pdf();
                    }
                    
                    if ui.button("Refresh").clicked() {
                        self.refresh_data();
                    }
                    
                    let sync_enabled = self.sync_status != SyncStatus::Syncing;
                    if ui.add_enabled(sync_enabled, egui::Button::new("Sync Device")).clicked() {
                        self.start_sync();
                    }
                });
            });
            
            ui.separator();
            
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, Tab::Dashboard, "Dashboard");
                ui.selectable_value(&mut self.current_tab, Tab::Readings, "Readings");
                ui.selectable_value(&mut self.current_tab, Tab::Charts, "Charts");
            });
        });
        
        // Settings window
        if self.show_settings {
            self.show_settings_window(ctx);
        }
        
        // Export dialog
        if self.show_export_dialog {
            self.show_export_dialog_window(ctx);
        }
        
        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.current_tab {
                Tab::Dashboard => self.show_dashboard(ui),
                Tab::Readings => self.show_readings(ui),
                Tab::Charts => self.show_charts(ui),
            }
        });
        
        // Notifications
        self.render_notifications(ctx);
    }
}

// ============= UI Components =============

impl AccuChekApp {
    fn show_settings_window(&mut self, ctx: &egui::Context) {
        let mut save_settings = false;
        
        egui::Window::new("Settings")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Display Settings");
                ui.add_space(5.0);
                
                ui.horizontal(|ui| {
                    ui.label("Glucose unit:");
                    if ui.selectable_value(&mut self.settings.glucose_unit, GlucoseUnit::MgDl, "mg/dL").clicked() {
                        save_settings = true;
                    }
                    if ui.selectable_value(&mut self.settings.glucose_unit, GlucoseUnit::MmolL, "mmol/L").clicked() {
                        save_settings = true;
                    }
                });
                
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(5.0);
                
                ui.heading("Thresholds");
                ui.add_space(5.0);
                
                // Edit thresholds directly in the user's preferred unit
                match self.settings.glucose_unit {
                    GlucoseUnit::MgDl => {
                        let mut low = self.settings.thresholds.low_mgdl as f64;
                        let mut high = self.settings.thresholds.high_mgdl as f64;
                        
                        ui.horizontal(|ui| {
                            ui.label("Low threshold (mg/dL):");
                            if ui.add(egui::DragValue::new(&mut low).range(50.0..=100.0).speed(1.0)).changed() {
                                self.settings.thresholds.low_mgdl = low as u16;
                                save_settings = true;
                            }
                        });
                        
                        ui.horizontal(|ui| {
                            ui.label("High threshold (mg/dL):");
                            if ui.add(egui::DragValue::new(&mut high).range(140.0..=250.0).speed(1.0)).changed() {
                                self.settings.thresholds.high_mgdl = high as u16;
                                save_settings = true;
                            }
                        });
                    }
                    GlucoseUnit::MmolL => {
                        let mut low = self.settings.thresholds.low_mmol;
                        let mut high = self.settings.thresholds.high_mmol;
                        
                        ui.horizontal(|ui| {
                            ui.label("Low threshold (mmol/L):");
                            if ui.add(egui::DragValue::new(&mut low).range(2.8..=5.6).speed(0.1).max_decimals(1)).changed() {
                                self.settings.thresholds.low_mmol = low;
                                save_settings = true;
                            }
                        });
                        
                        ui.horizontal(|ui| {
                            ui.label("High threshold (mmol/L):");
                            if ui.add(egui::DragValue::new(&mut high).range(7.8..=13.9).speed(0.1).max_decimals(1)).changed() {
                                self.settings.thresholds.high_mmol = high;
                                save_settings = true;
                            }
                        });
                    }
                }
                
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(5.0);
                
                ui.heading("Data Locations");
                ui.add_space(5.0);
                
                egui::Grid::new("paths_grid")
                    .num_columns(2)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Database:");
                        ui.monospace(&self.db_path);
                        ui.end_row();
                        
                        ui.label("Data folder:");
                        ui.monospace(crate::config::get_data_dir().to_string_lossy().to_string());
                        ui.end_row();
                    });
                
                ui.add_space(10.0);
                
                ui.horizontal(|ui| {
                    if ui.button("Open Data Folder").clicked() {
                        open_folder(&crate::config::get_data_dir());
                    }
                    
                    if ui.button("Close").clicked() {
                        self.show_settings = false;
                    }
                });
            });
        
        if save_settings {
            self.settings.save();
            self.refresh_data();
        }
    }
    
    fn show_export_dialog_window(&mut self, ctx: &egui::Context) {
        if let Some(ref path) = self.exported_path.clone() {
            egui::Window::new("Export Successful")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("PDF exported successfully!");
                    ui.add_space(5.0);
                    ui.label(format!("{}", path.display()));
                    ui.add_space(15.0);
                    
                    ui.horizontal(|ui| {
                        if ui.button("Open File").clicked() {
                            open_file(path);
                            self.show_export_dialog = false;
                        }
                        
                        if ui.button("Open Folder").clicked() {
                            if let Some(parent) = path.parent() {
                                open_folder(parent);
                            }
                            self.show_export_dialog = false;
                        }
                        
                        if ui.button("Close").clicked() {
                            self.show_export_dialog = false;
                        }
                    });
                });
        }
    }
    
    fn show_dashboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("Dashboard");
        ui.separator();
        
        if self.data.readings.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.heading("No readings yet");
                ui.label("Connect your Accu-Chek device and click 'Sync Device' to download readings.");
                ui.add_space(20.0);
                if ui.button("Sync Now").clicked() {
                    self.start_sync();
                }
            });
            return;
        }
        
        ui.columns(2, |columns| {
            // Left column: Time in Range
            columns[0].group(|ui| {
                ui.heading("Time in Range");
                ui.label(format!("Target: {}", self.settings.thresholds.format_range(self.settings.glucose_unit)));
                ui.add_space(10.0);
                
                if let Some(ref tir) = self.data.time_in_range {
                    self.render_tir_bars(ui, tir);
                    ui.add_space(10.0);
                    ui.label(format!("Total readings: {}", tir.total));
                }
            });
            
            // Right column: Summary stats
            columns[1].group(|ui| {
                ui.heading("Summary");
                ui.add_space(10.0);
                
                if let Some(ref stats) = self.data.basic_stats {
                    ui.horizontal(|ui| {
                        ui.label("Average:");
                        ui.colored_label(
                            self.get_reading_color(stats.mgdl.mean as u16), 
                            self.settings.glucose_unit.format(stats.mgdl.mean as u16, stats.mmol.mean)
                        );
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Lowest:");
                        ui.colored_label(
                            self.get_reading_color(stats.mgdl.min), 
                            self.settings.glucose_unit.format(stats.mgdl.min, stats.mmol.min)
                        );
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Highest:");
                        ui.colored_label(
                            self.get_reading_color(stats.mgdl.max), 
                            self.settings.glucose_unit.format(stats.mgdl.max, stats.mmol.max)
                        );
                    });
                }
                
                if let Some(latest) = self.data.readings.last() {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.label("Latest reading:");
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            self.get_reading_color(latest.mg_dl),
                            self.settings.glucose_unit.format(latest.mg_dl, latest.mmol_l)
                        );
                        ui.label(format!("({})", latest.timestamp));
                    });
                }
            });
        });
        
        ui.add_space(20.0);
        
        // Recent readings
        ui.group(|ui| {
            ui.heading("Recent Readings");
            
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                egui::Grid::new("recent_readings_grid")
                    .num_columns(4)
                    .spacing([20.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Time").strong());
                        ui.label(egui::RichText::new(self.settings.glucose_unit.label()).strong());
                        ui.label(egui::RichText::new("Status").strong());
                        ui.label(egui::RichText::new("Note").strong());
                        ui.end_row();
                        
                        for reading in self.data.readings.iter().rev().take(10) {
                            ui.label(&reading.timestamp);
                            ui.colored_label(
                                self.get_reading_color(reading.mg_dl), 
                                self.settings.glucose_unit.format_value(reading.mg_dl, reading.mmol_l)
                            );
                            
                            let range = self.settings.thresholds.classify(reading.mg_dl);
                            ui.colored_label(self.get_reading_color(reading.mg_dl), range.status());
                            
                            ui.label(reading.note.as_deref().unwrap_or("-"));
                            ui.end_row();
                        }
                    });
            });
        });
    }
    
    fn render_tir_bars(&self, ui: &mut egui::Ui, tir: &TimeInRange) {
        let total = tir.total as f32;
        if total == 0.0 {
            return;
        }

        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "Low:");
            let bar = egui::ProgressBar::new(tir.low_percent() as f32 / 100.0)
                .text(format!("{:.1}% ({} readings)", tir.low_percent(), tir.total_low()))
                .fill(egui::Color32::from_rgb(255, 100, 100));
            ui.add(bar);
        });
        
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_rgb(100, 200, 100), "In Range:");
            let bar = egui::ProgressBar::new(tir.in_range_percent() as f32 / 100.0)
                .text(format!("{:.1}% ({} readings)", tir.in_range_percent(), tir.in_range))
                .fill(egui::Color32::from_rgb(100, 200, 100));
            ui.add(bar);
        });
        
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_rgb(255, 180, 100), "High:");
            let bar = egui::ProgressBar::new(tir.high_percent() as f32 / 100.0)
                .text(format!("{:.1}% ({} readings)", tir.high_percent(), tir.total_high()))
                .fill(egui::Color32::from_rgb(255, 180, 100));
            ui.add(bar);
        });
    }
    
    fn show_readings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("All Readings");
            ui.add_space(20.0);
            ui.label("Search:");
            ui.text_edit_singleline(&mut self.search_query);
            if ui.button("X").clicked() {
                self.search_query.clear();
            }
        });
        ui.separator();
        
        let filtered = self.filtered_readings();
        
        if filtered.is_empty() {
            ui.label("No readings match your search.");
            return;
        }
        
        ui.label(format!("Showing {} readings", filtered.len()));
        
        let list_items: Vec<ReadingListItem> = filtered
            .iter()
            .rev()
            .enumerate()
            .map(|(idx, r)| (
                idx,
                format!(
                    "{} | {} {}{}",
                    r.timestamp,
                    self.settings.glucose_unit.format_value(r.mg_dl, r.mmol_l),
                    if r.note.is_some() { "*" } else { "" },
                    if r.tags.is_some() { " #" } else { "" }
                ),
                self.selected_reading == Some(idx),
                r.note.clone(),
                r.tags.clone(),
            ))
            .collect();
        
        let selected_details: Option<(i64, String, u16, f64, String)> = 
            self.selected_reading.and_then(|idx| {
                filtered.iter().rev().nth(idx).map(|r| (
                    r.id,
                    r.timestamp.clone(),
                    r.mg_dl,
                    r.mmol_l,
                    r.imported_at.clone(),
                ))
            });
        
        ui.columns(2, |columns| {
            egui::ScrollArea::vertical()
                .id_salt("readings_list")
                .show(&mut columns[0], |ui| {
                    for (idx, label, is_selected, note, tags) in &list_items {
                        let response = ui.selectable_label(*is_selected, label);
                        
                        if response.clicked() {
                            self.selected_reading = Some(*idx);
                            self.note_edit_buffer = note.clone().unwrap_or_default();
                            self.tag_edit_buffer = tags.clone().unwrap_or_default();
                        }
                    }
                });
            
            columns[1].group(|ui| {
                if let Some((reading_id, timestamp, mg_dl, mmol_l, imported_at)) = selected_details {
                    ui.heading("Reading Details");
                    ui.separator();
                    
                    egui::Grid::new("reading_details")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Timestamp:");
                            ui.label(&timestamp);
                            ui.end_row();
                            
                            ui.label("Glucose:");
                            ui.colored_label(
                                self.get_reading_color(mg_dl),
                                self.settings.glucose_unit.format(mg_dl, mmol_l)
                            );
                            ui.end_row();
                            
                            ui.label("Status:");
                            let range = self.settings.thresholds.classify(mg_dl);
                            ui.colored_label(self.get_reading_color(mg_dl), range.label());
                            ui.end_row();
                            
                            ui.label("Imported:");
                            ui.label(&imported_at);
                            ui.end_row();
                        });
                    
                    ui.add_space(15.0);
                    ui.separator();
                    
                    ui.label("Note:");
                    ui.text_edit_multiline(&mut self.note_edit_buffer);
                    
                    if ui.button("Save Note").clicked() {
                        self.save_note(reading_id, &self.note_edit_buffer.clone());
                    }
                    
                    ui.add_space(10.0);
                    
                    ui.label("Tags (comma-separated):");
                    ui.text_edit_singleline(&mut self.tag_edit_buffer);
                    
                    ui.horizontal(|ui| {
                        if ui.button("Save Tags").clicked() {
                            self.save_tags(reading_id, &self.tag_edit_buffer.clone());
                        }
                        
                        if ui.small_button("+ fasting").clicked() {
                            self.add_tag("fasting");
                        }
                        if ui.small_button("+ before_meal").clicked() {
                            self.add_tag("before_meal");
                        }
                        if ui.small_button("+ after_meal").clicked() {
                            self.add_tag("after_meal");
                        }
                    });
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        ui.label("Select a reading to view details");
                    });
                }
            });
        });
    }
    
    fn show_charts(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Charts & Visualizations");
            ui.add_space(20.0);
            
            ui.label("View:");
            ui.selectable_value(&mut self.current_chart_view, ChartView::Overview, "Overview");
            ui.selectable_value(&mut self.current_chart_view, ChartView::Histogram, "Distribution");
            ui.selectable_value(&mut self.current_chart_view, ChartView::TimeOfDay, "Time of Day");
            ui.selectable_value(&mut self.current_chart_view, ChartView::DailyTrend, "Daily TIR");
            ui.selectable_value(&mut self.current_chart_view, ChartView::TimeBins, "Time Bins");
            ui.selectable_value(&mut self.current_chart_view, ChartView::Calendar, "Calendar");
        });
        ui.separator();
        
        if self.data.readings.is_empty() {
            ui.label("No data to display. Sync your device first.");
            return;
        }
        
        egui::ScrollArea::vertical().show(ui, |ui| {
            match self.current_chart_view {
                ChartView::Overview => self.show_overview_charts(ui),
                ChartView::Histogram => self.show_histogram_chart(ui),
                ChartView::TimeOfDay => self.show_time_of_day_chart(ui),
                ChartView::DailyTrend => self.show_daily_tir_trend(ui),
                ChartView::TimeBins => self.show_time_bins_boxplot(ui),
                ChartView::Calendar => self.show_calendar_view(ui),
            }
        });
    }
    
    fn show_overview_charts(&self, ui: &mut egui::Ui) {
        // Glucose trend chart
        ui.group(|ui| {
            ui.label(egui::RichText::new("Glucose Trend (All Readings)").heading());
            ui.label(format!("n = {} readings", self.data.readings.len()));
            
            let points: PlotPoints = self.data.readings.iter().enumerate()
                .map(|(i, r)| [i as f64, r.mg_dl as f64])
                .collect();
            
            let line = Line::new("Glucose", points)
                .color(egui::Color32::from_rgb(100, 150, 255));
            
            let low_line = Line::new(format!("Low ({})", self.settings.thresholds.low_display(self.settings.glucose_unit)), PlotPoints::from_iter(
                (0..self.data.readings.len()).map(|i| [i as f64, self.settings.thresholds.low_mgdl as f64])
            ))
            .color(egui::Color32::from_rgb(255, 100, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            let high_line = Line::new(format!("High ({})", self.settings.thresholds.high_display(self.settings.glucose_unit)), PlotPoints::from_iter(
                (0..self.data.readings.len()).map(|i| [i as f64, self.settings.thresholds.high_mgdl as f64])
            ))
            .color(egui::Color32::from_rgb(255, 180, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            Plot::new("glucose_trend")
                .height(250.0)
                .show_axes(true)
                .legend(egui_plot::Legend::default())
                .show(ui, |plot_ui| {
                    plot_ui.line(line);
                    plot_ui.line(low_line);
                    plot_ui.line(high_line);
                });
        });
        
        ui.add_space(20.0);
        
        // Daily averages
        if !self.data.daily_stats.is_empty() {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Daily Averages with Range").heading());
                ui.label(format!("n = {} days", self.data.daily_stats.len()));
                
                let avg_points: PlotPoints = self.data.daily_stats.iter().enumerate()
                    .map(|(i, d)| [i as f64, d.avg_mgdl])
                    .collect();
                
                let min_points: PlotPoints = self.data.daily_stats.iter().enumerate()
                    .map(|(i, d)| [i as f64, d.min_mgdl as f64])
                    .collect();
                
                let max_points: PlotPoints = self.data.daily_stats.iter().enumerate()
                    .map(|(i, d)| [i as f64, d.max_mgdl as f64])
                    .collect();
                
                let avg_line = Line::new("Average", avg_points)
                    .color(egui::Color32::from_rgb(100, 200, 100));
                
                let min_line = Line::new("Min", min_points)
                    .color(egui::Color32::from_rgb(100, 100, 255))
                    .style(egui_plot::LineStyle::dashed_loose());
                
                let max_line = Line::new("Max", max_points)
                    .color(egui::Color32::from_rgb(255, 100, 100))
                    .style(egui_plot::LineStyle::dashed_loose());
                
                Plot::new("daily_averages")
                    .height(200.0)
                    .show_axes(true)
                    .legend(egui_plot::Legend::default())
                    .show(ui, |plot_ui| {
                        plot_ui.line(avg_line);
                        plot_ui.line(min_line);
                        plot_ui.line(max_line);
                    });
            });
        }
    }
    
    fn show_histogram_chart(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Glucose Distribution Histogram").heading());
            ui.label(format!("n = {} readings, bin width = 20 mg/dL", self.data.readings.len()));
            
            if self.data.histogram_bins.is_empty() {
                ui.label("No histogram data available.");
                return;
            }
            
            let bars: Vec<Bar> = self.data.histogram_bins.iter()
                .map(|bin| {
                    let mid = (bin.range_start + bin.range_end) as f64 / 2.0;
                    let color = if bin.range_end <= self.settings.thresholds.low_mgdl {
                        egui::Color32::from_rgb(255, 100, 100)
                    } else if bin.range_start >= self.settings.thresholds.high_mgdl {
                        egui::Color32::from_rgb(255, 180, 100)
                    } else {
                        egui::Color32::from_rgb(100, 200, 100)
                    };
                    Bar::new(mid, bin.count as f64)
                        .width(18.0)
                        .fill(color)
                        .name(format!("{}-{}", bin.range_start, bin.range_end))
                })
                .collect();
            
            let chart = BarChart::new("histogram", bars);
            
            Plot::new("glucose_histogram")
                .height(300.0)
                .x_axis_label(format!("Glucose ({})", self.settings.glucose_unit.label()))
                .y_axis_label("Count")
                .show(ui, |plot_ui| {
                    plot_ui.bar_chart(chart);
                });
            
            // Statistics
            if let Some(ref stats) = self.data.basic_stats {
                ui.add_space(10.0);
                let (ci_low, ci_high) = stats.confidence_interval_95(self.settings.glucose_unit);
                ui.horizontal(|ui| {
                    ui.label(format!("Mean: {} (95% CI: {}-{})", 
                        stats.mean(self.settings.glucose_unit),
                        ci_low,
                        ci_high));
                    ui.separator();
                    ui.label(format!("Median: {}", stats.median(self.settings.glucose_unit)));
                    ui.separator();
                    ui.label(format!("SD: {:.1}", stats.std_dev(self.settings.glucose_unit)));
                });
            }
        });
    }
    
    fn show_time_of_day_chart(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Glucose by Hour of Day (Scatter + Boxplot)").heading());
            
            let total_readings: usize = self.data.hourly_stats.iter().map(|h| h.count()).sum();
            ui.label(format!("n = {} readings across 24 hours", total_readings));
            
            if self.data.hourly_stats.is_empty() {
                ui.label("No hourly data available.");
                return;
            }
            
            let mut all_points: Vec<[f64; 2]> = Vec::new();
            for stat in &self.data.hourly_stats {
                for &val in &stat.mgdl_readings {
                    let jitter = (val as f64 % 7.0 - 3.5) * 0.1;
                    all_points.push([stat.hour as f64 + jitter, val as f64]);
                }
            }
            
            let scatter = Points::new("Readings", PlotPoints::from_iter(all_points))
                .radius(2.0)
                .color(egui::Color32::from_rgba_unmultiplied(100, 150, 255, 100));
            
            let boxes: Vec<BoxElem> = self.data.hourly_stats.iter()
                .filter(|s| s.stats.is_some())
                .map(|stat| {
                    let s = stat.stats.as_ref().unwrap();
                    BoxElem::new(stat.hour as f64, BoxSpread::new(
                        s.mgdl.min as f64,
                        s.mgdl.q1 as f64,
                        s.mgdl.median as f64,
                        s.mgdl.q3 as f64,
                        s.mgdl.max as f64,
                    ))
                    .whisker_width(0.3)
                    .box_width(0.6)
                    .fill(egui::Color32::from_rgba_unmultiplied(100, 200, 100, 150))
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(50, 150, 50)))
                })
                .collect();
            
            let boxplot = BoxPlot::new("Hourly Distribution", boxes);
            
            let low_line = Line::new(format!("Low ({})", self.settings.thresholds.low_display(self.settings.glucose_unit)), PlotPoints::from_iter(
                (0..25).map(|h| [h as f64, self.settings.thresholds.low_mgdl as f64])
            ))
            .color(egui::Color32::from_rgb(255, 100, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            let high_line = Line::new(format!("High ({})", self.settings.thresholds.high_display(self.settings.glucose_unit)), PlotPoints::from_iter(
                (0..25).map(|h| [h as f64, self.settings.thresholds.high_mgdl as f64])
            ))
            .color(egui::Color32::from_rgb(255, 180, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            Plot::new("time_of_day_scatter")
                .height(350.0)
                .x_axis_label("Hour of Day")
                .y_axis_label(format!("Glucose ({})", self.settings.glucose_unit.label()))
                .legend(egui_plot::Legend::default())
                .show(ui, |plot_ui| {
                    plot_ui.points(scatter);
                    plot_ui.box_plot(boxplot);
                    plot_ui.line(low_line);
                    plot_ui.line(high_line);
                });
        });
    }
    
    fn show_daily_tir_trend(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Daily Time-in-Range Trend").heading());
            ui.label(format!("Target range: {} | n = {} days", 
                self.settings.thresholds.format_range(self.settings.glucose_unit),
                self.data.daily_stats.len()));
            
            if self.data.daily_stats.is_empty() {
                ui.label("No daily TIR data available.");
                return;
            }
            
            let tir_trend = Line::new("TIR %", PlotPoints::from_iter(
                self.data.daily_stats.iter().enumerate().map(|(i, d)| [i as f64, d.tir.in_range_percent()])
            ))
            .color(egui::Color32::from_rgb(50, 200, 50))
            .width(2.0);
            
            Plot::new("daily_tir_trend")
                .height(250.0)
                .x_axis_label("Day")
                .y_axis_label("Percentage")
                .legend(egui_plot::Legend::default())
                .show(ui, |plot_ui| {
                    plot_ui.line(tir_trend);
                    
                    let goal_line = Line::new("70% Goal", PlotPoints::from_iter(
                        (0..self.data.daily_stats.len() + 1).map(|i| [i as f64, 70.0])
                    ))
                    .color(egui::Color32::from_rgb(150, 150, 150))
                    .style(egui_plot::LineStyle::dashed_loose());
                    plot_ui.line(goal_line);
                });
            
            ui.add_space(10.0);
            
            let avg_tir: f64 = self.data.daily_stats.iter().map(|d| d.tir.in_range_percent()).sum::<f64>() 
                / self.data.daily_stats.len() as f64;
            let days_at_goal = self.data.daily_stats.iter().filter(|d| d.tir.in_range_percent() >= 70.0).count();
            
            ui.horizontal(|ui| {
                ui.label(format!("Average TIR: {:.1}%", avg_tir));
                ui.separator();
                ui.label(format!("Days at ≥70% goal: {}/{} ({:.1}%)", 
                    days_at_goal, self.data.daily_stats.len(), 
                    (days_at_goal as f64 / self.data.daily_stats.len() as f64) * 100.0));
            });
        });
    }
    
    fn show_time_bins_boxplot(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Glucose by Clinical Time Periods (Boxplots)").heading());
            ui.label("Shows glucose patterns across clinically meaningful time windows");
            
            if self.data.time_bin_stats.is_empty() {
                ui.label("No time bin data available.");
                return;
            }
            
            let boxes: Vec<BoxElem> = self.data.time_bin_stats.iter()
                .enumerate()
                .filter(|(_, s)| s.stats.is_some())
                .map(|(i, stat)| {
                    let s = stat.stats.as_ref().unwrap();
                    let color = if s.mgdl.mean < self.settings.thresholds.low_mgdl as f64 {
                        egui::Color32::from_rgb(255, 120, 120)
                    } else if s.mgdl.mean > self.settings.thresholds.high_mgdl as f64 {
                        egui::Color32::from_rgb(255, 200, 120)
                    } else {
                        egui::Color32::from_rgb(120, 200, 120)
                    };
                    
                    BoxElem::new(i as f64, BoxSpread::new(
                        s.mgdl.min as f64,
                        s.mgdl.q1 as f64,
                        s.mgdl.median as f64,
                        s.mgdl.q3 as f64,
                        s.mgdl.max as f64,
                    ))
                    .whisker_width(0.4)
                    .box_width(0.7)
                    .fill(color)
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 80, 80)))
                    .name(&stat.name)
                })
                .collect();
            
            let boxplot = BoxPlot::new("Time Bin Analysis", boxes);
            
            let low_line = Line::new(format!("Low ({})", self.settings.thresholds.low_display(self.settings.glucose_unit)), PlotPoints::from_iter(
                (-1..7).map(|x| [x as f64, self.settings.thresholds.low_mgdl as f64])
            ))
            .color(egui::Color32::from_rgb(255, 100, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            let high_line = Line::new(format!("High ({})", self.settings.thresholds.high_display(self.settings.glucose_unit)), PlotPoints::from_iter(
                (-1..7).map(|x| [x as f64, self.settings.thresholds.high_mgdl as f64])
            ))
            .color(egui::Color32::from_rgb(255, 180, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            Plot::new("time_bins_boxplot")
                .height(300.0)
                .y_axis_label(format!("Glucose ({})", self.settings.glucose_unit.label()))
                .legend(egui_plot::Legend::default())
                .show(ui, |plot_ui| {
                    plot_ui.box_plot(boxplot);
                    plot_ui.line(low_line);
                    plot_ui.line(high_line);
                });
        });
    }
    
    fn show_calendar_view(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Calendar View (Daily Small Multiples)").heading());
            ui.label(format!("Showing {} days with readings", self.data.calendar_data.len()));
            
            if self.data.calendar_data.is_empty() {
                ui.label("No calendar data available.");
                return;
            }
            
            use std::collections::BTreeMap;
            let mut weeks: BTreeMap<u32, Vec<&CalendarDay>> = BTreeMap::new();
            for day in &self.data.calendar_data {
                weeks.entry(day.week_of_year).or_default().push(day);
            }
            
            let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
            
            ui.horizontal(|ui| {
                ui.label("Week");
                for name in day_names {
                    ui.add_sized([80.0, 20.0], egui::Label::new(name));
                }
            });
            ui.separator();
            
            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                for (week, days) in weeks.iter().rev().take(12) {
                    ui.horizontal(|ui| {
                        ui.label(format!("W{}", week));
                        
                        for dow in 0..7 {
                            let day_data = days.iter().find(|d| d.day_of_week == dow);
                            
                            ui.allocate_ui(egui::Vec2::new(80.0, 60.0), |ui| {
                                if let Some(day) = day_data {
                                    let bg_color = if day.in_range_percent() >= 70.0 {
                                        egui::Color32::from_rgb(200, 255, 200)
                                    } else if day.in_range_percent() >= 50.0 {
                                        egui::Color32::from_rgb(255, 255, 200)
                                    } else {
                                        egui::Color32::from_rgb(255, 220, 200)
                                    };
                                    
                                    egui::Frame::new()
                                        .fill(bg_color)
                                        .inner_margin(4.0)
                                        .corner_radius(4.0)
                                        .show(ui, |ui| {
                                            ui.vertical(|ui| {
                                                ui.label(egui::RichText::new(&day.date[5..]).small());
                                                ui.label(format!("n={}", day.count()));
                                                ui.label(format!("{:.0}", day.mean(self.settings.glucose_unit)));
                                                ui.label(format!("{}%", day.in_range_percent() as i32));
                                            });
                                        });
                                } else {
                                    egui::Frame::new()
                                        .fill(egui::Color32::from_gray(40))
                                        .inner_margin(4.0)
                                        .corner_radius(4.0)
                                        .show(ui, |ui| {
                                            ui.label("-");
                                        });
                                }
                            });
                        }
                    });
                }
            });
            
            ui.add_space(10.0);
            
            ui.horizontal(|ui| {
                ui.label("Legend:");
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(200, 255, 200))
                    .inner_margin(4.0)
                    .show(ui, |ui| { ui.label("≥70% TIR"); });
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(255, 255, 200))
                    .inner_margin(4.0)
                    .show(ui, |ui| { ui.label("50-70% TIR"); });
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(255, 220, 200))
                    .inner_margin(4.0)
                    .show(ui, |ui| { ui.label("<50% TIR"); });
            });
        });
    }
    
    fn render_notifications(&mut self, ctx: &egui::Context) {
        let mut dismiss_indices = Vec::new();
        let available_rect = ctx.available_rect();
        let notification_width = 350.0;
        let notification_spacing = 10.0;
        let right_margin = 20.0;
        let top_margin = 60.0;
        
        for (idx, notification) in self.notifications.iter().enumerate() {
            let y_pos = top_margin + (idx as f32) * (70.0 + notification_spacing);
            let x_pos = available_rect.width() - notification_width - right_margin;
            
            let age = notification.age();
            let alpha = if age > 2.5 {
                ((3.0 - age) / 0.5).max(0.0)
            } else {
                1.0
            };
            
            if notification.should_dismiss() {
                dismiss_indices.push(idx);
                continue;
            }
            
            let (bg_color, text_color) = match notification.notification_type {
                NotificationType::Success => (
                    egui::Color32::from_rgba_unmultiplied(20, 100, 20, (200.0 * alpha) as u8),
                    egui::Color32::from_rgba_unmultiplied(150, 255, 150, (255.0 * alpha) as u8)
                ),
                NotificationType::Error => (
                    egui::Color32::from_rgba_unmultiplied(100, 20, 20, (200.0 * alpha) as u8),
                    egui::Color32::from_rgba_unmultiplied(255, 150, 150, (255.0 * alpha) as u8)
                ),
            };
            
            egui::Area::new(egui::Id::new(format!("notification_{}", idx)))
                .fixed_pos(egui::pos2(x_pos, y_pos))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(bg_color)
                        .corner_radius(8.0)
                        .inner_margin(15.0)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (50.0 * alpha) as u8)))
                        .show(ui, |ui| {
                            ui.set_width(notification_width - 30.0);
                            ui.colored_label(text_color, &notification.message);
                        });
                });
        }
        
        for &idx in dismiss_indices.iter().rev() {
            self.notifications.remove(idx);
        }
        
        if !self.notifications.is_empty() {
            ctx.request_repaint();
        }
    }
}

// ============= Helper Functions =============

fn open_folder(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
}

fn open_file(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
}

/// Run the GUI application
pub fn run_gui(db_path: String) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_min_inner_size([800.0, 600.0])
            .with_transparent(false),
        vsync: true,
        multisampling: 0,
        depth_buffer: 0,
        ..Default::default()
    };
    
    eframe::run_native(
        "Accu-Chek Data Manager",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::default());
            Ok(Box::new(AccuChekApp::new(cc, db_path)))
        }),
    )
}
