# Changelog

All notable changes to OxTree are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- File View: every file in one flat list, largest first, with a Show cap of 1 000, 10 000, 100 000 or All and a Folders toggle; Ctrl+1 and Ctrl+2 switch views, double click or Enter jumps to the item in the tree; `--files` starts in it
- Right-click menu on tree rows and treemap tiles
- Rescans keep the view in place: old tree stays until the new one lands, selection, expanded folders and zoom carry over
- Cross-filesystem scanning behind `OXTREE_CROSS_FILESYSTEMS=1`, with kernel and network mounts kept as leaves

### Changed

- eframe 0.37: on Wayland, frames are paced by the compositor's frame callbacks instead of a blocking swap, and the app keeps running while the compositor withholds frames (locked screen, covered window)

### Fixed

- Ctrl+wheel zoom over the treemap
- A running cleanup no longer blocks the others
- An existing but empty cleanups file is left alone
- Rebuild Treemap works
- ShowNever cleanups never open the output window
- Tree expand markers and the sort indicator rendered as boxes

## [0.1.0] - 2026-10-21

### Added

- Parallel directory scan with progress in the status bar
- Directory tree with percent bars, sizes, item counts and modification dates, sortable by column
- Cushion treemap rendered off the UI thread, with hover details, click to select, double click and Ctrl+wheel to zoom
- Details panel for the selected item
- Cleanup actions with QDirStat's standard set, confirmation, output window and configuration file
- Move to trash through the desktop trash
- Reading and writing QDirStat cache files, plain or gzipped
- Headless benchmark and screenshot modes for development

[Unreleased]: https://github.com/nikolainyegaard/oxtree/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/nikolainyegaard/oxtree/releases/tag/v0.1.0
