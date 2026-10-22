# Changelog

All notable changes to OxTree are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.1] - 2026-10-23

### Fixed

- Cancel in the Open Directory dialog closes it even when nothing has been scanned yet, leaving an empty window
- The release build no longer prints a first frame timing line to the terminal; that line belongs to the diagnostics build

### Changed

- Launched without a directory, OxTree scans the home directory at once instead of asking; Preferences, At launch chooses between the home directory, asking, or a directory of your own

## [0.1.0] - 2026-10-22

The first release.

### Added

- Parallel directory scan on all cores, crossing into other local filesystems by default while kernel tables and network mounts stay out; progress in the status bar; hard-linked files counted once
- Directory tree with percent bars, sizes, item counts and modification dates, sortable and resizable columns, alternating rows, sideways scrolling, Expand All and expand to level, painted icons per kind, unreadable directories in red
- Cushion treemap rendered off the UI thread and rebuilt every frame of a resize, with names on the tiles, hover details, click to select, double click and Ctrl+wheel to zoom, free space as a strip
- Details panel with size, counts, dates, permissions, category and filesystem
- File View: every file in one flat list, largest first, with a Show cap and a Folders toggle; search with WizTree's syntax (`*.mp3`, `>100m`, `<2024/01/01`, `=folder`, `/regex`, AND, OR, NOT), by name or path, matches highlighted, a history dropdown
- File Types view: one row per extension with category colour, file count, size and share; double click lists the type's files
- Duplicates by name and size, or name, size and date, grouped, with Dups and Dup Size columns
- Multi-selection with Ctrl and Shift clicks and Ctrl+A; trash, cleanups and copy act on every selected item
- Cleanup actions with QDirStat's standard set, user-defined ones in a configuration file, confirmation, output window; move to trash through the desktop trash
- Rescan as Root through polkit, replacing the window on the same directory
- Open Directory dialog listing every mounted local filesystem with size, used and free
- Export to CSV or TSV from the menu, the right-click menu and the command line (`--export`), copy the tree to the clipboard
- QDirStat cache files, plain or gzipped, read and written
- Settings file and a Preferences dialog: cross filesystems, treemap rebuild delay, tile names, row height, size decimals, theme, an opt-in update check that is off by default; window size, panels, column widths, File View choices and search history saved with it
- A Reset button that returns every view to its defaults
- File names in any script, through the system's fonts
- A `diagnostics` Cargo feature with startup timing, a screenshot mode and a benchmark binary, off in release builds

[Unreleased]: https://github.com/nikolainyegaard/oxtree/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/nikolainyegaard/oxtree/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/nikolainyegaard/oxtree/releases/tag/v0.1.0
