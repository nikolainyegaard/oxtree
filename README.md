# OxTree

Directory statistics for Linux: where the disk space went, shown as a tree and as a cushion treemap, with cleanup actions to do something about it. A Rust reimplementation of the QDirStat idea in a single small binary.

OxTree scans a directory on all cores, draws the result as a sortable tree with percent bars and as a treemap where every file is a rectangle sized by its allocated space and coloured by type. Click a rectangle and the tree jumps to the file. Pick a cleanup from the menu to open a terminal there, check the file type, delete junk, or run your own command. Move things to the trash with one key.

## Status

Version 0.1.1 covers the daily use: scan, tree, treemap, details, cleanups, trash, QDirStat cache files, a File View with search, a File Types view, duplicates, CSV export, multi-selection and a Preferences dialog. Not there yet: rename, a file age view, exclude rule and category editors. Cleanups and exclude rules are edited in their configuration files.

## Install

Each release on the [releases page](https://github.com/nikolainyegaard/oxtree/releases) has, for x86_64 and aarch64: a tarball with the binary, a `.deb`, an `.rpm` and an AppImage. All are built against glibc 2.28, so they run on RHEL 8, Debian 10, Ubuntu 18.10 and anything newer. The packages install the desktop entry, the icon and the polkit action for Rescan as Root; the tarball and the AppImage work from anywhere, and Rescan as Root then goes through polkit's generic prompt. Only Fedora on x86_64 has been tested by the author; reports from elsewhere are welcome.

## Build

Rust 1.88 or newer and a working OpenGL driver.

```
cargo build --release
```

The binary is `target/release/oxtree`, about 9 MB, with no dependencies beyond the system's C library and Mesa.

## Use

```
oxtree /usr
oxtree --cache ~/root.cache.gz
oxtree
oxtree /usr --export usr.csv --max-depth 2
```

The last form scans without a window and writes WizTree's CSV columns (`--tsv`, `--utc`, `--percent`, `--search QUERY` to filter, `-` for stdout).

Without arguments OxTree scans your home directory straight away; Preferences can make it ask for a directory instead, or scan one of your choosing. The Open Directory dialog (Ctrl+O) lists every mounted local filesystem with its size, used and free space and a Scan button, and the Browse button uses `kdialog` or `zenity` when one is installed. A scan crosses into other local filesystems by default, so a scan of `/` covers the machine; kernel tables, network mounts and removable media under `/run` stay out. The checkbox in the dialog turns crossing off, which is the view to use when the question is which filesystem is full. Directories you cannot read are listed as `[Not Read]`; File, Rescan as Root replaces the window with OxTree running as root on the same directory, asking for your password through polkit.

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
| Ctrl+1, Ctrl+2, Ctrl+3 | Tree View, File View (all files, largest first, with a Show cap and a duplicates mode), File Types (by extension) |
| Ctrl+click, Shift+click, Ctrl+A | Select several items; trash, cleanups and copy then act on all of them |
| Shift+F4, Ctrl+W | Expand all tree levels, close them |
| Ctrl+Alt+E, Ctrl+Alt+C | Export to CSV, copy the tree as shown to the clipboard |
| Ctrl+F | Search in the File View: `*.mp3`, `>100m`, `<2024/01/01`, `>=today-7`, `=folder`, `/regex`; space is AND, `\|` is OR, `!` is NOT |
| F9 | Show or hide the treemap |
| Ctrl+C | Copy the path |
| Delete | Move to trash |
| Ctrl+G, Ctrl+T, Ctrl+Y, Ctrl+Delete | Open file manager, open terminal, check file type, delete (from the standard cleanups) |
| Ctrl+comma | Preferences: cross filesystems, treemap rebuild delay, names on tiles, row height, size decimals, theme, update check |
| Ctrl+Q | Quit |

`.snapshot` directories are skipped. Settings live in `~/.config/oxtree/settings.conf`, written when the Preferences dialog closes and on exit; column widths, the File View choices and the search history go there too. OxTree never touches the network unless the update check is turned on in Preferences or its button is pressed; a check is one request to the GitHub releases API through `curl`. When it finds a newer release, Update Now installs it the way this copy was installed: through dnf or apt with the desktop's password prompt for a package, in place for an AppImage, after checking the download against the release's checksums. A copy from a repository also updates with the rest of the system, as usual. Cleanups come from `~/.config/oxtree/cleanups.conf`, or from QDirStat's own cleanup file when OxTree has none yet. The format and the standard set are described in [docs/cleanups.md](docs/cleanups.md).

Cache files are QDirStat's format, so a scan written on a server with `qdirstat-cache-writer` opens here, and a file written here opens in QDirStat.

## Development

```
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo build --release --features diagnostics
target/release/oxtree-bench /usr
target/release/oxtree /usr --screenshot shot.ppm
```

The `diagnostics` feature adds the benchmark binary, the screenshot mode and the `OXTREE_STARTUP` and `OXTREE_STATS` switches. A plain release build has none of them.

The workspace forbids `unsafe` and denies `unwrap`, `expect`, `panic` and indexing outside tests. Reference documentation is in [docs/](docs/): [architecture](docs/architecture.md), [design](docs/design.md), [performance](docs/performance.md), [cleanups](docs/cleanups.md), [gotchas](docs/gotchas.md).

## Credits and licence

GPL-2.0, see [LICENSE](LICENSE). The treemap algorithm and its constants, the category table, the standard cleanups and the cache file format come from [QDirStat](https://github.com/shundhammer/qdirstat) by Stefan Hundhammer; see [NOTICE](NOTICE). QDirStat in turn builds on the squarified and cushion treemap papers by Bruls, Huizing, van Wijk and van de Wetering.

How far this is from WizTree, feature by feature, is tracked in [docs/wiztree-parity.md](docs/wiztree-parity.md).
