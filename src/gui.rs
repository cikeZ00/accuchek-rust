//! GUI for Accu-Chek Data Manager using egui

use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::fs;
use std::io::Write;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::device::find_and_operate_accuchek;
use crate::storage::{Storage, StoredReading, TimeInRange, DailyStats, HourlyStats, TimeBinStats, DailyTIR, CalendarDay, HistogramBin};
use crate::export::export_to_pdf;

/// Persistent user settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub low_threshold: u16,
    pub high_threshold: u16,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            low_threshold: 70,
            high_threshold: 180,
        }
    }
}

impl AppSettings {
    /// Load settings from the settings file
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
    
    /// Save settings to the settings file
    pub fn save(&self) {
        let path = crate::config::settings_file_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if let Ok(mut file) = fs::File::create(&path) {
                let _ = file.write_all(json.as_bytes());
            }
        }
    }
}

/// Message from sync thread to UI
pub enum SyncMessage {
    Started,
    Success { new_count: usize, total_from_device: usize },
    Error(String),
}

/// Main application state
pub struct AccuChekApp {
    // Database
    db_path: String,
    
    // Data
    readings: Vec<StoredReading>,
    time_in_range: Option<TimeInRange>,
    daily_stats: Vec<DailyStats>,
    
    // Visualization data
    hourly_stats: Vec<HourlyStats>,
    time_bin_stats: Vec<TimeBinStats>,
    daily_tir: Vec<DailyTIR>,
    calendar_data: Vec<CalendarDay>,
    histogram_bins: Vec<HistogramBin>,
    
    // UI state
    current_tab: Tab,
    selected_reading: Option<usize>,
    note_edit_buffer: String,
    tag_edit_buffer: String,
    search_query: String,
    current_chart_view: ChartView,
    
    // Sync state
    sync_receiver: Option<Receiver<SyncMessage>>,
    sync_status: SyncStatus,
    last_sync_message: String,
    
    // Settings
    low_threshold: u16,
    high_threshold: u16,
    show_settings: bool,
    
    // Export state
    export_message: String,
    export_status: ExportStatus,
    exported_path: Option<std::path::PathBuf>,
    show_export_dialog: bool,
}

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
enum SyncStatus {
    Idle,
    Syncing,
    Success,
    Error,
}

#[derive(PartialEq, Clone, Copy)]
enum ExportStatus {
    Idle,
    Success,
    Error,
}

impl Default for AccuChekApp {
    fn default() -> Self {
        let settings = AppSettings::load();
        Self {
            db_path: "accuchek.db".to_string(),
            readings: Vec::new(),
            time_in_range: None,
            daily_stats: Vec::new(),
            hourly_stats: Vec::new(),
            time_bin_stats: Vec::new(),
            daily_tir: Vec::new(),
            calendar_data: Vec::new(),
            histogram_bins: Vec::new(),
            current_tab: Tab::Dashboard,
            selected_reading: None,
            note_edit_buffer: String::new(),
            tag_edit_buffer: String::new(),
            search_query: String::new(),
            current_chart_view: ChartView::Overview,
            sync_receiver: None,
            sync_status: SyncStatus::Idle,
            last_sync_message: String::new(),
            low_threshold: settings.low_threshold,
            high_threshold: settings.high_threshold,
            show_settings: false,
            export_message: String::new(),
            export_status: ExportStatus::Idle,
            exported_path: None,
            show_export_dialog: false,
        }
    }
}

impl AccuChekApp {
    pub fn new(cc: &eframe::CreationContext<'_>, db_path: String) -> Self {
        // Set up custom fonts/visuals if needed
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(egui::Color32::from_gray(220));
        cc.egui_ctx.set_visuals(visuals);
        
        // Load settings
        let settings = AppSettings::load();
        
        let mut app = Self {
            db_path,
            low_threshold: settings.low_threshold,
            high_threshold: settings.high_threshold,
            ..Default::default()
        };
        
        app.refresh_data();
        app
    }
    
    fn refresh_data(&mut self) {
        if let Ok(storage) = Storage::new(&self.db_path) {
            self.readings = storage.get_all_readings().unwrap_or_default();
            self.time_in_range = storage.get_time_in_range().ok();
            self.daily_stats = storage.get_daily_averages().unwrap_or_default();
            
            // Load visualization data
            self.hourly_stats = storage.get_hourly_stats().unwrap_or_default();
            self.time_bin_stats = storage.get_time_bin_stats(self.low_threshold, self.high_threshold).unwrap_or_default();
            self.daily_tir = storage.get_daily_tir(self.low_threshold, self.high_threshold).unwrap_or_default();
            self.calendar_data = storage.get_calendar_data(self.low_threshold, self.high_threshold).unwrap_or_default();
            self.histogram_bins = storage.get_histogram(20, self.low_threshold, self.high_threshold).unwrap_or_default();
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
            
            // Load config from OS data directory first, then fallback to current directory
            let config = Config::load(crate::config::config_file_path())
                .or_else(|_| Config::load("config.txt"))
                .unwrap_or_default();
            
            // Try to sync
            match rusb::Context::new() {
                Ok(context) => {
                    match find_and_operate_accuchek(&context, &config, None) {
                        Ok(readings) => {
                            let total = readings.len();
                            match Storage::new(&db_path) {
                                Ok(storage) => {
                                    match storage.import_readings(&readings) {
                                        Ok(new_count) => {
                                            let _ = tx.send(SyncMessage::Success { 
                                                new_count, 
                                                total_from_device: total 
                                            });
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
        // Collect messages first to avoid borrow issues
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
                    self.last_sync_message = format!(
                        "✓ Synced! {} new readings ({} from device)", 
                        new_count, total_from_device
                    );
                    should_refresh = true;
                    clear_receiver = true;
                }
                SyncMessage::Error(e) => {
                    self.sync_status = SyncStatus::Error;
                    self.last_sync_message = format!("✗ Error: {}", e);
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
        
        // Default filename with date
        let default_name = format!("glucose_report_{}.pdf", 
            chrono::Local::now().format("%Y%m%d"));
        
        // Use rfd to get save location with OS-appropriate default directory
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_directory(default_export_dir())
            .set_file_name(&default_name)
            .save_file()
        {
            match export_to_pdf(
                &path,
                &self.readings,
                self.time_in_range.as_ref(),
                &self.daily_stats,
                self.low_threshold,
                self.high_threshold,
                &self.hourly_stats,
                &self.time_bin_stats,
                &self.daily_tir,
                &self.histogram_bins,
            ) {
                Ok(()) => {
                    self.export_status = ExportStatus::Success;
                    self.export_message = format!("Exported to {}", path.display());
                    self.exported_path = Some(path);
                    self.show_export_dialog = true;
                }
                Err(e) => {
                    self.export_status = ExportStatus::Error;
                    self.export_message = format!("Export failed: {}", e);
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
        if mg_dl < self.low_threshold {
            egui::Color32::from_rgb(255, 100, 100) // Red for low
        } else if mg_dl > self.high_threshold {
            egui::Color32::from_rgb(255, 180, 100) // Orange for high
        } else {
            egui::Color32::from_rgb(100, 255, 100) // Green for in-range
        }
    }
    
    fn filtered_readings(&self) -> Vec<&StoredReading> {
        if self.search_query.is_empty() {
            self.readings.iter().collect()
        } else {
            let query = self.search_query.to_lowercase();
            self.readings.iter().filter(|r| {
                r.timestamp.to_lowercase().contains(&query) ||
                r.note.as_ref().map(|n| n.to_lowercase().contains(&query)).unwrap_or(false) ||
                r.tags.as_ref().map(|t| t.to_lowercase().contains(&query)).unwrap_or(false) ||
                r.mg_dl.to_string().contains(&query)
            }).collect()
        }
    }
}

impl eframe::App for AccuChekApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for sync updates
        self.check_sync_status();
        
        // Request repaint while syncing
        if self.sync_status == SyncStatus::Syncing {
            ctx.request_repaint();
        }
        
        // Top panel with title and sync button
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Accu-Chek Data Manager");
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Settings button
                    if ui.button("[Settings]").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                    
                    // Export PDF button
                    if ui.button("Export PDF").clicked() {
                        self.export_pdf();
                    }
                    
                    // Refresh button
                    if ui.button("Refresh").clicked() {
                        self.refresh_data();
                    }
                    
                    // Sync button
                    let sync_enabled = self.sync_status != SyncStatus::Syncing;
                    if ui.add_enabled(sync_enabled, egui::Button::new("Sync Device")).clicked() {
                        self.start_sync();
                    }
                    
                    // Status message
                    if !self.last_sync_message.is_empty() {
                        let color = match self.sync_status {
                            SyncStatus::Success => egui::Color32::from_rgb(100, 255, 100),
                            SyncStatus::Error => egui::Color32::from_rgb(255, 100, 100),
                            _ => egui::Color32::GRAY,
                        };
                        ui.colored_label(color, &self.last_sync_message);
                    }
                    
                    // Export status message
                    if !self.export_message.is_empty() {
                        let color = match self.export_status {
                            ExportStatus::Success => egui::Color32::from_rgb(100, 255, 100),
                            ExportStatus::Error => egui::Color32::from_rgb(255, 100, 100),
                            _ => egui::Color32::GRAY,
                        };
                        ui.colored_label(color, &self.export_message);
                    }
                });
            });
            
            ui.separator();
            
            // Tab bar
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, Tab::Dashboard, "Dashboard");
                ui.selectable_value(&mut self.current_tab, Tab::Readings, "Readings");
                ui.selectable_value(&mut self.current_tab, Tab::Charts, "Charts");
            });
        });
        
        // Settings window
        if self.show_settings {
            let mut save_settings = false;
            
            egui::Window::new("Settings")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.heading("Thresholds");
                    ui.add_space(5.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Low threshold (mg/dL):");
                        if ui.add(egui::DragValue::new(&mut self.low_threshold).range(50..=100)).changed() {
                            save_settings = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("High threshold (mg/dL):");
                        if ui.add(egui::DragValue::new(&mut self.high_threshold).range(140..=250)).changed() {
                            save_settings = true;
                        }
                    });
                    
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
                            let data_dir = crate::config::get_data_dir();
                            #[cfg(target_os = "windows")]
                            {
                                let _ = std::process::Command::new("explorer")
                                    .arg(&data_dir)
                                    .spawn();
                            }
                            #[cfg(target_os = "linux")]
                            {
                                let _ = std::process::Command::new("xdg-open")
                                    .arg(&data_dir)
                                    .spawn();
                            }
                            #[cfg(target_os = "macos")]
                            {
                                let _ = std::process::Command::new("open")
                                    .arg(&data_dir)
                                    .spawn();
                            }
                        }
                        
                        if ui.button("Close").clicked() {
                            self.show_settings = false;
                        }
                    });
                });
            
            // Save settings if changed
            if save_settings {
                let settings = AppSettings {
                    low_threshold: self.low_threshold,
                    high_threshold: self.high_threshold,
                };
                settings.save();
            }
        }
        
        // Export success dialog
        if self.show_export_dialog {
            if let Some(ref path) = self.exported_path {
                let path_clone = path.clone();
                egui::Window::new("Export Successful")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.label("PDF exported successfully!");
                        ui.add_space(5.0);
                        ui.label(format!("{}", path_clone.display()));
                        ui.add_space(15.0);
                        
                        ui.horizontal(|ui| {
                            if ui.button("Open File").clicked() {
                                #[cfg(target_os = "windows")]
                                {
                                    let _ = std::process::Command::new("cmd")
                                        .args(["/C", "start", "", &path_clone.to_string_lossy()])
                                        .spawn();
                                }
                                #[cfg(target_os = "linux")]
                                {
                                    let _ = std::process::Command::new("xdg-open")
                                        .arg(&path_clone)
                                        .spawn();
                                }
                                #[cfg(target_os = "macos")]
                                {
                                    let _ = std::process::Command::new("open")
                                        .arg(&path_clone)
                                        .spawn();
                                }
                                self.show_export_dialog = false;
                            }
                            
                            if ui.button("Open Folder").clicked() {
                                if let Some(parent) = path_clone.parent() {
                                    #[cfg(target_os = "windows")]
                                    {
                                        let _ = std::process::Command::new("explorer")
                                            .arg(parent)
                                            .spawn();
                                    }
                                    #[cfg(target_os = "linux")]
                                    {
                                        let _ = std::process::Command::new("xdg-open")
                                            .arg(parent)
                                            .spawn();
                                    }
                                    #[cfg(target_os = "macos")]
                                    {
                                        let _ = std::process::Command::new("open")
                                            .arg(parent)
                                            .spawn();
                                    }
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
        
        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.current_tab {
                Tab::Dashboard => self.show_dashboard(ui),
                Tab::Readings => self.show_readings(ui),
                Tab::Charts => self.show_charts(ui),
            }
        });
    }
}

impl AccuChekApp {
    fn show_dashboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("Dashboard");
        ui.separator();
        
        if self.readings.is_empty() {
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
                ui.label(format!("Target: {}-{} mg/dL", self.low_threshold, self.high_threshold));
                ui.add_space(10.0);
                
                if let Some(ref tir) = self.time_in_range {
                    // Progress bars for each range
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "Low:");
                        let bar = egui::ProgressBar::new(tir.low_percent as f32 / 100.0)
                            .text(format!("{:.1}% ({} readings)", tir.low_percent, tir.low))
                            .fill(egui::Color32::from_rgb(255, 100, 100));
                        ui.add(bar);
                    });
                    
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(100, 200, 100), "In Range:");
                        let bar = egui::ProgressBar::new(tir.normal_percent as f32 / 100.0)
                            .text(format!("{:.1}% ({} readings)", tir.normal_percent, tir.normal))
                            .fill(egui::Color32::from_rgb(100, 200, 100));
                        ui.add(bar);
                    });
                    
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(255, 180, 100), "High:");
                        let bar = egui::ProgressBar::new(tir.high_percent as f32 / 100.0)
                            .text(format!("{:.1}% ({} readings)", tir.high_percent, tir.high))
                            .fill(egui::Color32::from_rgb(255, 180, 100));
                        ui.add(bar);
                    });
                    
                    ui.add_space(10.0);
                    ui.label(format!("Total readings: {}", tir.total));
                }
            });
            
            // Right column: Summary stats
            columns[1].group(|ui| {
                ui.heading("Summary");
                ui.add_space(10.0);
                
                // Calculate stats
                if !self.readings.is_empty() {
                    let avg: f64 = self.readings.iter().map(|r| r.mg_dl as f64).sum::<f64>() / self.readings.len() as f64;
                    let min = self.readings.iter().map(|r| r.mg_dl).min().unwrap_or(0);
                    let max = self.readings.iter().map(|r| r.mg_dl).max().unwrap_or(0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Average:");
                        ui.colored_label(self.get_reading_color(avg as u16), format!("{:.0} mg/dL", avg));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Lowest:");
                        ui.colored_label(self.get_reading_color(min), format!("{} mg/dL", min));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Highest:");
                        ui.colored_label(self.get_reading_color(max), format!("{} mg/dL", max));
                    });
                    
                    // Most recent reading
                    if let Some(latest) = self.readings.last() {
                        ui.add_space(10.0);
                        ui.separator();
                        ui.label("Latest reading:");
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                self.get_reading_color(latest.mg_dl),
                                format!("{} mg/dL", latest.mg_dl)
                            );
                            ui.label(format!("({})", latest.timestamp));
                        });
                    }
                }
            });
        });
        
        ui.add_space(20.0);
        
        // Recent readings mini-table
        ui.group(|ui| {
            ui.heading("Recent Readings");
            
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                egui::Grid::new("recent_readings_grid")
                    .num_columns(5)
                    .spacing([20.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Time").strong());
                        ui.label(egui::RichText::new("mg/dL").strong());
                        ui.label(egui::RichText::new("mmol/L").strong());
                        ui.label(egui::RichText::new("Status").strong());
                        ui.label(egui::RichText::new("Note").strong());
                        ui.end_row();
                        
                        for reading in self.readings.iter().rev().take(10) {
                            ui.label(&reading.timestamp);
                            ui.colored_label(self.get_reading_color(reading.mg_dl), format!("{}", reading.mg_dl));
                            ui.label(format!("{:.2}", reading.mmol_l));
                            
                            let (status_text, status_color) = if reading.mg_dl < self.low_threshold {
                                ("LOW", egui::Color32::from_rgb(255, 100, 100))
                            } else if reading.mg_dl > self.high_threshold {
                                ("HIGH", egui::Color32::from_rgb(255, 180, 100))
                            } else {
                                ("OK", egui::Color32::from_rgb(100, 255, 100))
                            };
                            ui.colored_label(status_color, status_text);
                            
                            ui.label(reading.note.as_deref().unwrap_or("-"));
                            ui.end_row();
                        }
                    });
            });
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
        
        // Prepare data for display to avoid borrow issues
        let list_items: Vec<(usize, String, bool, Option<String>, Option<String>)> = filtered
            .iter()
            .rev()
            .enumerate()
            .map(|(idx, r)| (
                idx,
                format!(
                    "{} | {} mg/dL {}{}",
                    r.timestamp,
                    r.mg_dl,
                    if r.note.is_some() { "*" } else { "" },
                    if r.tags.is_some() { " #" } else { "" }
                ),
                self.selected_reading == Some(idx),
                r.note.clone(),
                r.tags.clone(),
            ))
            .collect();
        
        // Get selected reading details
        let selected_details: Option<(i64, String, u16, f64, String, u16, u16)> = 
            self.selected_reading.and_then(|idx| {
                filtered.iter().rev().nth(idx).map(|r| (
                    r.id,
                    r.timestamp.clone(),
                    r.mg_dl,
                    r.mmol_l,
                    r.imported_at.clone(),
                    self.low_threshold,
                    self.high_threshold,
                ))
            });
        
        // Split view: list on left, details on right
        ui.columns(2, |columns| {
            // Left: Scrollable list
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
            
            // Right: Details panel
            columns[1].group(|ui| {
                if let Some((reading_id, timestamp, mg_dl, mmol_l, imported_at, low_thresh, high_thresh)) = selected_details.clone() {
                    ui.heading("Reading Details");
                    ui.separator();
                    
                    let color = if mg_dl < low_thresh {
                        egui::Color32::from_rgb(255, 100, 100)
                    } else if mg_dl > high_thresh {
                        egui::Color32::from_rgb(255, 180, 100)
                    } else {
                        egui::Color32::from_rgb(100, 255, 100)
                    };
                    
                    egui::Grid::new("reading_details")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Timestamp:");
                            ui.label(&timestamp);
                            ui.end_row();
                            
                            ui.label("Glucose:");
                            ui.colored_label(
                                color,
                                format!("{} mg/dL ({:.2} mmol/L)", mg_dl, mmol_l)
                            );
                            ui.end_row();
                            
                            ui.label("Status:");
                            let status = if mg_dl < low_thresh {
                                ("LOW", egui::Color32::from_rgb(255, 100, 100))
                            } else if mg_dl > high_thresh {
                                ("HIGH", egui::Color32::from_rgb(255, 180, 100))
                            } else {
                                ("In Range", egui::Color32::from_rgb(100, 255, 100))
                            };
                            ui.colored_label(status.1, status.0);
                            ui.end_row();
                            
                            ui.label("Imported:");
                            ui.label(&imported_at);
                            ui.end_row();
                        });
                    
                    ui.add_space(15.0);
                    ui.separator();
                    
                    // Note editing
                    ui.label("Note:");
                    ui.text_edit_multiline(&mut self.note_edit_buffer);
                    
                    ui.horizontal(|ui| {
                        if ui.button("Save Note").clicked() {
                            self.save_note(reading_id, &self.note_edit_buffer.clone());
                        }
                    });
                    
                    ui.add_space(10.0);
                    
                    // Tags editing
                    ui.label("Tags (comma-separated):");
                    ui.text_edit_singleline(&mut self.tag_edit_buffer);
                    
                    ui.horizontal(|ui| {
                        if ui.button("Save Tags").clicked() {
                            self.save_tags(reading_id, &self.tag_edit_buffer.clone());
                        }
                        
                        // Quick tag buttons
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
    
    fn add_tag(&mut self, tag: &str) {
        if self.tag_edit_buffer.is_empty() {
            self.tag_edit_buffer = tag.to_string();
        } else if !self.tag_edit_buffer.contains(tag) {
            self.tag_edit_buffer.push_str(",");
            self.tag_edit_buffer.push_str(tag);
        }
    }
    
    fn show_charts(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Charts & Visualizations");
            ui.add_space(20.0);
            
            // Chart view selector
            ui.label("View:");
            ui.selectable_value(&mut self.current_chart_view, ChartView::Overview, "Overview");
            ui.selectable_value(&mut self.current_chart_view, ChartView::Histogram, "Distribution");
            ui.selectable_value(&mut self.current_chart_view, ChartView::TimeOfDay, "Time of Day");
            ui.selectable_value(&mut self.current_chart_view, ChartView::DailyTrend, "Daily TIR Trend");
            ui.selectable_value(&mut self.current_chart_view, ChartView::TimeBins, "Time Bins");
            ui.selectable_value(&mut self.current_chart_view, ChartView::Calendar, "Calendar");
        });
        ui.separator();
        
        if self.readings.is_empty() {
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
    
    fn show_overview_charts(&mut self, ui: &mut egui::Ui) {
        // Glucose trend chart
        ui.group(|ui| {
            ui.label(egui::RichText::new("Glucose Trend (All Readings)").heading());
            ui.label(format!("n = {} readings", self.readings.len()));
            
            let points: PlotPoints = self.readings.iter().enumerate()
                .map(|(i, r)| [i as f64, r.mg_dl as f64])
                .collect();
            
            let line = Line::new("Glucose", points)
                .color(egui::Color32::from_rgb(100, 150, 255));
            
            // Reference lines for thresholds
            let low_line = Line::new(format!("Low ({})", self.low_threshold), PlotPoints::from_iter(
                (0..self.readings.len()).map(|i| [i as f64, self.low_threshold as f64])
            ))
            .color(egui::Color32::from_rgb(255, 100, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            let high_line = Line::new(format!("High ({})", self.high_threshold), PlotPoints::from_iter(
                (0..self.readings.len()).map(|i| [i as f64, self.high_threshold as f64])
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
        
        // Daily averages chart
        if !self.daily_stats.is_empty() {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Daily Averages with Range").heading());
                ui.label(format!("n = {} days", self.daily_stats.len()));
                
                let avg_points: PlotPoints = self.daily_stats.iter().enumerate()
                    .map(|(i, d)| [i as f64, d.avg_mg_dl])
                    .collect();
                
                let min_points: PlotPoints = self.daily_stats.iter().enumerate()
                    .map(|(i, d)| [i as f64, d.min_mg_dl as f64])
                    .collect();
                
                let max_points: PlotPoints = self.daily_stats.iter().enumerate()
                    .map(|(i, d)| [i as f64, d.max_mg_dl as f64])
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
                
                // Show date labels
                ui.horizontal_wrapped(|ui| {
                    ui.label("Days: ");
                    for (i, stat) in self.daily_stats.iter().enumerate() {
                        if i > 0 {
                            ui.label(" | ");
                        }
                        ui.label(format!("{}: {} (n={})", i, &stat.date, stat.count));
                    }
                });
            });
        }
        
        ui.add_space(20.0);
        
        // Quick distribution view
        ui.group(|ui| {
            ui.label(egui::RichText::new("Reading Distribution").heading());
            
            // Count readings in ranges
            let very_low = self.readings.iter().filter(|r| r.mg_dl < 54).count();
            let low = self.readings.iter().filter(|r| r.mg_dl >= 54 && r.mg_dl < 70).count();
            let normal = self.readings.iter().filter(|r| r.mg_dl >= 70 && r.mg_dl <= 180).count();
            let high = self.readings.iter().filter(|r| r.mg_dl > 180 && r.mg_dl <= 250).count();
            let very_high = self.readings.iter().filter(|r| r.mg_dl > 250).count();
            
            let total = self.readings.len() as f32;
            
            let ranges = [
                ("< 54 (Very Low)", very_low, egui::Color32::from_rgb(200, 50, 50)),
                ("54-70 (Low)", low, egui::Color32::from_rgb(255, 100, 100)),
                ("70-180 (Target)", normal, egui::Color32::from_rgb(100, 200, 100)),
                ("180-250 (High)", high, egui::Color32::from_rgb(255, 180, 100)),
                ("> 250 (Very High)", very_high, egui::Color32::from_rgb(255, 100, 50)),
            ];
            
            for (label, count, color) in ranges {
                ui.horizontal(|ui| {
                    ui.label(format!("{:<20}", label));
                    let bar = egui::ProgressBar::new(count as f32 / total)
                        .text(format!("{} ({:.1}%)", count, (count as f32 / total) * 100.0))
                        .fill(color);
                    ui.add(bar);
                });
            }
        });
    }
    
    fn show_histogram_chart(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Glucose Distribution Histogram").heading());
            ui.label(format!("n = {} readings, bin width = 20 mg/dL", self.readings.len()));
            
            if self.histogram_bins.is_empty() {
                ui.label("No histogram data available.");
                return;
            }
            
            // Calculate max count for scaling (used for reference)
            let _max_count = self.histogram_bins.iter().map(|b| b.count).max().unwrap_or(1);
            
            // Draw histogram bars using egui_plot
            use egui_plot::{Bar, BarChart};
            
            let bars: Vec<Bar> = self.histogram_bins.iter()
                .map(|bin| {
                    let mid = (bin.range_start + bin.range_end) as f64 / 2.0;
                    let color = if bin.range_end <= self.low_threshold {
                        egui::Color32::from_rgb(255, 100, 100)
                    } else if bin.range_start >= self.high_threshold {
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
                .x_axis_label("Glucose (mg/dL)")
                .y_axis_label("Count")
                .show(ui, |plot_ui| {
                    plot_ui.bar_chart(chart);
                });
            
            ui.add_space(10.0);
            
            // Statistics summary
            if !self.readings.is_empty() {
                let values: Vec<f64> = self.readings.iter().map(|r| r.mg_dl as f64).collect();
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                let mut sorted = values.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let median = sorted[sorted.len() / 2];
                let variance: f64 = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
                let std_dev = variance.sqrt();
                
                // 95% CI for mean
                let se = std_dev / (values.len() as f64).sqrt();
                let ci_low = mean - 1.96 * se;
                let ci_high = mean + 1.96 * se;
                
                ui.horizontal(|ui| {
                    ui.label(format!("Mean: {:.1} mg/dL (95% CI: {:.1}-{:.1})", mean, ci_low, ci_high));
                    ui.separator();
                    ui.label(format!("Median: {:.1} mg/dL", median));
                    ui.separator();
                    ui.label(format!("SD: {:.1}", std_dev));
                });
            }
        });
    }
    
    fn show_time_of_day_chart(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Glucose by Hour of Day (Scatter + Boxplot)").heading());
            
            let total_readings: usize = self.hourly_stats.iter().map(|h| h.count).sum();
            ui.label(format!("n = {} readings across 24 hours", total_readings));
            
            if self.hourly_stats.is_empty() {
                ui.label("No hourly data available.");
                return;
            }
            
            // Create scatter plot with all points + boxplot overlay
            use egui_plot::{Points, BoxElem, BoxPlot, BoxSpread};
            
            // Collect all points for scatter
            let mut all_points: Vec<[f64; 2]> = Vec::new();
            for stat in &self.hourly_stats {
                for &val in &stat.readings {
                    // Add small jitter for visibility
                    let jitter = (val as f64 % 7.0 - 3.5) * 0.1;
                    all_points.push([stat.hour as f64 + jitter, val as f64]);
                }
            }
            
            let scatter = Points::new("Readings", PlotPoints::from_iter(all_points))
                .radius(2.0)
                .color(egui::Color32::from_rgba_unmultiplied(100, 150, 255, 100));
            
            // Create boxplot elements
            let boxes: Vec<BoxElem> = self.hourly_stats.iter()
                .filter(|s| s.count > 0)
                .map(|stat| {
                    BoxElem::new(stat.hour as f64, BoxSpread::new(
                        stat.min as f64,
                        stat.q1 as f64,
                        stat.median as f64,
                        stat.q3 as f64,
                        stat.max as f64,
                    ))
                    .whisker_width(0.3)
                    .box_width(0.6)
                    .fill(egui::Color32::from_rgba_unmultiplied(100, 200, 100, 150))
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(50, 150, 50)))
                })
                .collect();
            
            let boxplot = BoxPlot::new("Hourly Distribution", boxes);
            
            // Threshold lines
            let low_line = Line::new(format!("Low ({})", self.low_threshold), PlotPoints::from_iter(
                (0..25).map(|h| [h as f64, self.low_threshold as f64])
            ))
            .color(egui::Color32::from_rgb(255, 100, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            let high_line = Line::new(format!("High ({})", self.high_threshold), PlotPoints::from_iter(
                (0..25).map(|h| [h as f64, self.high_threshold as f64])
            ))
            .color(egui::Color32::from_rgb(255, 180, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            Plot::new("time_of_day_scatter")
                .height(350.0)
                .x_axis_label("Hour of Day")
                .y_axis_label("Glucose (mg/dL)")
                .legend(egui_plot::Legend::default())
                .show(ui, |plot_ui| {
                    plot_ui.points(scatter);
                    plot_ui.box_plot(boxplot);
                    plot_ui.line(low_line);
                    plot_ui.line(high_line);
                });
            
            ui.add_space(10.0);
            
            // Hourly summary table
            ui.collapsing("Hourly Statistics", |ui| {
                egui::Grid::new("hourly_stats_grid")
                    .num_columns(6)
                    .spacing([15.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Hour").strong());
                        ui.label(egui::RichText::new("Count").strong());
                        ui.label(egui::RichText::new("Mean±SD").strong());
                        ui.label(egui::RichText::new("Median").strong());
                        ui.label(egui::RichText::new("IQR").strong());
                        ui.label(egui::RichText::new("Range").strong());
                        ui.end_row();
                        
                        for stat in &self.hourly_stats {
                            if stat.count > 0 {
                                ui.label(format!("{:02}:00", stat.hour));
                                ui.label(format!("{}", stat.count));
                                ui.label(format!("{:.0}±{:.0}", stat.mean, stat.std_dev));
                                ui.label(format!("{}", stat.median));
                                ui.label(format!("{}-{}", stat.q1, stat.q3));
                                ui.label(format!("{}-{}", stat.min, stat.max));
                                ui.end_row();
                            }
                        }
                    });
            });
        });
    }
    
    fn show_daily_tir_trend(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Daily Time-in-Range Trend").heading());
            ui.label(format!("Target range: {}-{} mg/dL | n = {} days", 
                self.low_threshold, self.high_threshold, self.daily_tir.len()));
            
            if self.daily_tir.is_empty() {
                ui.label("No daily TIR data available.");
                return;
            }
            
            // Note: Stacked area chart lines are prepared but we use the simpler TIR trend line
            // The percentages are available if needed for a more complex visualization
            
            // In-range percentage trend line
            let tir_trend = Line::new("TIR %", PlotPoints::from_iter(
                self.daily_tir.iter().enumerate().map(|(i, d)| [i as f64, d.in_range_pct])
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
                    // Reference line at 70% TIR goal
                    let goal_line = Line::new("70% Goal", PlotPoints::from_iter(
                        (0..self.daily_tir.len() + 1).map(|i| [i as f64, 70.0])
                    ))
                    .color(egui::Color32::from_rgb(150, 150, 150))
                    .style(egui_plot::LineStyle::dashed_loose());
                    plot_ui.line(goal_line);
                });
            
            ui.add_space(10.0);
            
            // Summary stats
            if !self.daily_tir.is_empty() {
                let avg_tir: f64 = self.daily_tir.iter().map(|d| d.in_range_pct).sum::<f64>() 
                    / self.daily_tir.len() as f64;
                let days_at_goal = self.daily_tir.iter().filter(|d| d.in_range_pct >= 70.0).count();
                
                ui.horizontal(|ui| {
                    ui.label(format!("Average TIR: {:.1}%", avg_tir));
                    ui.separator();
                    ui.label(format!("Days at ≥70% goal: {}/{} ({:.1}%)", 
                        days_at_goal, self.daily_tir.len(), 
                        (days_at_goal as f64 / self.daily_tir.len() as f64) * 100.0));
                });
            }
            
            ui.add_space(10.0);
            
            // Daily breakdown table
            ui.collapsing("Daily Details", |ui| {
                egui::Grid::new("daily_tir_grid")
                    .num_columns(6)
                    .spacing([15.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Date").strong());
                        ui.label(egui::RichText::new("Count").strong());
                        ui.label(egui::RichText::new("Low %").strong());
                        ui.label(egui::RichText::new("In Range %").strong());
                        ui.label(egui::RichText::new("High %").strong());
                        ui.label(egui::RichText::new("Status").strong());
                        ui.end_row();
                        
                        for day in &self.daily_tir {
                            ui.label(&day.date);
                            ui.label(format!("{}", day.total));
                            ui.colored_label(
                                egui::Color32::from_rgb(255, 100, 100),
                                format!("{:.1}%", day.low_pct)
                            );
                            ui.colored_label(
                                egui::Color32::from_rgb(100, 200, 100),
                                format!("{:.1}%", day.in_range_pct)
                            );
                            ui.colored_label(
                                egui::Color32::from_rgb(255, 180, 100),
                                format!("{:.1}%", day.high_pct)
                            );
                            let status = if day.in_range_pct >= 70.0 { "✓" } else { "—" };
                            ui.label(status);
                            ui.end_row();
                        }
                    });
            });
        });
    }
    
    fn show_time_bins_boxplot(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Glucose by Clinical Time Periods (Boxplots)").heading());
            ui.label("Shows glucose patterns across clinically meaningful time windows");
            
            if self.time_bin_stats.is_empty() {
                ui.label("No time bin data available.");
                return;
            }
            
            use egui_plot::{BoxElem, BoxPlot, BoxSpread};
            
            // Create boxplot elements
            let boxes: Vec<BoxElem> = self.time_bin_stats.iter()
                .enumerate()
                .filter(|(_, s)| s.count > 0)
                .map(|(i, stat)| {
                    let color = if stat.mean < self.low_threshold as f64 {
                        egui::Color32::from_rgb(255, 120, 120)
                    } else if stat.mean > self.high_threshold as f64 {
                        egui::Color32::from_rgb(255, 200, 120)
                    } else {
                        egui::Color32::from_rgb(120, 200, 120)
                    };
                    
                    BoxElem::new(i as f64, BoxSpread::new(
                        stat.min as f64,
                        stat.q1 as f64,
                        stat.median as f64,
                        stat.q3 as f64,
                        stat.max as f64,
                    ))
                    .whisker_width(0.4)
                    .box_width(0.7)
                    .fill(color)
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 80, 80)))
                    .name(&stat.name)
                })
                .collect();
            
            let boxplot = BoxPlot::new("Time Bin Analysis", boxes);
            
            // Threshold lines
            let low_line = Line::new(format!("Low ({})", self.low_threshold), PlotPoints::from_iter(
                (-1..7).map(|x| [x as f64, self.low_threshold as f64])
            ))
            .color(egui::Color32::from_rgb(255, 100, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            let high_line = Line::new(format!("High ({})", self.high_threshold), PlotPoints::from_iter(
                (-1..7).map(|x| [x as f64, self.high_threshold as f64])
            ))
            .color(egui::Color32::from_rgb(255, 180, 100))
            .style(egui_plot::LineStyle::dashed_dense());
            
            Plot::new("time_bins_boxplot")
                .height(300.0)
                .y_axis_label("Glucose (mg/dL)")
                .legend(egui_plot::Legend::default())
                .show(ui, |plot_ui| {
                    plot_ui.box_plot(boxplot);
                    plot_ui.line(low_line);
                    plot_ui.line(high_line);
                });
            
            ui.add_space(10.0);
            
            // Time bin statistics table
            egui::Grid::new("time_bin_stats_grid")
                .num_columns(7)
                .spacing([12.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Time Period").strong());
                    ui.label(egui::RichText::new("Hours").strong());
                    ui.label(egui::RichText::new("n").strong());
                    ui.label(egui::RichText::new("Mean±SD").strong());
                    ui.label(egui::RichText::new("Median").strong());
                    ui.label(egui::RichText::new("IQR").strong());
                    ui.label(egui::RichText::new("95% CI").strong());
                    ui.end_row();
                    
                    for stat in &self.time_bin_stats {
                        ui.label(&stat.name);
                        ui.label(&stat.description);
                        ui.label(format!("{}", stat.count));
                        
                        if stat.count > 0 {
                            let color = self.get_reading_color(stat.mean as u16);
                            ui.colored_label(color, format!("{:.0}±{:.0}", stat.mean, stat.std_dev));
                            ui.label(format!("{}", stat.median));
                            ui.label(format!("{}-{}", stat.q1, stat.q3));
                            
                            // 95% CI
                            if stat.count > 1 {
                                let se = stat.std_dev / (stat.count as f64).sqrt();
                                let ci_low = stat.mean - 1.96 * se;
                                let ci_high = stat.mean + 1.96 * se;
                                ui.label(format!("{:.0}-{:.0}", ci_low, ci_high));
                            } else {
                                ui.label("-");
                            }
                        } else {
                            ui.label("-");
                            ui.label("-");
                            ui.label("-");
                            ui.label("-");
                        }
                        ui.end_row();
                    }
                });
        });
    }
    
    fn show_calendar_view(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Calendar View (Daily Small Multiples)").heading());
            ui.label(format!("Showing {} days with readings", self.calendar_data.len()));
            
            if self.calendar_data.is_empty() {
                ui.label("No calendar data available.");
                return;
            }
            
            // Group by week
            use std::collections::BTreeMap;
            let mut weeks: BTreeMap<u32, Vec<&CalendarDay>> = BTreeMap::new();
            for day in &self.calendar_data {
                weeks.entry(day.week_of_year).or_insert_with(Vec::new).push(day);
            }
            
            let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
            
            // Header row
            ui.horizontal(|ui| {
                ui.label("Week");
                for name in day_names {
                    ui.add_sized([80.0, 20.0], egui::Label::new(name));
                }
            });
            ui.separator();
            
            // Calendar grid
            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                for (week, days) in weeks.iter().rev().take(12) {  // Show last 12 weeks
                    ui.horizontal(|ui| {
                        ui.label(format!("W{}", week));
                        
                        for dow in 0..7 {
                            let day_data = days.iter().find(|d| d.day_of_week == dow);
                            
                            ui.allocate_ui(egui::Vec2::new(80.0, 60.0), |ui| {
                                if let Some(day) = day_data {
                                    // Color based on TIR
                                    let bg_color = if day.in_range_pct >= 70.0 {
                                        egui::Color32::from_rgb(200, 255, 200)
                                    } else if day.in_range_pct >= 50.0 {
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
                                                ui.label(format!("n={}", day.count));
                                                ui.label(format!("{:.0}", day.mean));
                                                ui.label(format!("{}%", day.in_range_pct as i32));
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
            
            // Legend
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
            // Set to reactive mode - only repaint on input events, not continuously
            cc.egui_ctx.set_visuals(egui::Visuals::default());
            Ok(Box::new(AccuChekApp::new(cc, db_path)))
        }),
    )
}
