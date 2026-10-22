# Performance

Measured on the development laptop: 14 cores, Intel Meteor Lake graphics, one NVMe drive, btrfs with zstd compression, warm page cache unless stated. Cold-cache numbers need root to drop caches and have not been taken.

## Budgets

Checked in the change checklist. A change that moves one says so in the commit body.

| Measure | Budget | 0.1.0 | How to measure |
|---------|--------|-------|----------------|
| Stripped release binary | 10 MB | 9.1 MB (plain build; the diagnostics build is 9.3 MB) | `ls -l target/release/oxtree` |
| Scan of /usr, 352k nodes, warm, on mains | 150 ms | 125 ms (250 ms on battery with the powersave governor) | `target/release/oxtree-bench /usr` from a `--features diagnostics` build |
| Treemap layout plus render at 1900x600 | 15 ms | 1.7 + 7.7 ms | same |
| First frame, performance power profile | 50 ms | 34 ms (86 to 90 ms in the balanced profile on battery) | `target/release/oxtree /usr 2>&1 \| grep 'first frame'`; stages with `OXTREE_STARTUP=1`; check `/sys/firmware/acpi/platform_profile` first |
| RSS with /usr loaded and the window open | 120 MB | 133 MB (115 MB with `MALLOC_ARENA_MAX=2`) | `grep -E '^(Rss\|Pss\|Anonymous)' /proc/$(pgrep -n -x oxtree)/smaps_rollup` |
| Tree memory per node | 56 bytes | about 55 bytes | arithmetic on the arena arrays, see below |

RSS is over budget in 0.1.0. See the memory section for where it goes and the planned fix.

## Where the time goes

| Phase | 0.1.0 | Note |
|-------|-------|------|
| Scan, 14 workers | 125 ms | Was 716 ms until the progress loop stopped sleeping through the finish; see gotchas |
| Scan, 1 worker (spike) | 400 ms | 83% kernel time: the walk is syscall bound and scales with cores |
| Layout | 1.7 ms | Children are pre-sorted, so this is arithmetic only; the spike sorted and took 5 ms |
| Render | 7.7 ms | 14 bands on 14 threads; the spike took 21 ms single-threaded |
| Texture upload | about 1 ms | |
| Cache write / parse, 352k nodes | 89 ms / 111 ms | Text format, 14.8 MB |
| Search "lib >1k" over 357k nodes, by name | 18 ms | Substring compare in place on ASCII names, no allocation per node; `--bench` prints it |
| Search, the same query by path | 95 ms | Builds a path string per node; 816k nodes in a home directory take 220 to 260 ms. The list is built on a worker thread, so typing never waits: a keystroke during a build marks the list dirty and one more build follows |
| File Types table, 357k nodes | 10 ms | One hash map by extension; built on the UI thread when the tab is first shown |
| Duplicate counts by name and size | 49 ms | Two passes over a hash map; built on a thread when the mode is chosen |
| CSV export of /usr | 166 ms | 35 MB of text, one path string per node; on the UI thread, once per export |
| First frame | 75 ms | eframe on glow under Wayland; the spike measured 43 ms without menus and panels |

Installed QDirStat 2.0 for comparison: read of /usr logged at 41.6 s, RSS 97 MB after that scan, 1.8 MB binary plus five Qt6 libraries.

## Where the memory goes

With /usr loaded (352 101 nodes) and the window open:

| Mapping | RSS |
|---------|-----|
| Anonymous | 84 MB |
| libLLVM and libgallium (Mesa, shared with every GL process) | 37 MB |
| The binary | 8 MB |
| libc, libstdc++, xkbcommon, X11, EGL | 5 MB |

The arena itself is small: per node 4 (parent) + 4 (first child) + 4 (count) + 8 (size) + 8 (mtime) + 4 (mode) + 1 (kind) + 12 (items, files, subdirs) + 4 (name offset) = 49 bytes, plus about 19 bytes of name, about 68 bytes per node or 24 MB for /usr. The rest of the anonymous memory is what glibc kept after the scan: every worker thread allocated hundreds of thousands of small `String`s for names that were freed on the main thread after `Tree::build`, and glibc's per-thread arenas do not hand that back. Running with `MALLOC_ARENA_MAX=2` cuts anonymous memory from 84 MB to 65 MB with no code change, which confirms the diagnosis.

Planned fix, in order:

1. Per-worker name buffers. `DirRecord` carries one `String` per directory holding all its names and `FileRecord` holds offsets, so a worker makes one allocation per directory instead of one per file. Expected to remove most of the retained memory and to speed the scan up as well.
2. Drop `items`, `files` and `subdirs` for non-directory nodes by moving them to a directory-only side table indexed through the node. Saves about 10 bytes per node.
3. Only then consider the allocator. `mallopt` needs `unsafe`, which the workspace forbids, so the lever would be the environment variable in the desktop file.

The Mesa share is not ours to cut. PSS is the honest number for it; on a desktop that already runs a GL compositor those pages are resident anyway.

## Scan design

Facts behind the walker, so nobody re-derives them:

- The kernel's dentry and inode caches scale across cores for lookups; readdir on one directory serialises on that directory's inode lock. Parallelism is therefore per directory, and the queue has to hand out whole directories.
- std's `DirEntry::metadata` on Linux already stats relative to the directory fd with `AT_SYMLINK_NOFOLLOW`, and `DirEntry::file_type` reads `d_type`. Hand-rolled getdents64 and statx gain nothing; the spacebadger project measured a 64 KiB getdents buffer within noise and io_uring statx 1.5 to 3x slower warm, because the kernel sends statx to io-wq threads.
- Sorting entries by inode number before stat helps ext4 on rotating disks (Ts'o's advice, adopted by gnulib's fts) and nothing else. Not implemented; conditional on `ROTA` from sysfs if anyone with an HDD asks.
- dua-cli replaced jwalk and rayon with a crate-local work-stealing walker on std threads for up to 40% more throughput. OxTree's shared `Mutex<Vec>` queue is the simple version of that; a per-worker deque is the next step if the lock shows up in a profile.
- Hard links: each link of a regular file carries `blocks * 512 / nlink`, so the totals count the file once and the result does not depend on which worker met which link first. A `(dev, ino)` set would give the first link the full size but make the tree differ between runs.

## Treemap design

- Layout is a traversal because the arena stores children largest first. One scratch `Vec` is reused for rows.
- Shading splits the pixel rows into one band per thread with `chunks_mut`, each thread walking every tile and clipping to its band. No unsafe, no shared mutation, near-linear scaling.
- The GPU route (a vertex buffer of rectangles plus cushion coefficients, shading in a fragment shader through a glow paint callback) would make a resize cost layout plus an upload at any window size and remove the texture. Not needed at 7.7 ms for a 1900x600 map; worth it for 4K.
- During a resize the treemap is rebuilt every frame, one job in flight at a time; the previous texture is drawn at its own pixel size, anchored top left and clipped, never stretched. Measured on a five second resize drag with `OXTREE_STATS=1`: about 55 rebuilds a second at 110 to 135% of one core, idle unaffected. A QDirStat-style settle delay was tried and measured at one rebuild a second and 5 to 9%, and rejected: a drag lasts a second or two, and the per-frame rebuild is what makes the speed visible. `[Treemaps] RebuildDelayMillisec=200` in the settings file, or the Preferences dialog, turns the delay on. Rebuilds are capped at 120 a second (`TREEMAP_MAX_REBUILD_HZ`) so a 500 Hz display cannot multiply the cost; 120 rather than 60 because a 60 cap skips every other frame on a 60 Hz screen whenever the timer lands early.

## Launch

Where the time before the first frame goes, measured 2026-10-22 with `OXTREE_STARTUP=1` over several runs on this laptop (Intel MTL graphics, Mesa 26.2.3, KDE Wayland, 60 Hz) in the balanced power profile on battery. In that profile the first frame lands at 82 to 109 ms after `main`; in the performance profile the same binary lands at 34 ms, and the whole table scales with it. The bare eframe window of the spike, with no menus or panels, measured the same as OxTree in both, so almost all of it is window setup, not OxTree.

| Stage | ms | What it is |
|-------|---:|------------|
| exec, dynamic loader, static init | 3 to 4 | Five shared objects, under 300 relocations; `--help` round-trips in 2 ms |
| Wayland connection, registry roundtrip, globals bound | 10 to 17 | winit's event loop and the smithay client toolkit |
| EGL display init and config choice | 39 to 48 | `dlopen` of libEGL_mesa pulls in libgallium and libLLVM, 210 MB of mapped library; a bare `dlopen` of that one library, timed on its own outside the app, takes 26 ms. The largest single item and not ours |
| Wayland window, egui-winit state, clipboard | 4 to 7 | xdg surface, decorations, data-control clipboard |
| EGL surface, make current, swap interval | 2 to 3 | |
| egui_glow painter: shader compile, link, VAO | 8 to 10 | Mesa's shader cache makes no measurable difference, so this is the cost even when cached |
| `OxTreeApp::new` | 2 to 3 | cleanups file, `date +%z` as a child process, category table, scan thread start |
| event loop warm-up to the first update | 3 to 14 | |
| first update: menus, panels, views, font atlas | 3 to 9 | |
| first frame on screen | after that | tessellate, upload, draw, swap, then the window is mapped and the compositor presents it; the second update arrives 36 to 46 ms later, two to three vblanks |

Experiments that did not move it: `LIBGL_ALWAYS_SOFTWARE=1` (same, llvmpipe loads the same LLVM), `MESA_SHADER_CACHE_DISABLE=true` (same), the X11 backend (105 to 119 ms, slower).

What was shaved, and what the measurements said:

1. **Starting the scan at `main` entry, before the window.** Built, measured, and left off by default. It is not a free overlap: the walk is syscall-bound on every core, and the single-threaded window setup slows down while it runs. Measured on battery (powersave governor, where a scan of /usr takes 250 ms instead of 110 ms on mains), three runs each:

   | Scan start | Threads | First frame | Tree on screen |
   |---|---:|---:|---:|
   | after the window (before) | 14 | 85 to 97 ms | 365 to 378 ms |
   | before the window | 14 | 110 to 167 ms | 256 to 319 ms |
   | before the window | 7 | 97 to 120 ms | 310 to 335 ms |
   | before the window | 4 | 96 to 122 ms | 416 to 424 ms |

   The tree arrives about 90 ms earlier and the empty window about 40 ms later, and on a home directory scan the user's own log showed the EGL init stretched from 39 to 65 ms beside the walk. The window comes first. The early start and the thread-count switch used for this table were removed once it was settled; `begin_early` in `app.rs` is where an early scan would go if the question is ever reopened.
2. **The scan threads wake the UI.** Done. Every scan event calls back into egui's `request_repaint`, so a finished scan is drawn on the next frame instead of on the next 100 ms poll. Same for later rescans.
3. **`date +%z` and the cleanups file run on a thread started at `main` entry.** Done. The constructor's own cost went from 2 to 3 ms to 0.1 to 0.2 ms.
4. **Load Mesa on a thread at `main` entry.** Not done. A `dlopen` of libEGL_mesa while winit talks to the compositor; glutin then finds it mapped. Expected 15 to 25 ms off the first frame, less now that the scan competes for the cores during setup. Needs the `libloading` crate since the workspace forbids `unsafe`.
5. **wgpu over Vulkan instead of glow** would avoid libgallium and libLLVM entirely, since the Intel Vulkan driver does not link LLVM. Whether wgpu's own instance, adapter and shader setup is cheaper is an experiment, not a known: build the spike with the wgpu feature and compare. Not done.

Measure in the performance power profile (`/sys/firmware/acpi/platform_profile`), docked. In the balanced profile on battery this laptop takes 2.5 times as long to the first frame (86 to 90 ms against 34) and more than twice as long to scan (250 ms against 110 for /usr); every number in this file moves with the profile, so note it next to any number you add.

The release profile is `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.

## Diagnostic switches

Environment variables and flags read at startup, for measuring and for bisecting problems. None of them is a user setting, and none of them exists in a plain release build: they are compiled in with `cargo build --release --features diagnostics`, which also builds the `oxtree-bench` binary. A plain build answers `--screenshot` with one line saying it needs that feature.

| Switch | Effect |
|--------|--------|
| `OXTREE_STATS=1` | Prints treemap rebuild counts and process CPU time once a second to stderr |
| `OXTREE_STARTUP=1` | Prints the startup stages, about ten lines, each with the time since the previous one; `OXTREE_T0=$(date +%s%N)` on the same command line adds the exec and loader time |
| `OXTREE_STARTUP=2` | The stages plus every log record from eframe, winit, glutin and the Wayland client that followed a gap of a quarter millisecond or more |
| `OXTREE_STARTUP=/path/to/file` | Everything, some 2 500 lines, written to that file instead of the terminal |
| `--screenshot FILE.ppm` | Writes the first full frame and prints its minimum alpha and translucent pixel count to stderr; must say `min 255` |
| `oxtree-bench PATH` | Scan, layout, render, cache, types, duplicates, CSV and search timings for one tree, to stdout; `OXTREE_CROSS_FILESYSTEMS=0` keeps the scan on one filesystem |

## Dead ends

Recorded in `gotchas.md` under the spike date, with the sources in the spike's own notes. Short version: io_uring statx, custom directory syscalls, reading filesystem metadata tables the way WizTree reads the MFT, allocator swaps without a measurement, and a software renderer to shed Mesa.
