# Changelog

All notable changes to OxTree are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `OXTREE_STARTUP=1` prints where the time before the first frame goes; `OXTREE_EARLY_SCAN=1` and `OXTREE_THREADS=N` for measuring

- File, Rescan as Root: OxTree running as root through polkit replaces the window on the same directory, for the directories the user cannot read; the title shows [root], configuration is read from the user and never written; a polkit action file in `packaging/`
- A scan crosses into other local filesystems by default; kernel tables and network mounts stay out. The Open Directory dialog has the checkbox, lists every mounted local filesystem with size, used and free, and scans one with a click. Entered mount points keep a disk badge on their folder icon and the details panel names the filesystem
- Directories opened by a treemap click or a File View jump close again on the next such click, unless opened by hand or worked in since; the tree no longer accumulates every revealed path
- Tree icons per kind: folder, file, symlink, mount point, unreadable folder and the `<Files>` entry, painted rather than shipped as images; directories with an unreadable directory below them are shown in yellow as in QDirStat
- Search in the File View with WizTree's syntax: substrings and `*`/`?` globs, `|` for OR, `!` for NOT, quotes, size, date, time, name and path length comparisons, `=folder` and `=file`, `/regex`; Name or Path matching, highlighted matches, Ctrl+F; `--search QUERY` on the command line
- File View: every file in one flat list, largest first, with a Show cap of 1 000, 10 000, 100 000 or All and a Folders toggle; Ctrl+1 and Ctrl+2 switch views, double click or Enter jumps to the item in the tree; `--files` starts in it
- Right-click menu on tree rows and treemap tiles
- Rescans keep the view in place: old tree stays until the new one lands, selection, expanded folders and zoom carry over
- Cross-filesystem scanning behind `OXTREE_CROSS_FILESYSTEMS=1`, with kernel and network mounts kept as leaves

### Changed

- Scan threads wake the UI when they have news, so a finished scan is drawn on the next frame; the configuration reads run on a thread started before the window
- eframe 0.37: on Wayland, frames are paced by the compositor's frame callbacks instead of a blocking swap, and the app keeps running while the compositor withholds frames (locked screen, covered window)

### Fixed

- Row text in the tree and file views uses the strong text colour; egui's default label grey was never a choice

- Clicking a file in the treemap jumps to it in the tree even when its directory was collapsed; the tree rows were rebuilt a frame too late
- Paste into text fields: the eframe clipboard feature was off, so Ctrl+V had nothing to read

- Window content no longer bleeds through to the previous frame on Wayland: the frame is cleared with an opaque colour instead of eframe's 70% alpha default, which showed stale frames, ghosts and the wallpaper in the tree and treemap areas while moving the window or hiding the treemap
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
