# Core + Multi-Frontend Architecture

## Overview

ferris-scan follows a **"Core + Multi-Frontend"** architecture pattern, separating business logic from presentation. This enables us to maintain a single, well-tested core library while supporting multiple user interfaces.

```
┌─────────────────────────────────────────────────────┐
│                   src/lib.rs                        │
│              (Core Business Logic)                  │
│                                                     │
│  - Scanner: High-performance disk scanning          │
│  - Node: File tree data structures                  │
│  - ScanState: State management for UIs              │
│  - SharedProgress: Thread-safe progress tracking    │
│  - Pro Features: CSV export (feature-gated)         │
└──────────────┬─────────────┬────────────────────────┘
               │             │
       ┌───────┴────┐   ┌────┴────────┐
       │            │   │             │
       ▼            ▼   ▼             ▼
┌──────────┐  ┌──────────┐  ┌──────────────┐
│ TUI      │  │ GUI      │  │  Future:     │
│          │  │          │  │  - Web UI    │
│ ratatui  │  │ eframe   │  │  - REST API  │
│ crossterm│  │ egui     │  │  - CLI only  │
└──────────┘  └──────────┘  └──────────────┘
```

## Architecture Benefits

### 1. **Separation of Concerns**
- **Core Library (`lib.rs`):** Contains all business logic, data structures, and algorithms
- **Binary Targets (`bin/*.rs`):** Thin wrappers that handle UI/presentation only
- Each frontend is independent and can evolve separately

### 2. **Code Reusability**
- The `Scanner` is implemented once and used by all frontends
- Data structures (`Node`, `ScanReport`) are shared
- Progress tracking is frontend-agnostic

### 3. **Testability**
- Core logic can be unit tested without UI dependencies
- Each frontend can be tested independently
- Integration tests verify the library API

### 4. **Flexibility**
- Easy to add new frontends (Web UI, CLI-only, REST API)
- Frontends can be compiled independently
- Different frontends for different use cases

## File Structure

```
src/
├── lib.rs              # Core library (Scanner, Node, FileTree)
├── main.rs             # Deprecated (kept for compatibility)
└── bin/
    ├── tui.rs          # Terminal User Interface (ratatui)
    └── gui.rs          # Graphical User Interface (eframe/egui)
```

## Building and Running

### TUI (Terminal Interface)

```bash
# Free version
cargo build --release --bin ferris-scan-tui
cargo run --bin ferris-scan-tui -- /path/to/scan

# Pro version (with CSV export)
cargo build --release --features pro --bin ferris-scan-tui
cargo run --features pro --bin ferris-scan-tui -- /path/to/scan
```

### GUI (Graphical Interface)

```bash
# Free version
cargo build --release --bin ferris-scan-gui
cargo run --bin ferris-scan-gui -- /path/to/scan

# Pro version (with CSV export)
cargo build --release --features pro --bin ferris-scan-gui
cargo run --features pro --bin ferris-scan-gui -- /path/to/scan
```

### Build All Targets

```bash
# Build everything
cargo build --release

# Build everything with Pro features
cargo build --release --features pro
```

## Core Library API

### Scanner

The main interface for scanning directories:

```rust
use ferris_scan::{Scanner, SharedProgress};
use std::sync::Arc;

// Simple blocking scan
let scanner = Scanner::new();
let result = scanner.scan("/path/to/scan")?;
println!("Total size: {} bytes", result.size);

// Scan with progress tracking
let progress = Arc::new(SharedProgress::default());
let (root, report) = scanner.scan_with_progress("/path/to/scan", progress)?;

// Access progress from UI thread
let files = progress.files_scanned.load(Ordering::Relaxed);
let path = progress.last_path.lock().unwrap();
```

### Data Structures

#### Node
```rust
pub struct Node {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<Node>,
    pub path: PathBuf,
}
```

#### ScanState
```rust
pub enum ScanState {
    Idle,
    Scanning {
        files_scanned: u64,
        current_path: Option<PathBuf>,
    },
    Done {
        root: Node,
        report: ScanReport,
    },
    Error(String),
}
```

#### SharedProgress
```rust
pub struct SharedProgress {
    pub files_scanned: AtomicU64,
    pub last_path: Mutex<Option<PathBuf>>,
}
```

## Frontend Guidelines

When implementing a new frontend:

1. **Use the Scanner API** - Don't reimplement scanning logic
2. **Poll SharedProgress** - For real-time progress updates
3. **Handle ScanState** - Map core states to your UI states
4. **Run scans in background threads** - Keep UI responsive
5. **Feature-gate Pro functionality** - Use `#[cfg(feature = "pro")]`

### Example: Minimal Frontend

```rust
use ferris_scan::{Scanner, SharedProgress};
use std::sync::Arc;
use std::thread;

fn main() {
    let scanner = Scanner::new();
    let progress = Arc::new(SharedProgress::default());
    let progress_clone = Arc::clone(&progress);

    // Spawn background scan
    let handle = thread::spawn(move || {
        scanner.scan_with_progress(".", progress_clone)
    });

    // Poll progress (your UI loop)
    loop {
        let files = progress.files_scanned.load(Ordering::Relaxed);
        println!("Scanned {} files", files);
        
        if handle.is_finished() {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    // Get results
    let (root, report) = handle.join().unwrap()?;
    println!("Done! Total: {} bytes", root.size);
}
```

## Feature Flags

### `pro` Feature

Enables professional features (CSV export):

```rust
#[cfg(feature = "pro")]
{
    scanner.export_csv(&root, "output.csv")?;
}

#[cfg(not(feature = "pro"))]
{
    println!("CSV export requires Pro version");
}
```

## Future Frontends

The architecture makes it easy to add:

- **Web UI:** Serve the Scanner via WebAssembly or REST API
- **CLI-only:** Non-interactive command-line tool
- **REST API:** HTTP endpoints for remote scanning
- **Desktop Native:** Using native GUI frameworks (GTK, Qt)

## Testing

```bash
# Test core library
cargo test

# Test specific binary
cargo test --bin ferris-scan-tui

# Test with Pro features
cargo test --features pro
```

## Dependencies

### Core Library
- `jwalk` - Fast directory traversal
- `rayon` - Parallel processing
- `anyhow` - Error handling
- `serde`, `csv` - Pro features (optional)

### TUI Binary
- `ratatui` - Terminal UI framework
- `crossterm` - Terminal control

### GUI Binary
- `eframe` - egui framework wrapper
- Native windowing support

## Performance Notes

- The Scanner uses parallel processing via `jwalk`
- Progress updates are throttled (100-200ms) to avoid overhead
- UIs should render at 30-60 FPS for responsiveness
- Lock contention is minimized using atomic operations

## Contributing

When adding features:

1. **Core logic goes in `lib.rs`** - Not in frontend code
2. **Keep frontends thin** - They should only handle presentation
3. **Test the library API** - Add unit tests for core functionality
4. **Document public APIs** - Use rustdoc comments
5. **Consider all frontends** - Changes should work for TUI and GUI

## Migration from Old Architecture

If you have code using the old `main.rs`:

```rust
// Old (deprecated)
mod old_code;
use old_code::*;

// New (Core + Multi-Frontend)
use ferris_scan::{Scanner, Node, ScanReport};
```

The old `main.rs` is kept for compatibility but will be removed in v0.2.0.
