# OxTree vs WizTree: target-app parity

> **Last reviewed:** 2026-10-22 · **Last updated:** 2026-10-22 · **Change:** first version, inventory taken from WizTree's guide, feature page and version history · **Target:** WizTree 4.33 for Windows (16 September 2026), with WizTreeMac 1.00 as the reference for what WizTree does without an MFT

The authoritative answer to "how close is OxTree to WizTree, and what is left". Each row is a WizTree feature, what OxTree has for it today, and what closing the gap touches. The ranked list of what to build next is at the end. QDirStat is the reference for behaviour and defaults where WizTree has nothing to say; it is not the target.

## Headline (2026-10-22, OxTree 0.1.0, `main` at 0ed3c34)

| Number | Value | How |
|---|---|---|
| **Feature breadth** (does each WizTree feature exist?) | **36%** | Done rows plus half of Partial rows, over the 96 rows that apply on Linux; counted from the tables below |
| **Rows** | 25 done, 19 partial, 52 missing, 6 not applicable, 3 declined | counted over the status column of the tables below |
| **Stage** | **alpha** | Scan, tree, treemap, cleanups and trash work end to end; everything WizTree calls the File View does not exist yet |

The number is honest about breadth only. Where OxTree has a feature it is usually complete, because the views were built from QDirStat, which is deeper than WizTree in those areas. Where it is missing, it is missing entirely: there is no flat file list, no search, no file type view and no export.

## Target measured

- **WizTree 4.33** as described by the vendor's guide (Advanced File Search, CSV export, clipboard, command line export, MFT dump, installer options), the feature page, the "what's new" history back to 1.01, and the WizTreeMac comparison. Not run: WizTree is Windows and macOS only. Labels are the vendor's.
- **Not applicable** marks what has no Linux equivalent or no equivalent in a GPL desktop app: the NTFS Master File Table read, MTP phone scanning, Windows Explorer integration, the installer, the supporter code. **Declined** marks what OxTree will not do on purpose, with the reason.
- **OxTree** as of commit 0ed3c34 on 2026-10-22, from the code and from running it.

## Status legend

| Status | Meaning |
|---|---|
| Done | The feature exists and works the way WizTree's does, or better |
| Partial | Something exists but a user coming from WizTree would notice the difference |
| Missing | Nothing exists |
| N/A | No Linux equivalent, or Windows platform plumbing |
| Declined | Deliberately not built; reason in the row |

## Scanning

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Read the NTFS Master File Table directly | N/A | Linux has no MFT; ext4 inodes carry no names, btrfs tree search needs root. Recorded as a dead end in gotchas.md | Nothing; the parallel walk is the Linux answer and scans /usr in about 110 ms |
| Multi-threaded directory scan (the WizTreeMac path) | Done | One worker per core over a shared queue, stat relative to the directory handle | |
| Scan a whole drive or a single folder | Done | Any directory from the command line or Open Directory | |
| Scan several drives or folders at once, custom folder list | Missing | One root per window | A virtual root with several children in the arena; Open Directory taking several paths |
| Scan network drives and non-NTFS volumes | Partial | Any mounted filesystem can be the root; crossing into network mounts during a scan is refused by the mount rules | Nothing to change unless someone asks to cross into NFS |
| Cross into other local filesystems during a scan | Partial | Logic exists behind `OXTREE_CROSS_FILESYSTEMS=1`, kernel tables and network mounts stay leaves | A checkbox in Open Directory and a setting, pinned in gotchas.md |
| Scan phones and cameras over MTP | N/A | | GVFS exposes MTP as a mount, so a scan of /run/user/*/gvfs would work; not planned |
| Hard-linked files counted once, allocated totals match the OS | Missing | Each link counted | Track (dev, inode) for nlink > 1 in the walker, keep the first; QDirStat does the same |
| Allocated size and file size both available | Partial | Allocated size only (`st_blocks * 512`); the details panel shows it under both labels | A second u64 per node (8 MB per million files) and a column toggle |
| Total, free and reserved space of the volume | Missing | | `df` as a child process or one statvfs call; a free-space row in the tree and a grey area in the treemap |
| Include and exclude filters applied during the scan | Partial | Exclude by directory name only, default `.snapshot` | Glob patterns on full paths, from the settings file; the glob matcher exists |
| File system monitoring, live updates | Declined | | inotify on a million directories is not viable; F5 and the rescan after cleanups cover it |
| Drive list with automatic refresh on USB insert | Missing | | A mount list in Open Directory from `/proc/self/mounts`, already parsed for the crossing rules |
| Rescan with F5, keeping the view where it was | Done | Old tree stays on screen, selection, expanded folders and zoom carry over by path | |
| Scan progress in the status bar and on the taskbar | Partial | Status bar shows items, directories and bytes three times a second; no taskbar progress | Taskbar progress is not a Linux desktop convention; nothing planned |
| Long path support | Done | No path length limit on Linux | |

## Tree view

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Tree with Size, Allocated, Modified, Files, Folders, % of Parent columns | Partial | Name, Subtree Percentage, %, Size, Items, Files, Subdirs, Last Modified; no Attributes column, Size is allocated size | Attributes become Permissions, which the details panel already shows; add as a column |
| Percent bar per row | Done | QDirStat's bar, coloured by depth | |
| Sort by any column, sorted column highlighted | Done | Click a header; indicator triangle on the sorted column | |
| Expand All, reset tree (F4), expand to a level | Partial | Close All Tree Levels (Ctrl+W); no Expand All, no expand-to-level | QDirStat's Expand Tree to Level 0 to 9 menu; cheap, rows are rebuilt from the expanded set |
| Parent and child navigation (F6, F7) | Done | Alt+Up, Left and Right arrows, Enter | |
| Free space shown as a row in the tree | Missing | | Same statvfs work as the volume sizes |
| Deleted items marked until the next rescan | N/A | OxTree rescans after every delete, so the item is simply gone | |
| Separate File Name column, header show and hide (Ctrl+H) | Missing | | Low value; header hide is a one-liner if asked |
| Adjustable row height, font size with Ctrl+wheel | Missing | | egui zoom factor is a one-liner for Ctrl+wheel; row height from the settings file |
| Grid lines and alternating row colours | Missing | | A painter call per row; a setting |
| Values too wide for a cell marked with a red edge | Missing | | Columns are fixed width and sized for the formats; revisit with resizable columns |
| Status bar with selection totals, file count, hovered name | Partial | Selection path and size on the left, item count and total on the right; no multi-selection totals | Comes with multi-selection |
| Max modified date per folder | Done | Last Modified is the newest mtime in the subtree | |
| Configurable decimal places | Missing | | One setting into `format::size` |

## Treemap

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Visual treemap sized by file size | Done | Squarified cushion treemap, rendered on worker threads, rebuilt every frame of a resize | |
| Zoom in and out, navigate to a folder (F10, F11) | Done | Ctrl+Plus, Ctrl+Minus, Ctrl+0, Ctrl+wheel, double click, context menu | |
| File and folder names drawn on the tiles | Missing | | Text on tiles larger than a threshold, drawn with the painter after the texture; QDirStat does not do this, WizTree does |
| Colours by extension | Done | QDirStat's MIME categories: images, videos, music, documents, source, archives, executables, junk | |
| Custom treemap colours | Partial | Category table in `crates/core/src/mime.rs`; no configuration file or dialog | The category file QDirStat uses, then a dialog |
| Free space shown on the treemap | Missing | | A grey tile for free space once the volume size is known |
| Show allocated size or file size on the treemap | Partial | Allocated only | With the second size column, a toggle |
| Hover shows the item | Done | Tooltip with path and size | |
| Click selects and syncs with the tree, right-click menu | Done | Selection outlines in yellow and red, as QDirStat | |
| Export the treemap as an image, with size, grayscale and free space options | Partial | `--screenshot FILE` writes the whole window as PPM for development | A File menu entry writing only the treemap at a chosen size; PNG needs an encoder, PPM plus `magick` is the lazy answer |
| Deleted items marked in the treemap | N/A | Rescan removes them | |

## File view, file types, duplicates

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| File View: every file and folder in one flat list sorted by size | Missing | | A second view over the same arena: a sorted index of all nodes, `show_rows` for the list, the same columns and context menu as the tree. The largest single gap |
| Top 1000 files, max files to display setting | Missing | | The same view with a cap; sorting a million sizes takes a few milliseconds |
| Folders shown in the File View | Missing | | Part of the file view |
| File Types view: extension, count, size, allocated, colour | Missing | | A HashMap by extension over the arena, a table with four columns, coloured by category; QDirStat's File Type Statistics window is the model |
| Select all files of an extension | Missing | | Part of the file types view, needs multi-selection |
| Duplicate files by name and size, or name, size and date | Missing | | Group by (name, size[, mtime]) over the arena, a count and a size per group, a "duplicates only" filter; by content is out of scope, as in WizTree |
| Duplicate count and duplicate size columns, plus (+) to expand | Missing | | Part of the duplicates work |

## Search and filter

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| File name search with `*` and `?` wildcards | Missing | | A search box over the file view; the glob matcher exists in `crates/core/src/glob.rs` |
| Multiple terms: space is AND, `\|` is OR, quotes group | Missing | | A small query parser in core with a test |
| Size, allocated, date, time, name length and path length operators | Missing | | Same parser; dates need a parser for yyyy/mm/dd, today and now |
| `=folder` and `=file` filters | Missing | | Same parser |
| Regular expression search (`/` prefix) | Missing | | `regex-lite` (about 150 KB, no dependencies, no Unicode classes, enough for file names) behind the same prefix; the full `regex` crate was rejected for size. Part of the search work |
| Match File Name Only or Match Entire Path | Missing | | A radio pair on the search box |
| Search history dropdown | Missing | | Last ten queries in the settings file |
| Include and exclude filters (Ctrl+Shift+F), applied to scan results or during the scan | Partial | Exclude by directory name during the scan | The query parser applied to the arena, producing a filtered index for the views |
| Highlighted matches in results | Missing | | Paint the matched range in the name column |
| Accent-insensitive name search | Missing | | Fold to lowercase is there; accent folding needs a small table |

## File operations

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Delete with the system dialog, permanent | Partial | Move to Trash through `gio trash`, with Delete; permanent delete through the Delete Permanently cleanup with confirmation | Nothing; two separate actions are the safer design |
| Copy path to clipboard (Ctrl+Alt+C) | Done | Ctrl+C | |
| Cut, copy and paste files between folders | Declined | | A disk usage tool is not a file manager; Open File Manager Here covers it |
| Drag and drop to the file manager | Declined | | Same reason, and Wayland drag-out from egui is not solved |
| Rename (F2) | Missing | | A text field in the details panel and `std::fs::rename`, then a rescan; small |
| Open a file on double click | Partial | Double click expands a directory; files open through the Open in File Manager cleanup | `xdg-open` on double click of a file, which is what WizTree does |
| Explore folder with the file highlighted | Done | Open File Manager Here cleanup, desktop detection as QDirStat | |
| Multi-folder copy, cut and delete; multi-select (Alt+F3) | Missing | | Multi-selection in both views, trash and cleanups over a set, totals in the status bar; QDirStat has all of this |
| Custom actions beyond delete | Done | Nine standard cleanups, user-defined ones in `cleanups.conf`, output window, QDirStat-compatible | OxTree has more than WizTree here |

## Export and import

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Export to CSV from the GUI (Ctrl+Alt+E): File Name, Size, Allocated, Modified, Attributes, Files, Folders | Missing | | A writer over the arena in core with a test; a File menu entry and a context menu entry |
| Copy file and size info to clipboard, indented like the tree | Missing | | Same writer, text format, visible rows only |
| TSV export and import | Missing | | A separator option on the writer |
| Import a WizTree CSV for comparison | Missing | | A reader into DirRecords, like the cache reader; worth it only if someone has WizTree exports |
| Export the file types list | Missing | | Part of the file types view |
| Percent of parent and max depth export options | Missing | | Two parameters on the writer |
| UTC timestamps on export | Missing | | A flag on the writer |
| Save and load a scan (MFT dump) | Done | QDirStat cache files, read and write, plain or gzipped; the same thing by another name | |
| Drag an export file into the window to load it | Missing | | egui exposes dropped files; map to the cache reader |

## Command line

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Scan a path given on the command line | Done | `oxtree PATH` | |
| Automated CSV export without a window (`/export`, filters, sort, columns) | Missing | | A headless mode in `main.rs` calling the CSV writer; the benchmark mode shows the shape |
| Treemap image export from the command line | Partial | `--screenshot` writes the window | Treemap only, chosen size, no window |
| Load an export file from the command line | Done | `--cache FILE` | |
| Config folder location parameter | Missing | | XDG already covers the common case; `--config DIR` is a few lines |
| Debug logging parameter | Partial | `OXTREE_STATS=1` prints rebuild and CPU numbers | A `--log` flag if ever needed |
| Installer and registry options, silent install, read-only policy settings | N/A | | Packaging (rpm, Flatpak) is a separate task; policy overrides are not a Linux convention |

## Keyboard shortcuts

| WizTree | OxTree | Status |
|---|---|---|
| F3 select folder | Ctrl+O | Done |
| Shift+F3 open CSV | File, Read Cache File | Partial (no shortcut) |
| F4 reset tree, Shift+F4 expand all | Ctrl+W close all; no expand all | Partial |
| F5 rescan | F5 | Done |
| F6, F7 parent and child | Alt+Up, Left, Right, Enter | Done |
| F8 file types view | | Missing |
| F9 treemap toggle | F9 | Done |
| F10, F11 treemap zoom | Ctrl+Plus, Ctrl+Minus | Done |
| F2 rename | | Missing |
| Ctrl+H hide header | | Missing |
| Ctrl+Shift+F filter | | Missing |
| Ctrl+Alt+E export, Ctrl+Alt+C copy | Ctrl+C copies the path | Partial |
| Ctrl+wheel font size | | Missing |

## Settings and appearance

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Options menu with persistent settings | Missing | Cleanups are the only configuration file; two settings live behind environment variables, pinned in gotchas.md | A settings file in the INI format already parsed, then a Preferences dialog; the pinned list is the first content |
| Dark mode, Windows theme, high contrast | Partial | egui follows the desktop's light or dark preference; no explicit choice | A theme setting |
| High DPI scaling | Done | egui scales with the compositor | |
| Tweaks: drag and drop, double click action, rendering | Missing | | Settings, once there is a settings file |
| Portable mode, config next to the executable | Missing | | `--config DIR` covers the use |
| Run as administrator | Missing | | A Rescan as Root action running `pkexec oxtree PATH`; QDirStat's approach is a separate process too |
| Check for updates | Missing | No network access from the app today | Opt-in only: a Check for Updates button, a Check Automatically toggle that is off by default, and a frequency (daily, weekly, monthly). Never a request unless the toggle is on. One GET of the GitHub releases API through `curl` as a child process, compare the tag with the version, show a status line and a link; no download, no install. Pinned in gotchas.md for the settings work |
| Reset configuration | Missing | | Delete the settings file and reload defaults; part of the Preferences dialog |
| Translations | Missing | | English only; strings are literals in the views |
| Supporter code, enterprise licence | N/A | GPL | |

## Beyond WizTree

What OxTree has that WizTree does not, so the comparison runs both ways: user-defined cleanup actions with an output window, a terminal opener, QDirStat cache file compatibility, a details panel with permissions and category, exclude rules, cushion shading, and the whole thing in a 8.4 MB binary with no runtime dependencies beyond Mesa.

## Ranked work list

Ordered by how much of WizTree's daily use each one unlocks, divided by what it touches.

1. **File View with a top-N cap.** The second of WizTree's two views, and the one that answers "what are the biggest files". A sorted index over the arena, a flat list with the tree's columns and context menu, a cap setting. Needs nothing else first.
2. **Search and filter.** The query parser in core with the wildcard, AND and OR, size, date and type operators, then a search box over the file view and a filter over the tree. The glob matcher exists.
3. **File Types view.** Extension table with count, size and colour, double click to list the files of that type in the file view. QDirStat's window is the model.
4. **CSV export and clipboard copy.** One writer in core, three menu entries, a headless command line mode.
5. **Multi-selection.** Both views, trash and cleanups over the set, totals in the status bar. Also what "select all of this extension" and duplicates cleanup need.
6. **Duplicates.** Grouping over the arena, two columns, a filter.
7. **Names on treemap tiles, free space in tree and treemap, hard link dedup.** Three small items that make the pictures match WizTree.
8. **Settings file and Preferences dialog.** Unblocks the pinned items (cross filesystems, rebuild delay, opt-in update check), decimal places, row height, theme, expand level, column choice, reset to defaults.
9. **Rename, open on double click, Expand All, expand to level, font size.** Small items taken as they come.

Items 1 to 4 make OxTree usable the way WizTree is used. Items 5 to 9 are parity. Review this document when one of them lands: update the row, the counts and the headline.
