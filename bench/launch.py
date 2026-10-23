#!/usr/bin/env python3
"""Launch a Wayland program and measure it from outside.

    bench/launch.py [--runs N] [--quiet-ms 600] -- PROGRAM [ARGS...]

For each run it prints, in milliseconds from the exec:
  first_frame  the first wl_surface commit with a buffer attached, from
               WAYLAND_DEBUG, which the client library prints for any toolkit
  idle         when the process stopped burning CPU: the last tick of CPU time
               before a quiet window, from /proc/PID/stat at 10 ms polls; for a
               directory scanner that is the scan, the tree and the treemap done
and the memory at that point from /proc/PID: rss, pss (what the process adds
to the machine, shared libraries divided among their users) and hwm (the peak).
The program is killed after each run. Nothing in it needs root; drop the page
cache between runs yourself for cold numbers.
"""

import os
import re
import statistics
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone

POLL = 0.01


def parse_args(argv):
    runs, quiet = 3, 0.6
    i = 0
    while i < len(argv):
        if argv[i] == "--runs":
            runs = int(argv[i + 1])
            i += 2
        elif argv[i] == "--quiet-ms":
            quiet = int(argv[i + 1]) / 1000
            i += 2
        elif argv[i] == "--":
            i += 1
            break
        else:
            break
    cmd = argv[i:]
    if not cmd:
        sys.exit(__doc__)
    return runs, quiet, cmd


def cpu_ticks(pid):
    with open(f"/proc/{pid}/stat") as f:
        after = f.read().rsplit(")", 1)[1].split()
    return int(after[11]) + int(after[12])


def memory_mb(pid):
    out = {}
    with open(f"/proc/{pid}/status") as f:
        for line in f:
            if line.startswith(("VmRSS:", "VmHWM:")):
                out[line[2:5].lower()] = int(line.split()[1]) / 1024
    with open(f"/proc/{pid}/smaps_rollup") as f:
        for line in f:
            if line.startswith("Pss:"):
                out["pss"] = int(line.split()[1]) / 1024
    return out


def first_frame(log_path, started):
    """Seconds from `started` to the first commit of a surface with a buffer."""
    attached = set()
    today = datetime.fromtimestamp(started, timezone.utc).date()
    with open(log_path, errors="replace") as f:
        for line in f:
            m = re.match(r"\[(\d\d):(\d\d):(\d\d)\.(\d{6})\].*-> (wl_surface#\d+)\.(\w+)\(", line)
            if not m:
                continue
            h, mi, s, us, surface, call = m.groups()
            if call == "attach":
                attached.add(surface)
            elif call == "commit" and surface in attached:
                t = datetime(today.year, today.month, today.day, int(h), int(mi), int(s), int(us), timezone.utc)
                return t.timestamp() - started
    return None


def one_run(cmd, quiet):
    env = dict(os.environ, WAYLAND_DEBUG="1")
    with tempfile.NamedTemporaryFile(prefix="wl-", suffix=".log", delete=False) as log:
        log_path = log.name
    with open(log_path, "w") as err:
        started = time.time()
        proc = subprocess.Popen(cmd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=err)
        # CPU time arrives in 10 ms ticks. Ticks less than 100 ms apart form
        # a burst; a burst of at least BUSY ticks is work (a scan, a treemap),
        # anything smaller is a repaint or a timer and does not count. Idle
        # is the end of the last piece of work once `quiet` seconds have
        # passed without another one.
        BUSY = 5
        bursts = []  # (first tick time, last tick time, ticks)
        last = cpu_ticks(proc.pid)
        idle_at = None
        while proc.poll() is None:
            time.sleep(POLL)
            now = time.time()
            try:
                ticks = cpu_ticks(proc.pid)
            except FileNotFoundError:
                break
            if ticks > last:
                if bursts and now - bursts[-1][1] < 0.1:
                    f, _, n = bursts[-1]
                    bursts[-1] = (f, now, n + ticks - last)
                else:
                    bursts.append((now, now, ticks - last))
                last = ticks
            work = [b for b in bursts if b[2] >= BUSY]
            ongoing = bursts and now - bursts[-1][1] < 0.1 and bursts[-1][2] < BUSY
            if work and not ongoing and now - work[-1][1] > quiet:
                idle_at = work[-1][1]
                mem = memory_mb(proc.pid)
                break
        if proc.poll() is not None:
            os.unlink(log_path)
            return None
        ff = first_frame(log_path, started)
        proc.terminate()
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
    os.unlink(log_path)
    return {
        "first_frame": (ff or 0) * 1000,
        "idle": (idle_at - started) * 1000,
        **mem,
    }


def main():
    runs, quiet, cmd = parse_args(sys.argv[1:])
    results = []
    for i in range(runs):
        r = one_run(cmd, quiet)
        if r is None:
            print(f"run {i + 1}: the program exited before it went idle", file=sys.stderr)
            continue
        results.append(r)
        print(f"run {i + 1}: first frame {r['first_frame']:7.1f} ms  idle {r['idle']:8.1f} ms  rss {r['rss']:6.1f} MB  pss {r['pss']:6.1f} MB  peak {r['hwm']:6.1f} MB")
        time.sleep(1)
    if len(results) > 1:
        med = {k: statistics.median(r[k] for r in results) for k in results[0]}
        print(f"median: first frame {med['first_frame']:7.1f} ms  idle {med['idle']:8.1f} ms  rss {med['rss']:6.1f} MB  pss {med['pss']:6.1f} MB  peak {med['hwm']:6.1f} MB")


if __name__ == "__main__":
    main()
