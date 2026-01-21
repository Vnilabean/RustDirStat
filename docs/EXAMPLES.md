# ferris-scan Usage Examples

## Quick Start

### 1. Build the Free Version
```bash
cargo build --release
cd target/release
```

### 2. Run a Scan
```bash
# Scan current directory
./ferris-scan

# Scan specific path
./ferris-scan "C:\Users"

# Scan entire drive (requires admin for some folders)
./ferris-scan "C:\"
```

---

## Interactive Controls

### During Scan
- **Q / Esc**: Quit application (will terminate scan)

### After Scan Completes
- **E**: Export results to CSV
  - **Free Version**: Shows "This is a Pro Feature" popup
  - **Pro Version**: Exports to `ferris-scan-export.csv`
- **Q / Esc**: Quit application
- **Any Key**: Close popup messages

---

## Feature Flag Demonstration

### Free Version Behavior

```bash
cargo build --release
./target/release/ferris-scan "C:\Users\YourName\Documents"
```

**What you'll see:**
1. TUI launches with header showing `[FREE]` tag
2. Real-time scanning progress with file count
3. After scan: Sorted list of directories by size
4. Press `E`: Popup appears:
   ```
   ⚠ This is a Pro Feature

   CSV Export is only available in ferris-scan Pro.

   Build with: cargo build --release --features pro
   ```
5. Press any key to close popup

### Pro Version Behavior

```bash
cargo build --release --features pro
./target/release/ferris-scan "C:\Users\YourName\Documents"
```

**What you'll see:**
1. TUI launches with header showing `[PRO]` tag
2. Real-time scanning progress with file count
3. After scan: Sorted list of directories by size
4. Press `E`: Popup appears:
   ```
   ✓ Export successful!

   Saved to:
   C:\Users\YourName\ferris-scan-export.csv
   ```
5. CSV file is created with structured data

---

## CSV Export Format (Pro Only)

When you export in Pro version, the CSV contains:

```csv
Path,Name,Type,Size (bytes)
Documents,Documents,Directory,52428800000
Documents\Photos,Photos,Directory,25600000000
Documents\Photos\2024,2024,Directory,12800000000
Documents\Photos\2024\vacation.jpg,vacation.jpg,File,5242880
...
```

**Structure:**
- **Path**: Full relative path from scan root
- **Name**: File or directory name
- **Type**: "File" or "Directory"
- **Size (bytes)**: Raw byte count

**Use Cases:**
- Import into Excel/Google Sheets for analysis
- Process with scripts (Python/PowerShell)
- Track disk usage over time
- Generate custom reports

---

## Verifying Your Build

### Check Binary Size
```bash
# Free version
cargo build --release
ls -lh target/release/ferris-scan.exe
# ~2.5 MB

# Pro version
cargo build --release --features pro
ls -lh target/release/ferris-scan.exe
# ~2.7 MB
```

### Run Tests
```bash
# Free version tests
cargo test
# Should pass 2 tests

# Pro version tests
cargo test --features pro
# Should pass 3 tests (includes test_csv_export)
```

### Verify Compilation
```bash
# Check free version compiles
cargo check

# Check Pro version compiles
cargo check --features pro
```

---

## Development Workflow

### Adding New Pro Features

1. **Define the feature in lib.rs:**
```rust
#[cfg(feature = "pro")]
pub fn new_pro_feature(&self) -> Result<()> {
    // Your implementation
    Ok(())
}
```

2. **Add UI handler in main.rs:**
```rust
KeyCode::Char('n') => {
    #[cfg(feature = "pro")]
    {
        // Call the pro feature
        scanner.new_pro_feature()?;
    }
    
    #[cfg(not(feature = "pro"))]
    {
        app.show_popup("This is a Pro Feature");
    }
}
```

3. **Add tests:**
```rust
#[cfg(feature = "pro")]
#[test]
fn test_new_pro_feature() {
    let scanner = Scanner::new();
    let result = scanner.new_pro_feature();
    assert!(result.is_ok());
}
```

4. **Test both versions:**
```bash
cargo test
cargo test --features pro
```

---

## Performance Optimization

### For Large Scans
```bash
# Increase thread count (if needed)
export RAYON_NUM_THREADS=16
./ferris-scan "C:\"

# Release mode is crucial for performance
cargo build --release  # NOT cargo build
```

### For Systems with Many Files
The scanner uses `jwalk` which automatically parallelizes I/O. On systems with:
- **NVMe SSDs**: Expect ~100k files/second
- **SATA SSDs**: Expect ~50k files/second
- **HDDs**: Expect ~10k files/second

---

## Troubleshooting

### "Permission Denied" Errors
These are normal on Windows for:
- `C:\System Volume Information`
- `C:\$Recycle.Bin`
- `C:\Windows\CSC`

**Solution:** Run as Administrator, or the scanner will skip these (reported at end).

### Scan Takes Too Long
**Possible causes:**
1. Using Debug build instead of Release
   - **Fix:** `cargo build --release`
2. Scanning network drive
   - **Fix:** Network I/O is inherently slow; this is expected
3. Antivirus scanning each file access
   - **Fix:** Temporarily disable AV or add exception

### UI Appears Frozen
The UI polls at 50ms intervals. During very fast scans (< 1 second), you might see:
1. Scanning state briefly
2. Immediate transition to results

**This is normal:** The scan was simply too fast to see intermediate updates.

---

## Integration Examples

### PowerShell Script (Pro Version)
```powershell
# Weekly disk usage report
$date = Get-Date -Format "yyyy-MM-dd"
.\ferris-scan.exe "C:\Users"
# Press 'E' to export
Rename-Item "ferris-scan-export.csv" "disk-usage-$date.csv"

# Email the report
Send-MailMessage -To "admin@company.com" `
                 -Subject "Weekly Disk Usage Report" `
                 -Attachments "disk-usage-$date.csv"
```

### Python Analysis (Pro Version)
```python
import pandas as pd

# Load exported CSV
df = pd.read_csv('ferris-scan-export.csv')

# Find top 10 largest directories
dirs = df[df['Type'] == 'Directory']
top_dirs = dirs.nlargest(10, 'Size (bytes)')

print("Top 10 Largest Directories:")
for idx, row in top_dirs.iterrows():
    size_gb = row['Size (bytes)'] / (1024**3)
    print(f"{row['Name']}: {size_gb:.2f} GB")
```

---

## Build Recipes

### Production Free Binary
```bash
cargo build --release
strip target/release/ferris-scan.exe  # Remove debug symbols
upx target/release/ferris-scan.exe    # Optional: Compress binary
```

### Production Pro Binary
```bash
cargo build --release --features pro
strip target/release/ferris-scan.exe
# Code signing (Windows)
signtool sign /f cert.pfx /p password /t http://timestamp.digicert.com target/release/ferris-scan.exe
```

### Cross-Compilation (Advanced)
```bash
# Build for Windows from Linux
cargo build --release --target x86_64-pc-windows-gnu

# Build for Linux from Windows (requires WSL)
cargo build --release --target x86_64-unknown-linux-gnu
```

---

## Benchmarking Your System

```bash
# Scan and measure performance
time ./ferris-scan "C:\Users"

# Expected output:
# real    0m3.456s  (actual scan time)
# user    0m8.234s  (CPU time across all threads)
# sys     0m1.123s  (kernel I/O time)

# If user >> real: Good parallelization
# If real ≈ user: Not utilizing multiple cores (unexpected)
```

---

## Next Steps

1. **Try both versions:**
   - Build Free: Experience the upgrade prompt
   - Build Pro: Test CSV export functionality

2. **Scan various paths:**
   - Small: `Documents` folder (~1 GB)
   - Medium: User home directory (~50 GB)
   - Large: Entire drive (~500 GB)

3. **Integrate into workflow:**
   - Add to scheduled tasks for regular audits
   - Use Pro CSV exports for trend analysis
   - Share Free version with colleagues

4. **Contribute:**
   - Report bugs via GitHub Issues
   - Suggest Pro features for future releases
   - Submit pull requests for improvements

---

## Support

- **Documentation:** See `README.md` and `ARCHITECTURE.md`
- **Issues:** GitHub Issues tab
- **Source Code:** Fully available under MIT License
- **Pro License:** Contact for commercial licensing options
