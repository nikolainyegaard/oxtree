# OxTree against QDirStat

Both programs measured from outside, on the same machine, on the same files, the same afternoon. Nothing in either program was changed or instrumented for this; the numbers come from the kernel and the Wayland socket.

## Method

`bench/launch.py` starts a program with `WAYLAND_DEBUG=1`, polls `/proc/PID/stat` every 10 ms, and reports:

- **first frame**: the first `wl_surface.commit` with a buffer attached, which the client library prints for any toolkit. The window is on screen from that moment.
- **done**: when the process stops working. CPU time arrives in 10 ms ticks across all threads; ticks less than 100 ms apart form a burst, a burst of at least 50 ms is work, and "done" is the end of the last piece of work once 600 ms have passed without another. For a directory scanner that is the scan, the tree and the treemap finished. Timer repaints do not count.
- **rss, pss, peak**: from `/proc/PID` at that moment. PSS is what the process adds to the machine, with shared libraries divided among their users; it is the fair number for a Qt program on a Qt desktop.

Three runs each, the median. Each program is killed after its run and restarted for the next, so nothing is cached in the program itself; the page cache is warm throughout, which measures the programs rather than the disk. Cold-cache rows need `sudo sh -c 'sync; echo 3 > /proc/sys/vm/drop_caches'` before every run and have not been taken.

```
python3 bench/launch.py --runs 3 -- /usr/bin/oxtree /usr
python3 bench/launch.py --runs 3 -- qdirstat /usr
```

Machine: a 14 core Intel Meteor Lake laptop, one NVMe drive, btrfs with zstd compression, Fedora 45, KDE on Wayland at 60 Hz, performance power profile, on mains. OxTree 0.2.0, QDirStat 2.0 from the Fedora repository.

## Results, 2026-10-23

| Scenario | | OxTree | QDirStat | Ratio |
|---|---|---:|---:|---:|
| Empty launch (the Open Directory dialog, no scan) | first frame | 51 ms | 109 ms | 2.1x |
| | PSS | 28 MB | 32 MB | |
| `/usr`, 357 000 nodes, 14.6 GB | first frame | 58 ms | 114 ms | 2.0x |
| | done | 208 ms | 1 119 ms | 5.4x |
| | PSS | 95 MB | 158 MB | 1.7x |
| | RSS | 137 MB | 228 MB | 1.7x |
| Home directory, 981 000 nodes | first frame | 54 ms | 137 ms | 2.5x |
| | done | 649 ms | 3 092 ms | 4.8x |
| | PSS | 213 MB | 312 MB | 1.5x |
| | RSS | 256 MB | 379 MB | 1.5x |

The programs' own clocks agree with the outside view: QDirStat logs "Reading finished after 0.710 sec" for `/usr` and 2.302 sec for the home directory, OxTree's benchmark binary says 0.12 s and 0.45 s; the rest of each "done" figure is building the tree and the treemap.

Install footprint: OxTree is one 9.2 MB file with no dependencies beyond glibc and Mesa. QDirStat is 2.3 MB plus about 45 MB of Qt 6 libraries (qtbase, qtbase-gui, qtsvg), which a KDE desktop has installed already and a GNOME one may not.

## Reading the numbers

The scan ratio is parallelism: QDirStat reads one directory at a time on the UI thread, by design, so that the tree fills while it reads; OxTree reads on every core and shows the finished tree. The memory ratio is the arena against one C++ object per file. The first frame ratio is a plain window against a Qt main window with its toolbars and models.

What the numbers do not say: QDirStat has features OxTree does not, listed in [qdirstat-parity.md](qdirstat-parity.md) once it exists, and none of them is in the measured path. A package view or a file age window costs nothing until it is opened. Quote the numbers for what they measure: launching, scanning, and showing the tree and the treemap.
