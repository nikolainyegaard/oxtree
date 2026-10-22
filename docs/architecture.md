# Architecture

OxTree is a Cargo workspace with four layers. Dependencies point downwards only; nothing below the binary opens a window.

```
apps/oxtree          the binary: arguments, the eframe window, --files, --types, --search, the headless --export; with --features diagnostics also diagnostics.rs (startup timing, --screenshot) and the oxtree-bench binary
crates/ui            egui views, theme tokens, application state (oxtree-ui)
crates/native        filesystem walk, child processes, config paths, cache files on disk (oxtree-native)
crates/core          pure logic, no I/O: tree arena, layout, shading, categories, cleanups, formats (oxtree-core)
```

## File tree

```
Cargo.toml                 workspace, shared lints, release profile
clippy.toml                unwrap, expect, panic and indexing allowed in tests only
LICENSE                    GPL-2.0
NOTICE                     what was carried over from QDirStat
CHANGELOG.md
README.md
.github/workflows/
  ci.yml                   the gate on every push and pull request
  release.yml              on a version tag: x86_64 and aarch64 builds against glibc 2.28, tarball, .deb, .rpm, AppImage, the GitHub release
packaging/
  com.github.nikolainyegaard.oxtree.desktop     desktop entry
  com.github.nikolainyegaard.oxtree.svg         the icon
  com.github.nikolainyegaard.oxtree.metainfo.xml  AppStream metadata
  com.github.nikolainyegaard.oxtree.policy      polkit action for Rescan as Root
  oxtree-64.rgba           the window icon, raw pixels, compiled into the binary
docs/
  architecture.md          this file
  design.md                colours, sizes, UI structure and copy
  cleanups.md              cleanup configuration and placeholders
  performance.md           measurements, budgets and the optimisation plan
  gotchas.md               dated decisions and dead ends
crates/core/src/
  lib.rs
  tree.rs                  Tree arena: struct of arrays, children sorted by size, DirRecord input
  layout.rs                squarified layout with cushion coefficients, hit testing
  cushion.rs               per-pixel shading into an RGBA buffer across threads
  flat.rs                  the File View's list: every node sorted, filtered, capped at N
  types.rs                 the File Types table: files grouped by extension with category, count and size
  dupes.rs                 duplicate counts by name and size, or name, size and date
  export.rs                CSV and TSV in WizTree's columns, the indented clipboard text
  query.rs                 WizTree's search syntax: parser and matcher
  mime.rs                  file categories and QDirStat's default table
  glob.rs                  * and ? matching for category patterns
  cleanup.rs               Cleanup struct, placeholders, standard set, INI mapping
  ini.rs                   QSettings-compatible INI read and write
  settings.rs              the settings file's keys, defaults and fallbacks; release tag parsing
  cache.rs                 QDirStat cache file parse (1.0, 2.0) and write (1.0)
  format.rs                sizes, percentages, permissions, elapsed time
  time.rs                  unix time to date text with a fixed UTC offset
crates/core/tests/fixtures/
  qdirstat-cache-v2.txt    the documented example cache file
crates/native/src/
  lib.rs
  walker.rs                parallel scan on a shared queue, progress events, cancel
  run.rs                   shell -c with streamed stdout and stderr
  trash.rs                 gio trash
  dialog.rs                kdialog or zenity directory chooser
  config.rs                XDG config dir, settings and cleanups files, QDirStat fallback
  update.rs                the opt-in release check through curl
  cache_io.rs              cache files on disk, gzip through the system binary
  localtime.rs             date +%z once
  mounts.rs                /proc/self/mounts, df for volume sizes and free space, which filesystems a scan may cross into
  root.rs                  effective uid, second instance as root through pkexec
crates/ui/src/
  lib.rs
  app.rs                   OxTreeApp state, pump(), actions, row flattening, treemap jobs
  theme.rs                 tokens
  expansion.rs             which rows are open and whether by hand or by a reveal
  columns.rs               draggable column widths and the shared header band, stripes, selection background
  tree_view.rs             virtual rows, the free space row, keyboard navigation
  file_view.rs             tab strip with the Reset button, flat list with cap, folders and duplicates, keyboard navigation
  types_view.rs            the File Types table
  treemap_view.rs          texture, hover, click, zoom, selection outlines
  details_view.rs          details panel
  status_bar.rs
  menus.rs                 menu bar and shortcuts
  dialogs.rs               modals, the Preferences dialog and the cleanup output window
apps/oxtree/src/
  main.rs
```

## Data flow

1. `apps/oxtree` parses arguments and builds an `OxTreeApp`, which starts a scan through `oxtree_native::scan`.
2. The walker's worker threads produce one `DirRecord` per directory. When the queue drains, `Tree::build` turns the records into the arena and the result crosses an `mpsc` channel as `ScanEvent::Done`.
3. `OxTreeApp::pump`, called at the top of every frame, drains that channel and every other one: treemap render results, cleanup output lines, the trash job, the directory picker.
4. Views are free functions taking `&mut OxTreeApp` and a `Ui`. They read state and call methods; they hold no state of their own.
5. The treemap view asks for a texture of its current pixel size. `request_treemap` spawns one worker that lays out and shades the tree and sends back the pixels and the tiles; the view draws the latest texture at its own pixel size, paints the free space strip, the tile names and the selection outlines over it, and uses the tiles for hit testing.
6. The selection is a set (`marked`) plus the current item (`selected`). Views call `select` for a plain click and `select_in` with the modifiers and the rows as shown for Ctrl and Shift clicks; actions take `selection()`. A cleanup over several items runs once per item through `cleanup_queue`, continued from `pump` when the previous run finishes.
7. The File Types table is built on the UI thread the first time the tab is shown after a scan (10 ms for /usr); duplicate counts are built on a thread like the File View list, and the list waits for them.

## Threads and channels

No async runtime. Every blocking job runs on a `std::thread` and reports through an `std::sync::mpsc` channel drained once per frame, requesting a repaint when it finishes. The scan uses a `Mutex<Vec<Job>>` with a `Condvar` as its work queue and atomics for progress counters.

## Errors

Errors are `String`s shown in the status bar and, when they stop an action, in a modal. Everything from the filesystem, a child process or a configuration file is treated as hostile: bounded reads (`cache_io::MAX_BYTES`), `get()` instead of indexing, no `unwrap` outside tests. The workspace forbids `unsafe` and denies the panic lints in `Cargo.toml`.

## Dependencies

Four direct dependencies outside std:

| Crate | Why |
|-------|-----|
| log 0.4, optional | The `OXTREE_STARTUP` logger that timestamps eframe's, winit's and glutin's own log records; only with the `diagnostics` feature. The crate is already in the tree through eframe and has no dependencies. |
| regex-lite 0.1 | The `/regex` mode of the search box. About 150 KB with no dependencies and no Unicode classes, which file names do not need; the full `regex` crate was rejected at about 500 KB plus its tree. |
| epaint_default_fonts 0.37 | Ubuntu Light and egui's icon font, registered by hand so that Hack, the 309 KB monospace face, stays out of the binary. |
| eframe 0.37 (glow, wayland, x11, system_fonts, clipboard) | The window and the immediate-mode UI. glow is the lighter renderer; wgpu adds a shader compiler and more memory for nothing a treemap needs. The clipboard feature pulls in arboard and smithay-clipboard so Ctrl+V reads the system clipboard; without it egui only pastes what it copied itself. 0.37 paces Wayland frames by compositor callbacks instead of a blocking swap, so the app keeps running when the compositor withholds frames. The system_fonts feature loads the machine's own fonts on demand for characters the bundled face lacks, which is what makes CJK and emoji file names readable; it costs about 620 KB of fontique and friends. |

Everything else is a child process: `pkexec` for Rescan as Root, `df` for volume sizes and free space, `gzip` for compressed cache files, `gio trash`, `kdialog` or `zenity` for the directory chooser, `date +%z` for the time zone, `curl` for the opt-in update check, the user's shell for cleanups.

## Settings

`~/.config/oxtree/settings.conf`, INI, read once in `main` before the window (it holds the window size and the theme) and handed to `OxTreeApp` through `EarlyWork`. The app keeps settings as ordinary fields; `apply_settings` loads them and `to_settings` collects them for `config::save_settings`, called when the Preferences dialog closes and from `on_exit`. A root instance reads the invoking user's file through `OXTREE_CONFIG_DIR` and never writes it.
