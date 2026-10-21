# Performance

Measured on the development laptop: 14 cores, Intel Meteor Lake graphics, one NVMe drive, btrfs with zstd compression, warm page cache unless stated. Cold-cache numbers need root to drop caches and have not been taken.

## Budgets

Checked in the change checklist. A change that moves one says so in the commit body.

| Measure | Budget | 0.1.0 | How to measure |
|---------|--------|-------|----------------|
| Stripped release binary | 10 MB | 8.4 MB | `ls -l target/release/oxtree` |
| Scan of /usr, 352k nodes, warm | 150 ms | 125 ms | `target/release/oxtree --bench /usr` |
| Treemap layout plus render at 1900x600 | 15 ms | 1.7 + 7.7 ms | same |
| First frame | 100 ms | 75 ms | `target/release/oxtree /usr 2>&1 \| grep 'first frame'` |
| RSS with /usr loaded and the window open | 120 MB | 133 MB (115 MB with `MALLOC_ARENA_MAX=2`) | `grep -E '^(Rss\|Pss\|Anonymous)' /proc/$(pgrep -n -x oxtree)/smaps_rollup` |
| Tree memory per node | 56 bytes | about 55 bytes | arithmetic on the arena arrays, see below |

RSS is over budget in 0.1.0. See the memory section for where it goes and the planned fix.

## Where the time goes

| Phase | 0.1.0 | Note |
|-------|-------|------|
| Scan, 14 workers | 125 ms | Was 716 ms until the progress loop stopped sleeping through the finish; see gotchas |
| Scan, 1 worker (spike) | 400 ms | 83% kernel time: the walk is syscall bound and scales with cores |
| Layout | 1.7 ms | Children are pre-sorted, so this is arithmetic only; the spike sorted and took 5 ms |
| Render | 7.7 ms | 14 bands on 14 threads; the spike took 21 ms single-threaded |
| Texture upload | about 1 ms | |
| Cache write / parse, 352k nodes | 89 ms / 111 ms | Text format, 14.8 MB |
| First frame | 75 ms | eframe on glow under Wayland; the spike measured 43 ms without menus and panels |

Installed QDirStat 2.0 for comparison: read of /usr logged at 41.6 s, RSS 97 MB after that scan, 1.8 MB binary plus five Qt6 libraries.

## Where the memory goes

With /usr loaded (352 101 nodes) and the window open:

| Mapping | RSS |
|---------|-----|
| Anonymous | 84 MB |
| libLLVM and libgallium (Mesa, shared with every GL process) | 37 MB |
| The binary | 8 MB |
| libc, libstdc++, xkbcommon, X11, EGL | 5 MB |

The arena itself is small: per node 4 (parent) + 4 (first child) + 4 (count) + 8 (size) + 8 (mtime) + 4 (mode) + 1 (kind) + 12 (items, files, subdirs) + 4 (name offset) = 49 bytes, plus about 19 bytes of name, about 68 bytes per node or 24 MB for /usr. The rest of the anonymous memory is what glibc kept after the scan: every worker thread allocated hundreds of thousands of small `String`s for names that were freed on the main thread after `Tree::build`, and glibc's per-thread arenas do not hand that back. Running with `MALLOC_ARENA_MAX=2` cuts anonymous memory from 84 MB to 65 MB with no code change, which confirms the diagnosis.

Planned fix, in order:

1. Per-worker name buffers. `DirRecord` carries one `String` per directory holding all its names and `FileRecord` holds offsets, so a worker makes one allocation per directory instead of one per file. Expected to remove most of the retained memory and to speed the scan up as well.
2. Drop `items`, `files` and `subdirs` for non-directory nodes by moving them to a directory-only side table indexed through the node. Saves about 10 bytes per node.
3. Only then consider the allocator. `mallopt` needs `unsafe`, which the workspace forbids, so the lever would be the environment variable in the desktop file.

The Mesa share is not ours to cut. PSS is the honest number for it; on a desktop that already runs a GL compositor those pages are resident anyway.

## Scan design

Facts behind the walker, so nobody re-derives them:

- The kernel's dentry and inode caches scale across cores for lookups; readdir on one directory serialises on that directory's inode lock. Parallelism is therefore per directory, and the queue has to hand out whole directories.
- std's `DirEntry::metadata` on Linux already stats relative to the directory fd with `AT_SYMLINK_NOFOLLOW`, and `DirEntry::file_type` reads `d_type`. Hand-rolled getdents64 and statx gain nothing; the spacebadger project measured a 64 KiB getdents buffer within noise and io_uring statx 1.5 to 3x slower warm, because the kernel sends statx to io-wq threads.
- Sorting entries by inode number before stat helps ext4 on rotating disks (Ts'o's advice, adopted by gnulib's fts) and nothing else. Not implemented; conditional on `ROTA` from sysfs if anyone with an HDD asks.
- dua-cli replaced jwalk and rayon with a crate-local work-stealing walker on std threads for up to 40% more throughput. OxTree's shared `Mutex<Vec>` queue is the simple version of that; a per-worker deque is the next step if the lock shows up in a profile.
- Hard links: not deduplicated yet. The fix is a `(dev, ino)` set consulted only when `nlink > 1`.

## Treemap design

- Layout is a traversal because the arena stores children largest first. One scratch `Vec` is reused for rows.
- Shading splits the pixel rows into one band per thread with `chunks_mut`, each thread walking every tile and clipping to its band. No unsafe, no shared mutation, near-linear scaling.
- The GPU route (a vertex buffer of rectangles plus cushion coefficients, shading in a fragment shader through a glow paint callback) would make a resize cost layout plus an upload at any window size and remove the texture. Not needed at 7.7 ms for a 1900x600 map; worth it for 4K.
- During a resize the treemap is rebuilt every frame, one job in flight at a time; the previous texture is drawn at its own pixel size, anchored top left and clipped, never stretched. Measured on a five second resize drag with `OXTREE_STATS=1`: about 55 rebuilds a second at 110 to 135% of one core, idle unaffected. A QDirStat-style settle delay was tried and measured at one rebuild a second and 5 to 9%, and rejected: a drag lasts a second or two, and the per-frame rebuild is what makes the speed visible. `OXTREE_REBUILD_DELAY_MS=200` turns the delay on until it becomes a preference; see the pinned item in gotchas.md. Rebuilds are capped at 120 a second (`TREEMAP_MAX_REBUILD_HZ`) so a 500 Hz display cannot multiply the cost; 120 rather than 60 because a 60 cap skips every other frame on a 60 Hz screen whenever the timer lands early.

## Launch

Nothing runs before the first frame except reading the cleanup configuration and `date +%z`. The scan starts from the constructor on its own threads. The release profile is `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`. The dynamic loader is five shared objects and under 300 relocations, which is noise.

## Dead ends

Recorded in `gotchas.md` under the spike date, with the sources in the spike's own notes. Short version: io_uring statx, custom directory syscalls, reading filesystem metadata tables the way WizTree reads the MFT, allocator swaps without a measurement, and a software renderer to shed Mesa.
