# The Rust Renaissance: A Manifesto for High-Performance Windows Utilities

> "Software is getting slower more rapidly than hardware becomes faster." — *Wirth's Law*

## 1. The High RAM Crisis

We are currently navigating a profound resource crisis in the Windows ecosystem. While hardware capabilities have expanded exponentially—with NVMe SSDs hitting 7,000 MB/s and 32GB RAM becoming standard—the perceived performance of our software has stalled or regressed.

A simple text editor that once consumed megabytes now frequently reserves hundreds of megabytes, carrying the weight of an entire web browser engine (Electron) within it. We have traded user resources for developer convenience, transitioning from native, compiled languages to managed runtimes and web-technologies-as-desktop-apps.

## 2. The Great Bifurcation: Abandonware vs. Bloatware

The market for system utilities—tools explicitly designed to optimize our computers—has split into two stagnation points:

* **The Abandonware:** Legendary tools from the Windows 7 era (like WinDirStat) have ceased meaningful development. They rely on legacy APIs (like `FindNextFile`), struggle with high-DPI displays, and fail to utilize the multi-core parallelism of modern CPUs.

* **The Bloatware:** Utilities that found commercial success were acquired and monetized aggressively. Tools that were once lightweight are now background-resident suites that push notifications and consume more resources than the "clutter" they claim to remove.

## 3. The Solution: Return to the Metal

`ferris-scan` represents a rejection of this status quo. I believe the Windows utility landscape is ripe for a comprehensive architectural overhaul using **Rust**.

Rust offers a unique value proposition for systems programming: it combines the low-level memory control of C++ with compile-time guarantees of memory safety and thread safety.

### Our Architectural Pillars

#### I. Physical over Logical (The MFT Revolution)

Legacy tools use "Logical" APIs (`FindFirstFile`), which require expensive user-to-kernel mode transitions for every single file. On a drive with millions of files, this causes the UI to hang.

We use **Physical** access. By reading the raw **NTFS Master File Table ($MFT)** directly, we bypass the filesystem drivers entirely. This allows us to parse a 1TB drive in seconds rather than minutes, treating the disk as a flat database rather than a recursive tree.

#### II. Fearless Concurrency

Modern CPUs have dozens of cores, yet most legacy utilities are single-threaded. Rust's ownership model allows us to use libraries like `rayon` to parallelize scanning without fear of race conditions. We saturate the NVMe bandwidth, not the CPU single-thread performance.

#### III. Zero-Cost Abstractions via `windows-rs`

We bypass heavy frameworks. By leveraging the `windows` crate, we interact directly with the Win32 API and NT Kernel interfaces. This results in binaries that are tiny (<5MB), start instantly, and respect your hardware.

## 4. The Mission

`ferris-scan` is more than a disk usage analyzer; it is a proof of concept for a new era of Windows software. I'm proving that you don't need a browser engine to build a UI, and you don't need to sacrifice safety to get C++ performance.

**Join the Renaissance.**
