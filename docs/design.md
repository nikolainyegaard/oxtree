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
| Name column minimum width | 400 | OxTree; narrower windows scroll the tree sideways |

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
| Trash | `Moved to trash: /path` or `Move to trash failed: reason` |
| Clipboard | `Copied to clipboard: /path` |
| Cache | `Read cache file /path`, `Can't read cache file "/path": reason` |

Tree view column headers: Name, Subtree Percentage, %, Size, Items, Files, Subdirs, Last Modified. Directories that could not be read show `[Not Read]` in red in the percentage column; skipped mount points show `[Mount Point]`.

Details panel labels: Total Size, Items, Files, Subdirs, Size, Allocated, Last Modified, MIME Category, Permissions, Of Parent.

## Interaction

| Action | How |
|--------|-----|
| Switch view | Tree View and File View tabs above the list, Ctrl+1 and Ctrl+2 |
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
| Open directory | Ctrl+O |
| Refresh | F5 |
| Copy path | Ctrl+C |
| Move to trash | Delete |
| Cleanup hotkeys | From the configuration, Ctrl+G, Ctrl+T, Ctrl+Y, Ctrl+Delete in the standard set |
| Quit | Ctrl+Q |

Keyboard shortcuts are ignored while a text field has focus.
