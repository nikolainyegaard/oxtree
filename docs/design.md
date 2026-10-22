# Design

OxTree looks like QDirStat because its colours are QDirStat's defaults. QDirStat has no stylesheet; its appearance is its configuration defaults, and those are the tokens in `crates/ui/src/theme.rs`. The window chrome itself is egui's default style, light or dark following the desktop.

## Layout

```
+--------------------------------------------------------------+
| File  Edit  View  Go  Clean Up  Help                          |
+-----------------------------------------------+--------------+
| Name          Subtree Percentage  %  Size ... | Details      |
| ▾ /usr                                        |              |
|   ▸ lib64        [=======    ]  41.2%  6.1 GB |  name        |
|   ▸ share        [====       ]  28.0%  4.2 GB |  type        |
|   ...                                         |  size, items |
+-----------------------------------------------+  mtime, perm |
|                                               |  category    |
|                 treemap                       |  path        |
|                                               |              |
+-----------------------------------------------+--------------+
| Finished. Elapsed time: 0.125 sec        352 101 items, 14.8 GB |
+--------------------------------------------------------------+
```

The tree is the central panel, the treemap a resizable bottom panel (F9 hides it), the details a resizable right panel, the status bar one line at the bottom.

## Tokens

| Token | Value | Source in QDirStat |
|-------|-------|--------------------|
| Treemap file fill | `#de8d53` | `[Treemaps] FileFillColor` |
| Treemap directory gradient | `#606070` to `#707080` | `DirGradientStart`, `DirGradientEnd` |
| Treemap ambient light | 40 of 255 | `AmbientLight` |
| Treemap light vector | (-0.09759, -0.19518, 0.9759) | `TreemapView.cpp` |
| Minimum tile size | 3 px | `MinTileSize` |
| Selected item outline | `#ffff00` | `SelectedItemsColor` |
| Current item outline | `#ff0000` | `CurrentItemColor` |
| Percent bar background | `#a0a0a0` | `[PercentBar] Background` |
| Percent bar colours by depth | 12 colours, see theme.rs | `[PercentBar] Colors` |
| Percent bar column width | 180 | `PercentBarColumnWidth` |
| Unreadable directory text | `#ff0000` | `DirReadErrColor` |
| Directory with an unreadable directory below it | `#ffff00` | `SubtreeReadErrColor` (dark theme value) |
| Row text | egui's strong text colour | egui's plain label colour is a mid grey meant for captions; rows are the content |
| Entered mount point | folder icon with a small disk badge | a directory the scan crossed into; the details panel names the filesystem and device |
| Tree icons | painted folder `#e99a2e`, page `#d8d8d8`, symlink arrow `#40a0ff`, disk `#a0a8b8` | the colours of QDirStat's tree-medium icon set, drawn with the painter so no image assets ship |
| Status message timeout | 3 s | `StatusBarTimeoutMillisec` |
| Row height | 20 | OxTree |
| Name column default width | 400 | OxTree; columns are dragged at the header dividers and the tables scroll sideways |
| File View path column default width | 520 | OxTree |
| Alternating rows | egui's faint background on every second row | OxTree, in all three tables |
| Selected rows | egui's selection fill; the current item of a multi-selection gets a 1 px selection outline when not itself selected | OxTree |
| Free space | `#707880` for the tree row's bar and the treemap strip | OxTree |
| Treemap tile names | egui's small text style, white over a black shadow, on leaf tiles of at least 48 by 14 points | OxTree; WizTree draws names, QDirStat does not |

What the tree colours mean, as in QDirStat: red name and `[Not Read]` is a directory that could not be read; a yellow name is a directory with such a directory somewhere below it; a grey name and `[Mount Point]` is another filesystem that was not entered, which with crossing on, the default, means a kernel table or a network mount. QDirStat also sets dominant items in bold (a child whose share is at least five times the median share of its siblings, clamped between 3% and 70%, only while sorted by size descending); OxTree does not, because the bundled font has no bold face and adding one costs more than the signal is worth. WizTree colours nothing in its tree.

Category colours live in `crates/core/src/mime.rs` with the table: Junk red, Compressed Archives green, Uncompressed Archives olive, Compressed Files green, Images cyan, Uncompressed Images red, Videos `#a0ff00`, Music yellow, Documents blue, Source Files cyan, Object or Generated Files `#ffa000`, Libraries `#ffa000`, Executables magenta, Symbolic Links blue.

## Copy

Status bar states, taken from QDirStat where it had them:

| State | Text |
|-------|------|
| Scanning | `Reading /usr...` then `Reading... 123 456 items in 12 345 directories, 4.2 GB` |
| Finished | `Finished. Elapsed time: 0.125 sec` plus `[N directories could not be read]` when applicable |
| Aborted | `Reading aborted.` |
| Selection | `/usr/lib64  6.1 GB` |
| Root | `[N directories could not be read]  File, Rescan as Root reads them.` after a scan; `Waiting for authorization...` while the polkit dialog is up; then this window closes as the root window appears; `Authorization dismissed.` or `Not authorized to run OxTree as root.` if it does not; the root window's title is `OxTree [root]` |
| Cleanup | `Running Delete Junk Files...`, then `Cleanup action finished successfully.` or `Cleanup action finished with N errors.` |
| Trash | `Moved to trash: /path` (or `N items`) or `Move to trash failed: reason` |
| Multi-selection | `N items selected  1.2 GB` on the left; the details panel says `N items selected, 1.2 GB` above the current item |
| Free space | `352 501 items, 14.8 GB, 602.8 GB of 951.3 GB free` on the right; the details panel shows `Volume: 602.8 GB free of 951.3 GB` for the root |
| Export | `Exported /path` or `Cannot write /path: reason` |
| Copy tree | `Copied N rows to clipboard`; copying several paths says `Copied N paths to clipboard` |
| Clipboard | `Copied to clipboard: /path` |
| Cache | `Read cache file /path`, `Can't read cache file "/path": reason` |

Tree view column headers: Name, Subtree Percentage, %, Size, Items, Files, Subdirs, Last Modified. Directories that could not be read show `[Not Read]` in red in the percentage column; skipped mount points show `[Mount Point]`. The `<Free Space>` row, shown under the root when the scanned directory is a mount point, has the volume's free share as its bar, the free bytes as its size and `of 951.3 GB` in the last column.

File View headers: Path, Size, %, Last Modified, and with a duplicates mode Dups and Dup Size. File Types headers: Extension, Category, Files, Size, %; the extension `<No Extension>` stands for names without one.

The tab strip holds Tree View, File View, File Types, the Reset button (a painted eraser, tooltip "Reset the views: close the tree, clear the search and filters, default sorting, column widths and treemap zoom"), then the File View's Show cap, Folders, Duplicates (Off, Name and size, Name, size and date), the search box and the Name or Path radios.

Export to CSV dialog: "Exporting /path", "File to write:", checkboxes "Tab separated", "UTC timestamps", "% of parent column", "Levels below the root to include, empty for all:", Export and Cancel.

Preferences dialog (Edit, Preferences, Ctrl+comma), one row per group: At launch "Scan the home directory" (the default), "Ask for a directory", "Scan this directory:" with a path; Scanning "Cross into other local filesystems"; Treemap "Rebuild delay" in ms and "Names on tiles"; Tables "Row height" and "Size decimals"; Appearance "Follow the desktop", "Light", "Dark"; Updates "Check automatically" with daily, weekly or monthly, a "Check for Updates" button and its result ("vX is available, this is vY" with a "Release page" link, "OxTree vY is the latest release", or the error in red). Below, in small weak text, where the file is saved; then "Reset to Defaults" and "Close". Every change applies at once; the file is written on Close. The search box in the File View strip has a history dropdown after it with the last ten searches, newest first, or "No searches yet".

Details panel labels: Total Size, Items, Files, Subdirs, Size, Allocated, Last Modified, MIME Category, Permissions, Of Parent.

## Interaction

| Action | How |
|--------|-----|
| Switch view | Tree View, File View and File Types tabs above the list, Ctrl+1, Ctrl+2 and Ctrl+3 (also F8 for File Types) |
| File Types | One row per extension; click a header to sort; double click a row to list its files in the File View |
| Duplicates | The Duplicates combo in the File View strip; the list then holds only files with a duplicate, grouped, the group wasting the most space first |
| Reset | The eraser button in the tab strip: close all tree levels, default sort in every table, default column widths, search and filters cleared, Duplicates off, treemap zoom reset; the current tab and the scan stay |
| Resize columns | Drag the divider at the right edge of a header cell; the tables scroll sideways when the columns are wider than the window |
| Multi-selection | Ctrl+click toggles a row or a tile, Shift+click selects the range from the current item in the rows as shown, Ctrl+A selects every row of the tree or the File View; right-clicking inside the selection keeps it. Copy Path, Move to Trash and every cleanup act on all selected items, the cleanup once per item in turn with the confirmation listing them |
| File View | Flat list, largest first; Show cap and Folders toggle in the tab strip; click selects, double click or Enter jumps to the item in the tree |
| Search | Box in the File View strip, Ctrl+F focuses it; WizTree syntax (`*.mp3`, `>100m`, `<2024/01/01`, `=folder`, `/regex`, `\|` for OR, `!` for NOT); Name or Path radio; the first substring match is highlighted; a bad query shows its error in red under the strip |
| Select | Click a row or a tile |
| Reveal | QDirStat instead collapses everything on every treemap click and opens only the clicked path; considered and not taken, since it discards what the user opened by hand. A treemap click or a File View jump opens the path to the item temporarily; the next reveal closes the temporary rows that are not on its own path, so the tree does not fill up. Rows opened by hand in the tree stay open, and opening something by hand inside a revealed path makes that path permanent |
| Context menu | Right-click a row or a tile: copy path, trash, cleanups, treemap zoom, up one level |
| Expand or collapse | Click the arrow, double click the row, Right and Left arrow keys, Enter |
| Move selection | Up, Down, Home, End |
| Scroll the tree sideways | Shift+wheel, a trackpad, or the bar at the bottom; the header follows |
| Zoom treemap into the selection | Ctrl+Plus, View menu, double click a tile, Ctrl+wheel up |
| Zoom treemap out | Ctrl+Minus, Ctrl+wheel down |
| Reset zoom | Ctrl+0 |
| Up one level | Alt+Up |
| Close all tree levels | Ctrl+W |
| Expand all tree levels | Shift+F4 |
| Expand tree to a level | View, Expand Tree to Level, 1 to 9 |
| Names on treemap tiles | View, Names on Treemap Tiles, on by default; also in Preferences |
| Preferences | Ctrl+comma or Edit, Preferences; Reset to Defaults in the dialog resets the file too |
| Search history | The dropdown after the search box; a search is remembered when the box loses focus |
| Export to CSV | Ctrl+Alt+E for the whole tree, Export Subtree to CSV in the right-click menu for the selected directory |
| Copy the tree to the clipboard | Ctrl+Alt+C: the rows as shown, indented |
| Open directory | Ctrl+O |
| Refresh | F5 |
| Copy path | Ctrl+C |
| Move to trash | Delete |
| Cleanup hotkeys | From the configuration, Ctrl+G, Ctrl+T, Ctrl+Y, Ctrl+Delete in the standard set |
| Quit | Ctrl+Q |

Keyboard shortcuts are ignored while a text field has focus.
