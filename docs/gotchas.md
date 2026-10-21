# Gotchas and decisions

Dated notes on things that look wrong until you know why, and on dead ends. Add to this the moment something non-obvious is decided.

## 2026-10-21

- **Sizes are allocated sizes only.** The tree stores `st_blocks * 512` per node and nothing else, which is what the views show (QDirStat shows allocated size in the tree too). Sparse files therefore look small, and the cache writer emits allocated size in the size field. Writing cache version 1.0 instead of 2.0 is deliberate: 2.0 wants uid, gid and permissions, and the arena does not keep uid and gid.
- **Children are born sorted.** `Tree::build` computes subtree totals from the records before allocating nodes, so every directory's child block is written largest first and nothing ever sorts for the treemap. Sorting by another column in the tree view sorts a copy of one directory's indices when the rows are rebuilt.
- **The `<Files>` dot entry exists only when a directory has both files and subdirectories**, as in QDirStat. Its path is its parent's path; `Tree::path` skips it.
- **Scan completion used to be late by up to 333 ms** because the progress loop slept for the whole reporting interval before checking. It now polls every 5 ms and reports every 333 ms. The benchmark showed 716 ms for a scan that takes about 110 ms.
- **Excluded directories and mount points are leaf nodes**, kind `UnreadDir` or `MountPoint`, so the user sees they were skipped. Btrfs subvolumes have their own device id, so on this laptop scanning `/` stops at `/home`. QDirStat behaves the same with its default `CrossFilesystems=false`.
- **Cleanups and trash rescan the whole tree** for any refresh policy other than NoRefresh, but nothing on screen changes until the new tree lands: the old tree and texture stay, and the selection, expanded directories and treemap zoom are carried over by path (`Tree::find`, `find_nearest`) because every node id changes with the arena. A subtree rescan spliced into the arena is the upgrade if scans ever get slow enough to notice. Marked `ponytail:` in `app.rs`.
- **Local time is one offset from `date +%z`**, read once at startup. A timestamp on the other side of a daylight saving change shows an hour off. Pulling in a tz database parser for a file list did not seem worth it.
- **Hard links are counted once per link.** A file with two names inside the scanned tree is counted twice. QDirStat tracks `(dev, ino)` for `nlink > 1`; that is the fix if someone wants it.
- **No file dialog crate.** Open Directory has a text field plus Browse, which runs `kdialog --getexistingdirectory` or `zenity --file-selection --directory` as a child process. Cache files are entered as paths.
- **Treemap rebuilds run on one worker thread at a time, every frame during a resize.** A 200 ms settle delay like QDirStat's DelayedRebuilder exists behind `OXTREE_REBUILD_DELAY_MS` and is off by default: the per-frame cost measured at about 120% of one core only while dragging, and the instant rebuild is the point. The previous texture is drawn unstretched and anchored top left while a job runs; the first version stretched it, which distorted every tile and looked wrong. Results from stale generations (zoom changed meanwhile) are dropped.
- **Clippy's `indexing_slicing` is denied workspace-wide.** The arena uses `get()` everywhere and the pixel loops use `as_chunks_mut::<4>()`. The bounds checks cost the same as the ones `[]` would do; the difference is that a bad index returns a default instead of aborting the app.
- **The release profile uses `panic = "abort"`.** With every panic path denied by clippy there is nothing to unwind, and abort keeps the binary smaller.

## Pinned for the settings work

- **Treemap rebuild delay as a preference.** The settle delay (`OxTreeApp::rebuild_delay`, read from `OXTREE_REBUILD_DELAY_MS`, default 0) stays in the code so that it becomes a setting once a settings file and a Preferences dialog exist. Proposed key: `[Treemaps] RebuildDelayMillisec`, default 0, with 200 as the QDirStat-like value for slow or battery-powered machines. The environment variable goes away at that point. The 120 Hz cap is not a preference; it stays a constant.

## Dead ends from the spike (2026-10-21)

- io_uring batched statx: slower on a warm cache in every published measurement, because the kernel hands statx to io-wq threads.
- Custom getdents and statx calls: std already stats relative to the directory fd and uses `d_type`.
- Reading filesystem metadata tables directly (the WizTree approach): no Linux equivalent of the MFT; ext4 inodes have no names, btrfs tree search needs root.
- The REBUILD_STOPWATCH comparison against QDirStat's treemap build time was never run; it needs a Qt source build.
