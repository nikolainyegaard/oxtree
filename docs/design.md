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
| Status message timeout | 3 s | `StatusBarTimeoutMillisec` |
| Row height | 20 | OxTree |

Category colours live in `crates/core/src/mime.rs` with the table: Junk red, Compressed Archives green, Uncompressed Archives olive, Compressed Files green, Images cyan, Uncompressed Images red, Videos `#a0ff00`, Music yellow, Documents blue, Source Files cyan, Object or Generated Files `#ffa000`, Libraries `#ffa000`, Executables magenta, Symbolic Links blue.

## Copy

Status bar states, taken from QDirStat where it had them:

| State | Text |
|-------|------|
| Scanning | `Reading /usr...` then `Reading... 123 456 items in 12 345 directories, 4.2 GB` |
| Finished | `Finished. Elapsed time: 0.125 sec` plus `[N directories could not be read]` when applicable |
| Aborted | `Reading aborted.` |
| Selection | `/usr/lib64  6.1 GB` |
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
| Select | Click a row or a tile |
| Context menu | Right-click a row or a tile: copy path, trash, cleanups, treemap zoom, up one level |
| Expand or collapse | Click the arrow, double click the row, Right and Left arrow keys, Enter |
| Move selection | Up, Down, Home, End |
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
