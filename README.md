# OxTree

Directory statistics for Linux: where the disk space went, shown as a tree and as a cushion treemap, with cleanup actions to do something about it. A Rust reimplementation of the QDirStat idea in a single small binary.

OxTree scans a directory on all cores, draws the result as a sortable tree with percent bars and as a treemap where every file is a rectangle sized by its allocated space and coloured by type. Click a rectangle and the tree jumps to the file. Pick a cleanup from the menu to open a terminal there, check the file type, delete junk, or run your own command. Move things to the trash with one key.

## Status

Version 0.1.0 covers the daily use: scan, tree, treemap, details, cleanups, trash, QDirStat cache files. Not there yet: file type and size statistics, file age view, package views, filesystems list, exclude rule and category editors, a settings dialog. Cleanups and exclude rules are edited in their configuration files.

## Build

Rust 1.88 or newer and a working OpenGL driver.

```
cargo build --release
```

The binary is `target/release/oxtree`, about 8 MB, with no dependencies beyond the system's C library and Mesa.

## Use

```
oxtree /usr
oxtree --cache ~/root.cache.gz
oxtree
```

Without arguments a dialog asks for the directory. The Browse button uses `kdialog` or `zenity` when one is installed.

| Key | Action |
|-----|--------|
| Ctrl+O | Open directory |
| F5 | Rescan |
| Up, Down, Left, Right, Enter | Move in the tree, collapse, expand |
| Alt+Up | Go to the parent |
| Ctrl+Plus, Ctrl+Minus, Ctrl+0 | Zoom the treemap into the selection, out, reset |
| Ctrl+wheel over the treemap | Zoom |
| Double click a tile | Zoom into its directory |
| Right click a row or a tile | Menu with copy path, trash, cleanups and treemap zoom |
| F9 | Show or hide the treemap |
| Ctrl+C | Copy the path |
| Delete | Move to trash |
| Ctrl+G, Ctrl+T, Ctrl+Y, Ctrl+Delete | Open file manager, open terminal, check file type, delete (from the standard cleanups) |
| Ctrl+Q | Quit |

Directories on other filesystems are shown but not entered; `.snapshot` directories are skipped. Cleanups come from `~/.config/oxtree/cleanups.conf`, or from QDirStat's own cleanup file when OxTree has none yet. The format and the standard set are described in [docs/cleanups.md](docs/cleanups.md).

Cache files are QDirStat's format, so a scan written on a server with `qdirstat-cache-writer` opens here, and a file written here opens in QDirStat.

## Development

```
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
target/release/oxtree --bench /usr
target/release/oxtree /usr --screenshot shot.ppm
```

The workspace forbids `unsafe` and denies `unwrap`, `expect`, `panic` and indexing outside tests. Reference documentation is in [docs/](docs/): [architecture](docs/architecture.md), [design](docs/design.md), [performance](docs/performance.md), [cleanups](docs/cleanups.md), [gotchas](docs/gotchas.md).

## Credits and licence

GPL-2.0, see [LICENSE](LICENSE). The treemap algorithm and its constants, the category table, the standard cleanups and the cache file format come from [QDirStat](https://github.com/shundhammer/qdirstat) by Stefan Hundhammer; see [NOTICE](NOTICE). QDirStat in turn builds on the squarified and cushion treemap papers by Bruls, Huizing, van Wijk and van de Wetering.

How far this is from WizTree, feature by feature, is tracked in [docs/wiztree-parity.md](docs/wiztree-parity.md).
