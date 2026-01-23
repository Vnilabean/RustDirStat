//! Terminal User Interface for ferris-scan
//!
//! This provides an interactive terminal UI for the disk usage analyzer.
//! 
//! # Architecture
//! 
//! This is a thin wrapper around the core `ferris_scan` library. It uses
//! `ratatui` for rendering and handles all terminal-specific logic.

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ferris_scan::{Node, Scanner, ScanReport, SharedProgress};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    env,
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

// ============================================================================
// APPLICATION STATE
// ============================================================================

enum AppState {
    Scanning,
    ViewingResults(Node, ScanReport),
}

/// Navigation state for tree browsing
struct NavigationState {
    /// Stack of nodes from root to current directory
    path: Vec<Node>,
    /// Currently selected item index in the list
    selected: usize,
}

impl NavigationState {
    fn new(root: Node) -> Self {
        Self {
            path: vec![root],
            selected: 0,
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
    fn drill_down(&mut self, index: usize) -> bool {
        let current = self.current();
        if let Some(child) = current.children.get(index) {
            if child.is_dir {
                self.path.push(child.clone());
                self.selected = 0;
                return true;
            }
        }
        false
    }

    /// Navigate up to parent directory
    fn drill_up(&mut self) -> bool {
        if self.path.len() > 1 {
            self.path.pop();
            self.selected = 0;
            return true;
        }
        false
    }
}

struct App {
    state: AppState,
    should_quit: bool,
    scan_path: PathBuf,
    shared_progress: Arc<SharedProgress>,
    popup_message: Option<String>,
    navigation: Option<NavigationState>,
    list_state: ListState,
}

impl App {
    fn new(scan_path: PathBuf) -> Self {
        Self {
            state: AppState::Scanning,
            should_quit: false,
            scan_path,
            shared_progress: Arc::new(SharedProgress::default()),
            popup_message: None,
            navigation: None,
            list_state: ListState::default(),
        }
    }

    fn show_popup(&mut self, message: String) {
        self.popup_message = Some(message);
    }

    fn close_popup(&mut self) {
        self.popup_message = None;
    }

    fn handle_export(&mut self) {
        #[cfg(feature = "pro")]
        {
            // Pro version: Actually export the data
            if let AppState::ViewingResults(ref root, _) = self.state {
                let output_path = self.scan_path.with_file_name("ferris-scan-export.csv");
                let scanner = Scanner::new();
                
                match scanner.export_csv(root, &output_path) {
                    Ok(_) => {
                        self.show_popup(format!(
                            "✓ Export successful!\n\nSaved to:\n{}",
                            output_path.display()
                        ));
                    }
                    Err(e) => {
                        self.show_popup(format!("✗ Export failed:\n{}", e));
                    }
                }
            } else {
                self.show_popup("Please wait for scan to complete first.".to_string());
            }
        }

        #[cfg(not(feature = "pro"))]
        {
            // Free version: Show upgrade message
            self.show_popup(
                "⚠ This is a Pro Feature\n\n\
                CSV Export is only available in ferris-scan Pro.\n\n\
                Build with: cargo build --release --features pro"
                    .to_string(),
            );
        }
    }
}

// ============================================================================
// MAIN ENTRY POINT
// ============================================================================

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let scan_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        env::current_dir()?
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(scan_path.clone());

    // Spawn scanning thread
    let shared_progress = Arc::clone(&app.shared_progress);
    let scan_done = Arc::new(AtomicBool::new(false));
    let scan_done_clone = Arc::clone(&scan_done);

    let scan_handle = thread::spawn(move || {
        let scanner = Scanner::new();
        let result = scanner.scan_with_progress(&scan_path, shared_progress);
        scan_done_clone.store(true, Ordering::Relaxed);
        result
    });

    // Run UI loop
    let res = run_app(&mut terminal, &mut app, scan_handle, scan_done);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

// ============================================================================
// UI EVENT LOOP
// ============================================================================

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    scan_handle: thread::JoinHandle<Result<(Node, ScanReport)>>,
    scan_done: Arc<AtomicBool>,
) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    let mut last_draw = std::time::Instant::now();
    let mut scan_handle = Some(scan_handle);

    loop {
        // Check if scan is complete
        if scan_done.load(Ordering::Relaxed) {
            if let AppState::Scanning = app.state {
                if let Some(handle) = scan_handle.take() {
                    match handle.join() {
                        Ok(Ok((root, report))) => {
                            app.state = AppState::ViewingResults(root.clone(), report);
                            app.navigation = Some(NavigationState::new(root));
                            app.list_state.select(Some(0));
                        }
                        Ok(Err(e)) => {
                            app.show_popup(format!("Scan error: {}", e));
                        }
                        Err(_) => {
                            app.show_popup("Internal error: scan thread panicked".to_string());
                        }
                    }
                }
            }
        }

        // Render UI (throttled to ~30 FPS)
        if last_draw.elapsed() >= Duration::from_millis(33) {
            terminal.draw(|f| ui(f, &mut *app))?;
            last_draw = std::time::Instant::now();
        }

        // Handle input (with timeout for responsive UI)
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                // Only process key press, not release
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Handle popup close first
                if app.popup_message.is_some() {
                    app.close_popup();
                    continue;
                }

                // Main key handlers
                match key.code {
                    KeyCode::Char('q') => {
                        app.should_quit = true;
                    }
                    KeyCode::Esc => {
                        // If in a subdirectory, go up; otherwise quit
                        if let Some(ref mut nav) = app.navigation {
                            if nav.drill_up() {
                                app.list_state.select(Some(0));
                            } else {
                                app.should_quit = true;
                            }
                        } else {
                            app.should_quit = true;
                        }
                    }
                    KeyCode::Char('e') => {
                        app.handle_export();
                    }
                    KeyCode::Enter => {
                        // Drill down into selected directory
                        if let Some(ref mut nav) = app.navigation {
                            if let Some(selected) = app.list_state.selected() {
                                if nav.drill_down(selected) {
                                    app.list_state.select(Some(0));
                                }
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        // Go up one level
                        if let Some(ref mut nav) = app.navigation {
                            nav.drill_up();
                            app.list_state.select(Some(0));
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if let Some(ref mut nav) = app.navigation {
                            let current = nav.current();
                            if !current.children.is_empty() {
                                let selected = app.list_state.selected().unwrap_or(0);
                                let new_selected = if selected > 0 {
                                    selected - 1
                                } else {
                                    current.children.len() - 1
                                };
                                app.list_state.select(Some(new_selected));
                            }
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if let Some(ref mut nav) = app.navigation {
                            let current = nav.current();
                            if !current.children.is_empty() {
                                let selected = app.list_state.selected().unwrap_or(0);
                                let new_selected = if selected < current.children.len() - 1 {
                                    selected + 1
                                } else {
                                    0
                                };
                                app.list_state.select(Some(new_selected));
                            }
                        }
                    }
                    KeyCode::Char('h') => {
                        // Go up one level (same as Backspace)
                        if let Some(ref mut nav) = app.navigation {
                            nav.drill_up();
                            app.list_state.select(Some(0));
                        }
                    }
                    KeyCode::Char('l') => {
                        // Drill down into selected directory (same as Enter)
                        if let Some(ref mut nav) = app.navigation {
                            if let Some(selected) = app.list_state.selected() {
                                if nav.drill_down(selected) {
                                    app.list_state.select(Some(0));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

// ============================================================================
// UI RENDERING
// ============================================================================

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Footer
        ])
        .split(f.area());

    // Header
    render_header(f, chunks[0], app);

    // Content
    match &app.state {
        AppState::Scanning => render_scanning(f, chunks[1], app),
        AppState::ViewingResults(root, report) => {
            render_results(f, chunks[1], root, report, &app.navigation, &mut app.list_state)
        }
    }

    // Footer
    render_footer(f, chunks[2], app);

    // Popup (if any)
    if let Some(ref message) = app.popup_message {
        render_popup(f, message);
    }
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let title = format!("ferris-scan TUI v{} | {}", env!("CARGO_PKG_VERSION"), app.scan_path.display());
    
    #[cfg(feature = "pro")]
    let version_tag = " [PRO] ";
    #[cfg(not(feature = "pro"))]
    let version_tag = " [FREE] ";

    let header = Paragraph::new(title)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(version_tag)
                .title_alignment(Alignment::Right)
                .border_style(Style::default().fg(Color::LightGreen)),
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));

    f.render_widget(header, area);
}

fn render_scanning(f: &mut Frame, area: Rect, app: &App) {
    let files = app
        .shared_progress
        .files_scanned
        .load(Ordering::Relaxed);
    let last_path = app
        .shared_progress
        .last_path
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "Starting scan...".to_string());

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "⟳ Scanning in progress...",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("Files scanned: {}", files)),
        Line::from(""),
        Line::from(Span::styled(
            "Current path:",
            Style::default().add_modifier(Modifier::DIM),
        )),
        Line::from(last_path),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Status")
                .border_style(Style::default().fg(Color::Cyan))
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

fn render_results(f: &mut Frame, area: Rect, root: &Node, report: &ScanReport, navigation: &Option<NavigationState>, list_state: &mut ListState) {
    // First split: breadcrumb at top, content below
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Breadcrumb
            Constraint::Min(0),     // Multi-pane content
        ])
        .split(area);

    // Breadcrumb
    let breadcrumb_text = navigation
        .as_ref()
        .map(|nav| nav.breadcrumb())
        .unwrap_or_else(|| "Root".to_string());
    
    let breadcrumb = Paragraph::new(breadcrumb_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Location")
                .border_style(Style::default().fg(Color::LightGreen))
        )
        .style(Style::default().fg(Color::LightCyan));
    
    f.render_widget(breadcrumb, main_chunks[0]);

    // Multi-pane layout: Tree | Details | Progress
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),  // Tree view
            Constraint::Percentage(35), // Details view
            Constraint::Percentage(25), // Progress/Stats view
        ])
        .split(main_chunks[1]);

    // Get current node and selected item
    let current_node = navigation
        .as_ref()
        .map(|nav| nav.current())
        .unwrap_or(root);
    
    let selected_index = list_state.selected().unwrap_or(0);
    let selected_item = current_node.children.get(selected_index);

    // Render tree pane (left)
    render_tree_pane(f, panes[0], current_node, list_state);

    // Render details pane (middle)
    render_details_pane(f, panes[1], selected_item, current_node);

    // Render progress/stats pane (right)
    render_stats_pane(f, panes[2], root, report, current_node);
}

fn render_tree_pane(f: &mut Frame, area: Rect, current_node: &Node, list_state: &mut ListState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Header row
            Constraint::Min(0),     // List
        ])
        .split(area);

    // Calculate available width for the list (accounting for borders)
    let available_width = area.width.saturating_sub(2) as usize; // Accounts for borders
    
    // Reserve space for size column,, need enough for largest formatted size with units
    // "1234.56 GB" is 10 chars, but lets use 12 to be safe
    let size_column_width = 12;
    let name_column_width = available_width.saturating_sub(size_column_width + 1); // +1 for spacing
    
    // Ensure minimum widths
    let size_column_width = size_column_width.max(10); // At least 10 for "1234.56 GB"
    let name_column_width = name_column_width.max(10); // At least 10 for names
    
    // Header row
    let header_text = format!(
        "{:<width$} {:>size_width$}",
        "Name",
        "Size",
        width = name_column_width,
        size_width = size_column_width
    );
    let header = Paragraph::new(Line::from(Span::styled(
        header_text,
        Style::default()
            .fg(Color::LightGreen)
            .add_modifier(Modifier::BOLD),
    )));
    f.render_widget(header, chunks[0]);

    // List of items
    let mut items = Vec::new();
    for child in &current_node.children {
        let size_str = format_size(child.size);
        let type_indicator = if child.is_dir { "📁" } else { "📄" };
        
        let size_str_len = size_str.chars().count();
        
        let max_name_len = available_width
            .saturating_sub(2)
            .saturating_sub(1)
            .saturating_sub(size_str_len);
        
        let max_name_len = max_name_len.max(1);
        
        // Truncate name if needed
        let display_name = if child.name.chars().count() > max_name_len {
            let truncated: String = child.name.chars().take(max_name_len.saturating_sub(3)).collect();
            format!("{}...", truncated)
        } else {
            child.name.clone()
        };
        
        // Build the line ensuring size_str (with units) is always visible
        // Format: emoji + name + padding + size_str (with units)
        let name_with_emoji = format!("{} {}", type_indicator, display_name);
        
        // Calculate maximum line length
        // We'll truncate the name if needed, but NEVER truncate size_str
        let max_line_len = available_width;
        let size_str_bytes = size_str.len(); // size_str is ASCII, so bytes == chars
        
        // Calculate how much space we need for name + padding
        // We MUST reserve size_str_bytes for the size (with units)
        let max_name_bytes = max_line_len.saturating_sub(size_str_bytes).saturating_sub(1); 
        
        // Truncate name_with_emoji if it's too long (but preserve size_str)
        let final_name = if name_with_emoji.len() > max_name_bytes {
            // Truncate name to fit (safely handle UTF-8 multi-byte characters)
            let truncate_to = max_name_bytes.saturating_sub(3);
            if truncate_to > 0 {
                // Find the last character whose end position fits within truncate_to bytes
                let safe_truncate = name_with_emoji
                    .char_indices()
                    .take_while(|(idx, c)| idx + c.len_utf8() <= truncate_to)
                    .last()
                    .map(|(idx, c)| idx + c.len_utf8())
                    .unwrap_or(0);
                format!("{}...", &name_with_emoji[..safe_truncate])
            } else {
                name_with_emoji.chars().take(1).collect::<String>()
            }
        } else {
            name_with_emoji
        };
        
        // Calculate padding to right-align size_str
        let final_name_len = final_name.len();
        let padding_needed = max_line_len
            .saturating_sub(final_name_len)
            .saturating_sub(size_str_bytes);
        
        let padding = " ".repeat(padding_needed.max(1));
        
        // Build final line: name + padding + size_str
        // size_str is ALWAYS at the end and NEVER truncated
        let final_line = format!("{}{}{}", final_name, padding, size_str);
        
        // Verify size_str is at the end
        if final_line.ends_with(&size_str) {
            // Split for coloring
            let split_point = final_line.len() - size_str_bytes;
            let name_part = final_line[..split_point].to_string();
            let size_part = final_line[split_point..].to_string();
            
            // Verify size_part equals size_str
            if size_part == size_str {
                items.push(ListItem::new(Line::from(vec![
                    Span::raw(name_part),
                    Span::styled(
                        size_part,
                        Style::default().fg(Color::Cyan),
                    ),
                ])));
            } else {
                // Fallback: show whole line
                items.push(ListItem::new(Line::from(Span::raw(final_line))));
            }
        } else {
            // Fallback: size_str not at end (shouldn't happen)
            items.push(ListItem::new(Line::from(Span::raw(final_line))));
        }
    }

    let title = format!("Tree View | {} items", current_node.children.len());

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Cyan))
        )
        .highlight_style(
            Style::default()
                .bg(Color::LightGreen)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    f.render_stateful_widget(list, chunks[1], list_state);
}

fn render_details_pane(f: &mut Frame, area: Rect, selected_item: Option<&Node>, current_node: &Node) {
    let details_text = if let Some(item) = selected_item {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "Selected Item Details",
                Style::default()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Name: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&item.name),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(if item.is_dir { "Directory" } else { "File" }),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Size: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format_size(item.size),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Path: ", Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(Span::styled(
                item.path.display().to_string(),
                Style::default().fg(Color::LightCyan),
            )),
            Line::from(""),
            if item.is_dir {
                Line::from(vec![
                    Span::styled("Children: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!("{} items", item.children.len())),
                ])
            } else {
                Line::from("")
            },
        ]
    } else {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "No item selected",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::ITALIC),
            )),
            Line::from(""),
            Line::from("Use ↑/↓ to navigate"),
            Line::from("and select an item"),
            Line::from("to view details."),
        ]
    };

    let details = Paragraph::new(details_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Details")
                .border_style(Style::default().fg(Color::LightGreen))
        )
        .wrap(Wrap { trim: true });

    f.render_widget(details, area);
}

fn render_stats_pane(f: &mut Frame, area: Rect, root: &Node, report: &ScanReport, current_node: &Node) {
    let stats_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Scan Statistics",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Total Size: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format_size(root.size),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Skipped: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{} entries", report.skipped.len())),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Current Directory",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Name: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&current_node.name),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Size: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format_size(current_node.size),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Items: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}", current_node.children.len())),
        ]),
    ];

    let stats = Paragraph::new(stats_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Progress & Stats")
                .border_style(Style::default().fg(Color::Cyan))
        )
        .wrap(Wrap { trim: true });

    f.render_widget(stats, area);
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let key_hints = match &app.state {
        AppState::Scanning => vec![
            Span::styled("q", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
            Span::raw(": Quit"),
        ],
        AppState::ViewingResults(_, _) => vec![
            Span::styled("q", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
            Span::raw(": Quit | "),
            Span::styled("Enter", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": Open | "),
            Span::styled("Esc", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
            Span::raw(": Back | "),
            Span::styled("↑/↓", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" or "),
            Span::styled("h/j/k/l", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": Nav"),
        ],
    };

    let footer = Paragraph::new(Line::from(key_hints))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::LightGreen))
        )
        .alignment(Alignment::Center);

    f.render_widget(footer, area);
}

fn render_popup(f: &mut Frame, message: &str) {
    let area = centered_rect(60, 40, f.area());

    let block = Block::default()
        .title(" Message ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightGreen))
        .style(Style::default().bg(Color::Black));

    let text = Paragraph::new(message)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Cyan));

    f.render_widget(Clear, area);
    f.render_widget(text, area);
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

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
