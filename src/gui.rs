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
use crate::storage::{Storage, StoredReading, TimeInRange, DailyStats};
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
    
    // UI state
    current_tab: Tab,
    selected_reading: Option<usize>,
    note_edit_buffer: String,
    tag_edit_buffer: String,
    search_query: String,
    
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
            current_tab: Tab::Dashboard,
            selected_reading: None,
            note_edit_buffer: String::new(),
            tag_edit_buffer: String::new(),
            search_query: String::new(),
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
        ui.heading("Charts");
        ui.separator();
        
        if self.readings.is_empty() {
            ui.label("No data to display. Sync your device first.");
            return;
        }
        
        // Glucose trend chart
        ui.group(|ui| {
            ui.label(egui::RichText::new("Glucose Trend (All Readings)").heading());
            
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
                ui.label(egui::RichText::new("Daily Averages").heading());
                
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
                        ui.label(format!("{}: {}", i, &stat.date));
                    }
                });
            });
        }
        
        ui.add_space(20.0);
        
        // Distribution histogram-like display
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
