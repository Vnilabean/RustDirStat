//! Graphical User Interface for ferris-scan
//!
//! This provides a windowed GUI for the disk usage analyzer using eframe/egui.
//! 
//! # Architecture
//! 
//! This is a thin wrapper around the core `ferris_scan` library. It uses
//! `eframe` for rendering and handles all GUI-specific logic.

use eframe::egui;
use ferris_scan::{Node, ScanReport, Scanner, SharedProgress};
use std::{
    env,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

// ============================================================================
// APPLICATION STATE
// ============================================================================

enum ScanStatus {
    Idle,
    Scanning {
        progress: Arc<SharedProgress>,
        done_flag: Arc<AtomicBool>,
    },
    Done {
        root: Node,
        report: ScanReport,
    },
    Error(String),
}

/// Navigation state for tree browsing
struct NavigationState {
    /// Stack of nodes from root to current directory
    path: Vec<Node>,
}

impl NavigationState {
    fn new(root: Node) -> Self {
        Self {
            path: vec![root],
        }
    }

    /// Get the current node being viewed
    fn current(&self) -> &Node {
        self.path.last().unwrap()
    }

    /// Get breadcrumb path as a string
    fn breadcrumb(&self) -> String {
        self.path
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// Navigate into a child directory
    fn drill_down(&mut self, child: Node) {
        self.path.push(child);
    }

    /// Navigate up to parent directory
    fn drill_up(&mut self) -> bool {
        if self.path.len() > 1 {
            self.path.pop();
            return true;
        }
        false
    }
}

struct FerrisScanApp {
    scan_path: String,
    status: Arc<Mutex<ScanStatus>>,
    popup_message: Option<String>,
    navigation: Option<NavigationState>,
}

impl FerrisScanApp {
    fn new(initial_path: PathBuf) -> Self {
        Self {
            scan_path: initial_path.display().to_string(),
            status: Arc::new(Mutex::new(ScanStatus::Idle)),
            popup_message: None,
            navigation: None,
        }
    }

    fn start_scan(&mut self) {
        let path = PathBuf::from(&self.scan_path);
        
        // Validate path
        if !path.exists() {
            self.popup_message = Some(format!("Path does not exist: {}", path.display()));
            return;
        }

        let progress = Arc::new(SharedProgress::default());
        let done_flag = Arc::new(AtomicBool::new(false));

        // Update status to scanning
        *self.status.lock().unwrap() = ScanStatus::Scanning {
            progress: Arc::clone(&progress),
            done_flag: Arc::clone(&done_flag),
        };

        // Spawn scan thread
        let status_clone = Arc::clone(&self.status);
        let progress_clone = Arc::clone(&progress);
        let done_flag_clone = Arc::clone(&done_flag);

        thread::spawn(move || {
            let scanner = Scanner::new();
            let result = scanner.scan_with_progress(&path, progress_clone);
            done_flag_clone.store(true, Ordering::Relaxed);

            // Update status with result
            let new_status = match result {
                Ok((root, report)) => {
                    // Initialize navigation with root
                    // Note: We need to pass this to the app, but we can't easily do that here
                    // So we'll initialize it when the status is read
                    ScanStatus::Done { root, report }
                }
                Err(e) => ScanStatus::Error(e.to_string()),
            };

            *status_clone.lock().unwrap() = new_status;
        });
    }

    fn handle_export(&mut self, root: &Node) {
        #[cfg(feature = "pro")]
        {
            let path = PathBuf::from(&self.scan_path);
            let output_path = path.with_file_name("ferris-scan-export.csv");
            let scanner = Scanner::new();

            match scanner.export_csv(root, &output_path) {
                Ok(_) => {
                    self.popup_message = Some(format!(
                        "Export successful!\n\nSaved to:\n{}",
                        output_path.display()
                    ));
                }
                Err(e) => {
                    self.popup_message = Some(format!("Export failed:\n{}", e));
                }
            }
        }

        #[cfg(not(feature = "pro"))]
        {
            let _ = root; // Suppress unused warning
            self.popup_message = Some(
                "This is a Pro Feature\n\n\
                CSV Export is only available in ferris-scan Pro.\n\n\
                Build with: cargo build --release --features pro --bin ferris-scan-gui"
                    .to_string(),
            );
        }
    }
}

impl eframe::App for FerrisScanApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request repaint for progress updates
        ctx.request_repaint();

        // Track user actions to apply after rendering
        let mut should_start_scan = false;
        let mut should_export = false;
        let mut should_reset = false;
        let mut should_drill_up = false;
        let mut should_drill_down: Option<Node> = None;
        let mut root_for_export: Option<Node> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("🦀 ferris-scan GUI");
            ui.add_space(10.0);

            // Version badge
            #[cfg(feature = "pro")]
            let version = "v0.1.0 [PRO]";
            #[cfg(not(feature = "pro"))]
            let version = "v0.1.0 [FREE]";

            ui.label(version);
            ui.add_space(10.0);

            // Path input
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.text_edit_singleline(&mut self.scan_path);
            });

            ui.add_space(10.0);

            // Status display and controls
            let status = self.status.lock().unwrap();
            match &*status {
                ScanStatus::Idle => {
                    if ui.button("Start Scan").clicked() {
                        should_start_scan = true;
                    }
                }
                ScanStatus::Scanning {
                    progress,
                    done_flag,
                } => {
                    let files = progress.files_scanned.load(Ordering::Relaxed);
                    let last_path = progress
                        .last_path
                        .lock()
                        .ok()
                        .and_then(|g| g.as_ref().map(|p| p.display().to_string()))
                        .unwrap_or_else(|| "Starting...".to_string());

                    ui.label(format!("⟳ Scanning in progress..."));
                    ui.label(format!("Files scanned: {}", files));
                    ui.add_space(5.0);
                    ui.label("Current path:");
                    ui.label(last_path);

                    // Check if done
                    if done_flag.load(Ordering::Relaxed) {
                        ctx.request_repaint();
                    }
                }
                ScanStatus::Done { root, report } => {
                    // Initialize navigation if not already done
                    if self.navigation.is_none() {
                        self.navigation = Some(NavigationState::new(root.clone()));
                    }

                    ui.label(format!("✓ Scan complete!"));
                    ui.label(format!("Total size: {}", format_size(root.size)));
                    ui.label(format!("Skipped entries: {}", report.skipped.len()));
                    ui.add_space(10.0);

                    // Breadcrumb navigation
                    let breadcrumb = self.navigation
                        .as_ref()
                        .map(|nav| nav.breadcrumb())
                        .unwrap_or_else(|| "Root".to_string());
                    let can_go_up = self.navigation
                        .as_ref()
                        .map(|nav| nav.path.len() > 1)
                        .unwrap_or(false);
                    
                    ui.horizontal(|ui| {
                        ui.label("Location:");
                        ui.label(egui::RichText::new(&breadcrumb).color(egui::Color32::from_rgb(100, 200, 255)));
                        
                        if can_go_up {
                            if ui.button("← Go Up").clicked() {
                                should_drill_up = true;
                            }
                        }
                    });
                    ui.separator();

                    // Current directory entries
                    let current_node = self.navigation
                        .as_ref()
                        .map(|nav| nav.current())
                        .unwrap_or(root);

                    ui.heading(format!("Entries in: {}", current_node.name));
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for child in &current_node.children {
                                let icon = if child.is_dir { "📁" } else { "📄" };
                                ui.horizontal(|ui| {
                                    let label_text = format!("{} {}", icon, child.name);
                                    
                                    if child.is_dir {
                                        // Make directories clickable
                                        if ui.button(label_text).clicked() {
                                            should_drill_down = Some(child.clone());
                                        }
                                    } else {
                                        ui.label(label_text);
                                    }
                                    
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(format_size(child.size));
                                        },
                                    );
                                });
                            }
                        });

                    ui.add_space(10.0);

                    // Action buttons
                    ui.horizontal(|ui| {
                        if ui.button("Export CSV").clicked() {
                            should_export = true;
                            root_for_export = Some(root.clone());
                        }

                        if ui.button("New Scan").clicked() {
                            should_reset = true;
                        }
                    });
                }
                ScanStatus::Error(err) => {
                    ui.colored_label(egui::Color32::RED, format!("✗ Error: {}", err));
                    ui.add_space(10.0);

                    if ui.button("Try Again").clicked() {
                        should_reset = true;
                    }
                }
            }
        });

        // Apply actions after releasing lock
        if should_start_scan {
            self.start_scan();
        }
        if should_export {
            if let Some(root) = root_for_export {
                self.handle_export(&root);
            }
        }
        if should_reset {
            *self.status.lock().unwrap() = ScanStatus::Idle;
            self.navigation = None;
        }
        if should_drill_up {
            if let Some(ref mut nav) = self.navigation {
                nav.drill_up();
            }
        }
        if let Some(child) = should_drill_down {
            if let Some(ref mut nav) = self.navigation {
                nav.drill_down(child);
            }
        }

        // Popup modal
        let popup_msg = self.popup_message.clone();
        if let Some(message) = popup_msg {
            let mut should_close = false;
            egui::Window::new("Message")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(&message);
                    ui.add_space(10.0);

                    if ui.button("OK").clicked() {
                        should_close = true;
                    }
                });

            if should_close {
                self.popup_message = None;
            }
        }
    }
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

// ============================================================================
// MAIN ENTRY POINT
// ============================================================================

fn main() -> eframe::Result<()> {
    let args: Vec<String> = env::args().collect();
    let initial_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ferris-scan",
        options,
        Box::new(|_cc| Ok(Box::new(FerrisScanApp::new(initial_path)))),
    )
}
