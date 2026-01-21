//! Core library for ferris-scan - disk usage analyzer
//! 
//! # Overview
//!
//! This library provides high-performance disk usage scanning with feature gated Pro functionality.
//!
//! # Business Model: Open Source Code, Paid Binaries
//!
//! - **Free Version:** Full scanning capabilities
//! - **Pro Version:** Adds data export (CSV) and advanced features
//!
//! # Usage
//!
//! ```rust
//! use ferris_scan::Scanner;
//! use std::path::Path;
//!
//! let scanner = Scanner::new();
//! let result = scanner.scan(Path::new("."));
//! ```
//! 

use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicU64, atomic::Ordering, mpsc, Arc, Mutex};
use std::time::Instant;

use jwalk::WalkDir;

// Pro only imports (conditional compilation)
#[cfg(feature = "pro")]
use serde::Serialize;





/// Represents a file or directory node in the filesystem tree
#[derive(Debug, Clone)]
#[cfg_attr(feature = "pro", derive(Serialize))]
pub struct Node {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    #[cfg_attr(feature = "pro", serde(skip_serializing_if = "Vec::is_empty"))]
    pub children: Vec<Node>,
    pub path: PathBuf,
}





impl Node {
    pub fn new(name: String, path: PathBuf, is_dir: bool) -> Self {
        Self {
            name,
            path,
            is_dir,
            size: 0,
            children: Vec::new(),
        }
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.size.cmp(&self.size) // Sort descending by size
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size && self.name == other.name
    }
}

impl Eq for Node {}

/// Progress update sent during scanning
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub files_scanned: usize,
    pub current_path: PathBuf,
    pub elapsed: std::time::Duration,
}



/// Shared progress state for tick-based UIs.
///
/// The scanner updates these fields frequently; the UI should redraw on a timer
/// (e.g. every 100-200ms) by reading them.
#[derive(Debug, Default)]
pub struct SharedProgress {
    /// Number of files processed
    pub files_scanned: AtomicU64,
    /// Last path the scanner touched 
    pub last_path: Mutex<Option<PathBuf>>,
}

/// Entry that was skipped during scanning (permissions)
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedEntry {
    pub path: Option<PathBuf>,
    pub message: String,
}

/// Additional information gathered during a scan.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanReport {
    pub skipped: Vec<SkippedEntry>,
}

/// Scan a directory and build a tree structure of disk usage
pub fn scan_directory<P: AsRef<Path>>(
    root: P,
    progress_tx: Option<mpsc::Sender<ScanProgress>>,
) -> anyhow::Result<Node> {
    Ok(scan_directory_with_report(root, progress_tx)?.0)
}

/// Scan a directory and return both the tree and a report
pub fn scan_directory_with_report<P: AsRef<Path>>(
    root: P,
    progress_tx: Option<mpsc::Sender<ScanProgress>>,
) -> anyhow::Result<(Node, ScanReport)> {
    scan_directory_with_report_shared(root, progress_tx, None)
}

/// Scan a directory and return both the tree and a report, while optionally updating shared progress.
pub fn scan_directory_with_report_shared<P: AsRef<Path>>(
    root: P,
    progress_tx: Option<mpsc::Sender<ScanProgress>>,
    shared_progress: Option<Arc<SharedProgress>>,
) -> anyhow::Result<(Node, ScanReport)> {
    let start = Instant::now();
    let root_path = root.as_ref().to_path_buf();
    let mut report = ScanReport::default();

    // Build tree structure
    let mut root_node = Node::new(
        root_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".")
            .to_string(),
        root_path.clone(),
        true,
    );

    // Stream entries via jwalk 
    let mut files_scanned: usize = 0;
    for entry in WalkDir::new(&root_path).sort(true) {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path == root_path {
                    continue;
                }

                if let Some(ref sp) = shared_progress {
                    if let Ok(mut lp) = sp.last_path.lock() {
                        *lp = Some(path.to_path_buf());
                    }
                }

                if let Some(ref tx) = progress_tx {
                    // Keep progress lightweight; update count for files only.
                    let _ = tx.send(ScanProgress {
                        files_scanned,
                        current_path: path.to_path_buf(),
                        elapsed: start.elapsed(),
                    });
                }

                let Ok(relative) = path.strip_prefix(&root_path) else {
                    continue;
                };

                let is_dir = entry.file_type().is_dir();
                if is_dir {
                    ensure_dir_path(&mut root_node, relative);
                    continue;
                }

                // For files, use metadata length as size.
                let md = match entry.metadata() {
                    Ok(md) => md,
                    Err(e) => {
                        if is_permission_denied(&e) {
                            report.skipped.push(SkippedEntry {
                                path: Some(path.to_path_buf()),
                                message: e.to_string(),
                            });
                        }
                        continue;
                    }
                };
                files_scanned += 1;
                if let Some(ref sp) = shared_progress {
                    sp.files_scanned.store(files_scanned as u64, Ordering::Relaxed);
                }
                add_file_to_tree(&mut root_node, relative, md.len());
            }
            Err(e) => {
                // Windows gotcha: permission denied (System Volume Information, etc.)
                if is_permission_denied(&e) {
                    report.skipped.push(SkippedEntry {
                        path: None,
                        message: e.to_string(),
                    });
                }
                continue;
            }
        }
    }

    // Calculate directory sizes as sum(children) for dirs.
    calculate_dir_sizes(&mut root_node);
    
    // Sort children by size
    sort_tree(&mut root_node);
    
    Ok((root_node, report))
}

fn is_permission_denied(e: &jwalk::Error) -> bool {
    use std::io::ErrorKind;
    e.io_error()
        .is_some_and(|io| io.kind() == ErrorKind::PermissionDenied)
}

fn ensure_dir_path(root: &mut Node, path: &Path) {
    let mut current = root;
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy().to_string();
        let existing_idx = current.children.iter().position(|c| c.name == name);
        let idx = match existing_idx {
            Some(i) => i,
            None => {
                current.children.push(Node::new(
                    name.clone(),
                    current.path.join(&name),
                    true,
                ));
                current.children.len() - 1
            }
        };
        current = &mut current.children[idx];
        current.is_dir = true;
    }
}

fn add_file_to_tree(root: &mut Node, path: &Path, size: u64) {
    let mut current = root;
    let mut components = path.components().peekable();

    while let Some(component) = components.next() {
        let name = component.as_os_str().to_string_lossy().to_string();
        let is_leaf = components.peek().is_none();

        let existing_idx = current.children.iter().position(|c| c.name == name);
        let idx = match existing_idx {
            Some(i) => i,
            None => {
                current.children.push(Node::new(
                    name.clone(),
                    current.path.join(&name),
                    !is_leaf, // dirs for intermediate components
                ));
                current.children.len() - 1
            }
        };

        current = &mut current.children[idx];

        if is_leaf {
            current.is_dir = false;
            current.size = current.size.saturating_add(size);
        } else {
            current.is_dir = true;
        }
    }
}

fn calculate_dir_sizes(node: &mut Node) -> u64 {
    if !node.is_dir {
        return node.size;
    }

    let mut total = 0u64;
    for child in &mut node.children {
        total = total.saturating_add(calculate_dir_sizes(child));
    }
    node.size = total;
    total
}

fn sort_tree(node: &mut Node) {
    node.children.sort();
    for child in &mut node.children {
        sort_tree(child);
    }
}


















// ============================================================================
// SCAN STATE (For Frontend Polling)
// ============================================================================

/// Represents the current state of a scan operation.
/// 
/// Frontends (TUI/GUI) can poll this to update their UI accordingly.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanState {
    /// No scan is currently running
    Idle,
    /// Scan is in progress with current statistics
    Scanning {
        files_scanned: u64,
        current_path: Option<PathBuf>,
    },
    /// Scan completed successfully with results
    Done {
        root: Node,
        report: ScanReport,
    },
    /// Scan failed with error message
    Error(String),
}

impl Default for ScanState {
    fn default() -> Self {
        Self::Idle
    }
}

// ============================================================================
// SCANNER API (Primary Interface)
// ============================================================================

/// High-performance disk usage scanner
/// 
/// This is the main interface for scanning directories. Use this instead of
/// the lower-level `scan_directory` functions for better encapsulation.
/// 
/// # Multi-Frontend Architecture
/// 
/// This Scanner is designed to be used by multiple frontends (TUI, GUI, etc.).
/// It provides both blocking and progress-based scanning methods.
#[derive(Debug, Default)]
pub struct Scanner {
    // TODO: Future: Add configuration options here (filters, exclusions, etc.)
}

impl Scanner {
    /// Create a new Scanner instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Scan a directory and return the root node with all children
    /// 
    /// # Arguments
    /// * `path` - The directory path to scan
    /// 
    /// # Returns
    /// * `Ok(Node)` - The root node containing the entire tree
    /// * `Err(anyhow::Error)` - If scanning fails
    /// 
    /// # Example
    /// ```no_run
    /// use ferris_scan::Scanner;
    /// use std::path::Path;
    /// 
    /// let scanner = Scanner::new();
    /// let result = scanner.scan(Path::new("C:/")).unwrap();
    /// println!("Total size: {} bytes", result.size);
    /// ```
    pub fn scan<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<Node> {
        let (root, _report) = scan_directory_with_report_shared(path, None, None)?;
        Ok(root)
    }

    /// Scan with progress reporting
    pub fn scan_with_progress<P: AsRef<Path>>(
        &self,
        path: P,
        shared_progress: Arc<SharedProgress>,
    ) -> anyhow::Result<(Node, ScanReport)> {
        scan_directory_with_report_shared(path, None, Some(shared_progress))
    }











    // ========================================================================
    // PRO FEATURE: Data Export
    // ========================================================================
    // This method is only compiled when the 'pro' feature is enabled.
    // In the free version, this method does not exist.
    // ========================================================================

    /// Export scan results to CSV format (Pro feature only)
    /// 
    /// This function is only available when compiled with `--features pro`.
    /// 
    /// # Arguments
    /// * `root` - The root node to export
    /// * `output_path` - Path where the CSV file will be written
    /// 
    /// # Returns
    /// * `Ok(())` - If export succeeds
    /// * `Err(anyhow::Error)` - If export fails
    /// 
    /// # Pro Feature
    /// This method is only available in the Pro version.
    /// 
    /// # Example
    /// ```no_run
    /// # #[cfg(feature = "pro")]
    /// # {
    /// use ferris_scan::Scanner;
    /// use std::path::Path;
    /// 
    /// let scanner = Scanner::new();
    /// let result = scanner.scan(Path::new("C:/")).unwrap();
    /// scanner.export_csv(&result, Path::new("output.csv")).unwrap();
    /// # }
    /// ```
    #[cfg(feature = "pro")]
    pub fn export_csv<P: AsRef<Path>>(&self, root: &Node, output_path: P) -> anyhow::Result<()> {
        use std::fs::File;

        let file = File::create(output_path)?;
        let mut writer = csv::Writer::from_writer(file);

        // Write header
        writer.write_record(["Path", "Name", "Type", "Size (bytes)"])?;

        // Flatten the tree and write each node
        self.write_node_csv(&mut writer, root, &PathBuf::new())?;

        writer.flush()?;
        Ok(())
    }

    #[cfg(feature = "pro")]
    fn write_node_csv(
        &self,
        writer: &mut csv::Writer<std::fs::File>,
        node: &Node,
        parent_path: &Path,
    ) -> anyhow::Result<()> {
        let current_path = parent_path.join(&node.name);
        let node_type = if node.is_dir { "Directory" } else { "File" };

        writer.write_record(&[
            current_path.display().to_string(),
            node.name.clone(),
            node_type.to_string(),
            node.size.to_string(),
        ])?;

        // Recursively write children
        for child in &node.children {
            self.write_node_csv(writer, child, &current_path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_scan_empty_directory() {
        let dir = tempdir().unwrap();
        let (tx, _rx) = mpsc::channel();
        let result = scan_directory(dir.path(), Some(tx));
        assert!(result.is_ok());
    }

    #[test]
    fn test_scanner_api() {
        let dir = tempdir().unwrap();
        let scanner = Scanner::new();
        let result = scanner.scan(dir.path());
        assert!(result.is_ok());
    }

    #[cfg(feature = "pro")]
    #[test]
    fn test_csv_export() {
        let dir = tempdir().unwrap();
        let scanner = Scanner::new();
        let result = scanner.scan(dir.path()).unwrap();
        
        let output_path = dir.path().join("export.csv");
        let export_result = scanner.export_csv(&result, &output_path);
        assert!(export_result.is_ok());
        assert!(output_path.exists());
    }
}
