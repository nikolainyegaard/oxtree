# OxTree vs WizTree: target-app parity

> **Last reviewed:** 2026-10-22 · **Last updated:** 2026-10-22 · **Change:** settings file and Preferences dialog with the pinned items, theme, row height, size decimals, search history, opt-in update check, reset; earlier the same day: File Types view, CSV export and headless export, copy tree to clipboard, multi-selection, duplicates, names on tiles, free space in tree, treemap and status bar, hard links shared, Expand All and expand to level, resizable columns, alternating rows, sideways scrolling, the Reset button; earlier the same day: crossing local filesystems is the default, mounted filesystem list in Open Directory, entered mount points marked; a Ports section pins the Windows MFT reader and the macOS branch; earlier the same day: search landed (WizTree syntax, name or path mode, regex via regex-lite, highlighted matches, Ctrl+F); earlier the same day: File View landed (flat list, top-N cap, folders toggle, Ctrl+1 and Ctrl+2); earlier the same day: first version, inventory taken from WizTree's guide, feature page and version history · **Target:** WizTree 4.33 for Windows (16 September 2026), with WizTreeMac 1.00 as the reference for what WizTree does without an MFT

The authoritative answer to "how close is OxTree to WizTree, and what is left". Each row is a WizTree feature, what OxTree has for it today, and what closing the gap touches. The ranked list of what to build next is at the end. QDirStat is the reference for behaviour and defaults where WizTree has nothing to say; it is not the target.

## Headline (2026-10-22, OxTree 0.1.0, `main` after 15ca189)

| Number | Value | How |
|---|---|---|
| **Feature breadth** (does each WizTree feature exist?) | **77%** | Done rows plus half of Partial rows, over the 97 rows that apply on Linux; counted from the tables below |
| **Rows** | 65 done, 20 partial, 12 missing, 6 not applicable, 3 declined | counted over the status column of the tables below |
| **Stage** | **beta** | Scan, tree, treemap, File View, File Types, duplicates, search, export, multi-selection, cleanups and trash work end to end; what is left is settings, rename and the smaller items |

The number is honest about breadth only. Where OxTree has a feature it is usually complete, because the views were built from QDirStat, which is deeper than WizTree in those areas. What is missing is the small items at the end of the ranked list and the Windows-only plumbing.

## Target measured

- **WizTree 4.33** as described by the vendor's guide (Advanced File Search, CSV export, clipboard, command line export, MFT dump, installer options), the feature page, the "what's new" history back to 1.01, and the WizTreeMac comparison. Not run: WizTree is Windows and macOS only. Labels are the vendor's.
- **Not applicable** marks what has no Linux equivalent or no equivalent in a GPL desktop app: the NTFS Master File Table read, MTP phone scanning, Windows Explorer integration, the installer, the supporter code. **Declined** marks what OxTree will not do on purpose, with the reason.
- **OxTree** as of the working tree after commit 15ca189 on 2026-10-22, from the code and from running it.

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
| Scan a whole drive or a single folder | Done | Any directory from the command line or Open Directory; without an argument the home directory, a chosen directory, or the dialog, by the launch setting | |
| Scan several drives or folders at once, custom folder list | Missing | One root per window | A virtual root with several children in the arena; Open Directory taking several paths |
| Scan network drives and non-NTFS volumes | Partial | Any mounted filesystem can be the root; crossing into network mounts during a scan is refused by the mount rules | Nothing to change unless someone asks to cross into NFS |
| Cross into other local filesystems during a scan | Done | On by default, as `du` and `ncdu` do; kernel tables and network mounts stay leaves, and removable media under /run with them. Checkbox in Open Directory and in Preferences, saved. Entered mount points keep a disk badge on their folder icon and show their filesystem in the details panel | |
| Scan phones and cameras over MTP | N/A | | GVFS exposes MTP as a mount, so a scan of /run/user/*/gvfs would work; not planned |
| Hard-linked files counted once, allocated totals match the OS | Done | Each link of a file carries an equal share of its allocated size, so the totals count the file once whichever link the walk met first; links outside the scanned directory make a file look smaller than it is, as in QDirStat | |
| Allocated size and file size both available | Partial | Allocated size only (`st_blocks * 512`); the details panel shows it under both labels | A second u64 per node (8 MB per million files) and a column toggle |
| Total, free and reserved space of the volume | Done | Free and total space of the scanned filesystem in the status bar and the details panel for every scan; a `<Free Space>` row in the tree and a grey strip in the treemap when the scanned directory is a mount point, from `df` | Reserved space is not shown separately |
| Include and exclude filters applied during the scan | Partial | Exclude by directory name only, default `.snapshot` | Glob patterns on full paths, from the settings file; the glob matcher exists |
| File system monitoring, live updates | Declined | | inotify on a million directories is not viable; F5 and the rescan after cleanups cover it |
| Drive list with automatic refresh on USB insert | Done | Open Directory lists every mounted local filesystem with type, size, used and free and a Scan button; reloaded each time the dialog opens | |
| Rescan with F5, keeping the view where it was | Done | Old tree stays on screen, selection, expanded folders and zoom carry over by path | |
| Scan progress in the status bar and on the taskbar | Partial | Status bar shows items, directories and bytes three times a second; no taskbar progress | Taskbar progress is not a Linux desktop convention; nothing planned |
| Long path support | Done | No path length limit on Linux | |

## Tree view

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Tree with Size, Allocated, Modified, Files, Folders, % of Parent columns | Partial | Name, Subtree Percentage, %, Size, Items, Files, Subdirs, Last Modified, resizable by dragging the header dividers; no Attributes column, Size is allocated size | Attributes become Permissions, which the details panel already shows; add as a column |
| Percent bar per row | Done | QDirStat's bar, coloured by depth | |
| Sort by any column, sorted column highlighted | Done | Click a header; indicator triangle on the sorted column | |
| Expand All, reset tree (F4), expand to a level | Done | Close All Tree Levels (Ctrl+W), Expand All Tree Levels (Shift+F4), Expand Tree to Level 1 to 9 in the View menu; the Reset button in the tab strip closes the tree and clears every filter | |
| Parent and child navigation (F6, F7) | Done | Alt+Up, Left and Right arrows, Enter | |
| Free space shown as a row in the tree | Done | `<Free Space>` as the last row under the root, with its share of the volume as a bar, when the scanned directory is a mount point | |
| Deleted items marked until the next rescan | N/A | OxTree rescans after every delete, so the item is simply gone | |
| Separate File Name column, header show and hide (Ctrl+H) | Missing | | Low value; header hide is a one-liner if asked |
| Adjustable row height, font size with Ctrl+wheel | Partial | Row height in Preferences, 16 to 40, saved | Font size: egui's zoom factor on Ctrl+wheel over the tables; Ctrl+wheel over the treemap is zoom already |
| Grid lines and alternating row colours | Done | Alternating row colours in all three tables | |
| Values too wide for a cell marked with a red edge | Partial | Columns are resizable by dragging the header dividers and the tables scroll sideways, so nothing is cut off for long; no red edge | Low value now |
| Status bar with selection totals, file count, hovered name | Done | Selection path and size, or the count and total of a multi-selection, on the left; item count, total and free space on the right; the hovered tile's name is in the tooltip | |
| Max modified date per folder | Done | Last Modified is the newest mtime in the subtree | |
| Configurable decimal places | Done | Size decimals in Preferences, 0 to 3 | |

## Treemap

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Visual treemap sized by file size | Done | Squarified cushion treemap, rendered on worker threads, rebuilt every frame of a resize | |
| Zoom in and out, navigate to a folder (F10, F11) | Done | Ctrl+Plus, Ctrl+Minus, Ctrl+0, Ctrl+wheel, double click, context menu | |
| File and folder names drawn on the tiles | Done | Names on the tiles that are leaves of the layout and at least 48 by 14 points, white with a shadow; View, Names on Treemap Tiles turns them off | Directory names are not drawn over their children's tiles |
| Colours by extension | Done | QDirStat's MIME categories: images, videos, music, documents, source, archives, executables, junk | |
| Custom treemap colours | Partial | Category table in `crates/core/src/mime.rs`; no configuration file or dialog | The category file QDirStat uses, then a dialog |
| Free space shown on the treemap | Done | A grey strip on the right, sized by the volume's free share, when the scanned directory is a mount point and the map shows the whole tree | |
| Show allocated size or file size on the treemap | Partial | Allocated only | With the second size column, a toggle |
| Hover shows the item | Done | Tooltip with path and size | |
| Click selects and syncs with the tree, right-click menu | Done | Selection outlines in yellow and red, as QDirStat | |
| Export the treemap as an image, with size, grayscale and free space options | Partial | `--screenshot FILE` writes the whole window as PPM in a diagnostics build | A File menu entry writing only the treemap at a chosen size; PNG needs an encoder, PPM plus `magick` is the lazy answer |
| Deleted items marked in the treemap | N/A | Rescan removes them | |

## File view, file types, duplicates

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| File View: every file and folder in one flat list sorted by size | Done | File View tab (Ctrl+2): path, size, percent of total, last modified; sort by column; click selects, double click or Enter jumps to the item in the tree; same right-click menu | |
| Top 1000 files, max files to display setting | Done | Show 1 000, 10 000, 100 000 or All, default 1 000, saved with the Folders, Duplicates and Name or Path choices | |
| Folders shown in the File View | Done | Folders checkbox, off by default | |
| File Types view: extension, count, size, allocated, colour | Done | File Types tab (Ctrl+3, F8): extension with its category colour, category, files, size, share; sortable; built in about 10 ms for /usr | Allocated is the only size |
| Select all files of an extension | Done | Double click a type lists its files in the File View; Ctrl+A there selects them all | |
| Duplicate files by name and size, or name, size and date | Done | Duplicates combo in the File View strip: Off, Name and size, Name, size and date; the list then shows only files with a duplicate, grouped, the group wasting the most space first; counts are built on a thread, 50 ms for /usr | Contents are not compared, as in WizTree |
| Duplicate count and duplicate size columns, plus (+) to expand | Done | Dups and Dup Size columns appear with the mode; groups are flat rows next to each other rather than collapsed | |

## Search and filter

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| File name search with `*` and `?` wildcards | Done | Search box in the File View strip (Ctrl+F); a term without wildcards matches as a substring, as in WizTree | |
| Multiple terms: space is AND, `\|` is OR, quotes group | Done | Plus `!` for NOT; quoted terms force a name match | |
| Size, allocated, date, time, name length and path length operators | Done | `>100m`, `a<=1g`, `<2024/01/01`, `>=today-7`, `=13:00`, `>now-2h`, `namelen>40`, `pathlen<100`; dates and times are local | Allocated and size are the same number until the second size column exists |
| `=folder` and `=file` filters | Done | | |
| Regular expression search (`/` prefix) | Done | `regex-lite`, case-insensitive | |
| Match File Name Only or Match Entire Path | Done | Name and Path radio buttons | |
| Search history dropdown | Done | The dropdown after the search box, last ten searches, saved | |
| Include and exclude filters (Ctrl+Shift+F), applied to scan results or during the scan | Partial | Exclude by directory name during the scan | The query parser applied to the arena, producing a filtered index for the views |
| Highlighted matches in results | Done | The first substring term is highlighted in the path; globs and regexes are not | |
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
| Multi-folder copy, cut and delete; multi-select (Alt+F3) | Done | Ctrl+click and Shift+click in the tree and the File View, Ctrl+click on tiles, Ctrl+A for every row; trash, cleanups and copy path act on the whole selection, the status bar and the details panel show its count and size, every selected tile is outlined | Copy and cut of files stay declined |
| Custom actions beyond delete | Done | Nine standard cleanups, user-defined ones in `cleanups.conf`, output window, QDirStat-compatible | OxTree has more than WizTree here |

## Export and import

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Export to CSV from the GUI (Ctrl+Alt+E): File Name, Size, Allocated, Modified, Attributes, Files, Folders | Done | File, Export to CSV (Ctrl+Alt+E) for the whole tree, Export Subtree to CSV in the right-click menu; the same columns, directories with a trailing slash before their contents; 170 ms for /usr | Allocated repeats Size until a second size exists |
| Copy file and size info to clipboard, indented like the tree | Done | Edit, Copy Tree to Clipboard (Ctrl+Alt+C): the rows as shown, name and size, two spaces per level | |
| TSV export and import | Partial | Tab separated is a checkbox in the dialog and `--tsv` on the command line | Import is below |
| Import a WizTree CSV for comparison | Missing | | A reader into DirRecords, like the cache reader; worth it only if someone has WizTree exports |
| Export the file types list | Missing | | Part of the file types view |
| Percent of parent and max depth export options | Done | Both in the dialog and on the command line | |
| UTC timestamps on export | Done | A checkbox and `--utc` | |
| Save and load a scan (MFT dump) | Done | QDirStat cache files, read and write, plain or gzipped; the same thing by another name | |
| Drag an export file into the window to load it | Missing | | egui exposes dropped files; map to the cache reader |

## Command line

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Scan a path given on the command line | Done | `oxtree PATH` | |
| Automated CSV export without a window (`/export`, filters, sort, columns) | Done | `oxtree DIR --export FILE` scans and writes without a window, `-` for stdout, `--tsv`, `--utc`, `--max-depth N`, `--percent`, `--search QUERY` as a filter; `--cache FILE --export` converts a cache file | No sort or column choice; the file is in tree order |
| Treemap image export from the command line | Partial | `--screenshot` writes the window | Treemap only, chosen size, no window |
| Load an export file from the command line | Done | `--cache FILE` | |
| Config folder location parameter | Partial | `OXTREE_CONFIG_DIR`, which Rescan as Root uses | A `--config DIR` flag if anyone asks |
| Debug logging parameter | Partial | `OXTREE_STATS=1` prints rebuild and CPU numbers | A `--log` flag if ever needed |
| Installer and registry options, silent install, read-only policy settings | N/A | | Packaging (rpm, Flatpak) is a separate task; policy overrides are not a Linux convention |

## Keyboard shortcuts

| WizTree | OxTree | Status |
|---|---|---|
| F3 select folder | Ctrl+O | Done |
| Shift+F3 open CSV | File, Read Cache File | Partial (no shortcut) |
| F4 reset tree, Shift+F4 expand all | Ctrl+W close all, Shift+F4 expand all, the Reset button | Done |
| F5 rescan | F5 | Done |
| F6, F7 parent and child | Alt+Up, Left, Right, Enter | Done |
| F8 file types view | F8, Ctrl+3 | Done |
| F9 treemap toggle | F9 | Done |
| F10, F11 treemap zoom | Ctrl+Plus, Ctrl+Minus | Done |
| F2 rename | | Missing |
| Ctrl+H hide header | | Missing |
| Ctrl+Shift+F filter | | Missing |
| Ctrl+Alt+E export, Ctrl+Alt+C copy | Ctrl+Alt+E, Ctrl+Alt+C; Ctrl+C copies the paths | Done |
| Ctrl+wheel font size | | Missing |

## Settings and appearance

| WizTree feature | Status | OxTree today | What closing it touches |
|---|---|---|---|
| Options menu with persistent settings | Done | Edit, Preferences (Ctrl+comma) and `~/.config/oxtree/settings.conf`: cross filesystems, rebuild delay, tile names, row height, size decimals, theme, update check; the window size, the panels, the column widths, the File View choices and the search history are saved with it | |
| Dark mode, Windows theme, high contrast | Done | Follow the desktop, light or dark, in Preferences | No high contrast theme |
| High DPI scaling | Done | egui scales with the compositor | |
| Tweaks: drag and drop, double click action, rendering | Partial | Rebuild delay and tile names are the rendering tweaks | Drag and drop is declined above; double click on a file is a small item |
| Portable mode, config next to the executable | Partial | `OXTREE_CONFIG_DIR=. oxtree` | |
| Run as administrator | Done | File, Rescan as Root replaces the window with one running as root through polkit on the same path; the title shows [root], the root instance reads the user's configuration and never writes it; the status bar suggests it when directories could not be read | A polkit action file is in `packaging/` for installs that put the binary in /usr/bin |
| Check for updates | Done | Opt-in: a Check for Updates button in Preferences, Check automatically off by default, daily, weekly or monthly; one GET of the GitHub releases API through `curl`, the tag compared with the version, a line with a link to the release page; no download, no install, no request while the toggle is off | |
| Reset configuration | Done | Reset to Defaults in Preferences, applied at once and written | Cleanups keep their own file |
| Translations | Missing | | English only; strings are literals in the views |
| Supporter code, enterprise licence | N/A | GPL | |

## Ports

Pinned, not scheduled. Nothing outside `crates/native` touches the operating system, so a port is one crate's worth of branches.

- **Windows, only with an MFT reader.** A directory walk on NTFS would land in WinDirStat territory and lose to WizTree on the one number its users care about, so a Windows build is worth doing only with WizTree's approach: read the volume raw through `\\.\C:` with administrator rights, parse the MFT (boot sector, `$MFT` data runs, file records, `$FILE_NAME` and `$DATA` attributes, attribute lists, hard links) and assemble the tree from parent references. That is a project of its own, a few thousand lines plus a corpus of real volumes, and it needs `unsafe` for the raw handle, which the workspace forbids today; the ordinary walk stays for exFAT, ReFS and network shares, as WizTree's own fallback does. Everything above the native crate, the arena, the treemap, the views, the search and the cache files, is already platform-agnostic.
- **macOS, a native branch of the same crate.** The walk, the shell cleanups, gzip and date work unchanged. Mounts come from `mount` or `getmntinfo`, trash and the directory chooser through `osascript`, the terminal and file manager are Terminal.app and `open -R`. WizTreeMac has no MFT either and scans with bulk directory enumeration on threads, which is what OxTree does.

## Beyond WizTree

What OxTree has that WizTree does not, so the comparison runs both ways: user-defined cleanup actions with an output window, a terminal opener, QDirStat cache file compatibility, a details panel with permissions and category, exclude rules, cushion shading, and the whole thing in a 9 MB binary with no runtime dependencies beyond Mesa.

## Ranked work list

Ordered by how much of WizTree's daily use each one unlocks, divided by what it touches.

1. **File View with a top-N cap.** Done 2026-10-22.
2. **Search and filter.** Search and the history done 2026-10-22. What remains is the tree filter: Ctrl+Shift+F include and exclude patterns applied to the scan results with sizes recomputed over the matches.
3. **File Types view.** Done 2026-10-22.
4. **CSV export and clipboard copy.** Done 2026-10-22, with the headless mode.
5. **Multi-selection.** Done 2026-10-22.
6. **Duplicates.** Done 2026-10-22.
7. **Names on treemap tiles, free space in tree and treemap, hard link dedup.** Done 2026-10-22.
8. **Settings file and Preferences dialog.** Done 2026-10-22.
9. **Rename, open on double click, font size, a second size column (file size next to allocated), the treemap image export, a CSV import, the tree filter.** Small items taken as they come.

Items 1 to 8 are in. Review this document when one of the small items lands: update the row, the counts and the headline.
