# ferris-scan

A high-performance disk usage analyzer for Windows, written in Rust. Designed as a lightweight, multi-threaded alternative to WinDirStat and TreeSize.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)]()
[![Platform](https://img.shields.io/badge/platform-Windows-gray)](https://www.microsoft.com/windows)
[![Sponsor](https://img.shields.io/badge/Sponsor-GitHub-pink)](https://github.com/sponsors/vnilabean)

[![Buy Me a Coffee](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/vnilabean)

---

## Overview

`ferris-scan` addresses the performance bottlenecks found in legacy disk analysis tools. By leveraging modern parallel filesystem traversal (`jwalk`) and a thread-safe architecture (`rayon`), it aims to saturate I/O bandwidth on NVMe drives while maintaining a minimal memory footprint (<100MB RAM).

**Key Capabilities:**
* **Parallel Traversal:** Multi-threaded scanning utilizing available CPU cores.
* **Memory Efficiency:** Optimized tree structures to handle millions of files with low overhead.
* **Terminal UI:** A keyboard-driven TUI built with `ratatui` for headless or low-resource environments.
* **No Runtime Dependencies:** Compiled native binary with no reliance on .NET or Electron.

![Demo](assets/demo.gif)

---

## Installation

### Option A: Pre-built Installer (Recommended)
For users who want a signed Windows installer with automatic updates.

[**Download Installer ($5)**](https://ferris-scan.lemonsqueezy.com)

*Proceeds support development and maintenance.*

### Option B: Build from Source
The entire project is open source under the MIT license. You can compile the full version (including export features) directly from this repository.

**Prerequisites:**
* Rust 1.75+ ([Install via Rustup](https://rustup.rs/))

**Build Instructions:**

```bash
git clone [https://github.com/vnilabean/ferris-scan.git](https://github.com/vnilabean/ferris-scan.git)
cd ferris-scan

# Build the release binary (includes pro features)
cargo build --release --features pro --bin ferris-scan-tui

# The binary is now available in target/release/
cd target/release

# Run the analyzer
./ferris-scan-tui