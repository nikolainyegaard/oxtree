# Architecture

OxTree is a Cargo workspace with four layers. Dependencies point downwards only; nothing below the binary opens a window.

```
apps/oxtree          the binary: arguments, the eframe window, --bench, --screenshot
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
  mime.rs                  file categories and QDirStat's default table
  glob.rs                  * and ? matching for category patterns
  cleanup.rs               Cleanup struct, placeholders, standard set, INI mapping
  ini.rs                   QSettings-compatible INI read and write
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
  config.rs                XDG config dir, cleanups file, QDirStat fallback
  cache_io.rs              cache files on disk, gzip through the system binary
  localtime.rs             date +%z once
crates/ui/src/
  lib.rs
  app.rs                   OxTreeApp state, pump(), actions, row flattening, treemap jobs
  theme.rs                 tokens
  tree_view.rs             header and virtual rows, keyboard navigation
  treemap_view.rs          texture, hover, click, zoom, selection outlines
  details_view.rs          details panel
  status_bar.rs
  menus.rs                 menu bar and shortcuts
  dialogs.rs               modals and the cleanup output window
apps/oxtree/src/
  main.rs
```

## Data flow

1. `apps/oxtree` parses arguments and builds an `OxTreeApp`, which starts a scan through `oxtree_native::scan`.
2. The walker's worker threads produce one `DirRecord` per directory. When the queue drains, `Tree::build` turns the records into the arena and the result crosses an `mpsc` channel as `ScanEvent::Done`.
3. `OxTreeApp::pump`, called at the top of every frame, drains that channel and every other one: treemap render results, cleanup output lines, the trash job, the directory picker.
4. Views are free functions taking `&mut OxTreeApp` and a `Ui`. They read state and call methods; they hold no state of their own.
5. The treemap view asks for a texture of its current pixel size. `request_treemap` spawns one worker that lays out and shades the tree and sends back the pixels and the tiles; the view draws the latest texture stretched to its rectangle and uses the tiles for hit testing.

## Threads and channels

No async runtime. Every blocking job runs on a `std::thread` and reports through an `std::sync::mpsc` channel drained once per frame, requesting a repaint when it finishes. The scan uses a `Mutex<Vec<Job>>` with a `Condvar` as its work queue and atomics for progress counters.

## Errors

Errors are `String`s shown in the status bar and, when they stop an action, in a modal. Everything from the filesystem, a child process or a configuration file is treated as hostile: bounded reads (`cache_io::MAX_BYTES`), `get()` instead of indexing, no `unwrap` outside tests. The workspace forbids `unsafe` and denies the panic lints in `Cargo.toml`.

## Dependencies

One direct dependency outside std:

| Crate | Why |
|-------|-----|
| eframe 0.36 (glow, wayland, x11, default fonts) | The window and the immediate-mode UI. glow is the lighter renderer; wgpu adds a shader compiler and more memory for nothing a treemap needs. |

Everything else is a child process: `gzip` for compressed cache files, `gio trash`, `kdialog` or `zenity` for the directory chooser, `date +%z` for the time zone, the user's shell for cleanups.

Planned, with its justification settled in advance: `regex-lite` for the search box's regex mode, about 150 KB with no dependencies and no Unicode classes, which file names do not need; the full `regex` crate was rejected at about 500 KB plus its tree. The opt-in update check will use `curl` as a child process rather than an HTTP crate.
