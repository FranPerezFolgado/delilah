"""Sample RSS and CPU of process groups.

usage: sample.py SECONDS INTERVAL label=pid[+pid...][:tree|:self][~exclude_pid,...] ...
CPU is derived from cputime deltas (100% = one core).
"""
import subprocess, sys, time, statistics

def snap():
    out = subprocess.run(["ps", "-axo", "pid=,ppid=,rss=,time=,comm="], capture_output=True, text=True).stdout
    procs = {}
    for line in out.splitlines():
        parts = line.split(None, 4)
        if len(parts) < 5:
            continue
        pid, ppid, rss, t, comm = parts
        m, s = t.split(":")[-2:]
        h = t.split(":")[0] if t.count(":") == 2 else 0
        secs = int(h) * 3600 + int(m) * 60 + float(s)
        procs[int(pid)] = (int(ppid), int(rss), secs, comm)
    return procs

def members(procs, roots, tree, exclude):
    found = set(r for r in roots if r in procs)
    if tree:
        changed = True
        while changed:
            changed = False
            for pid, (ppid, *_r) in procs.items():
                if ppid in found and pid not in found and pid not in exclude:
                    found.add(pid); changed = True
    return found - exclude

def parse(spec):
    label, rest = spec.split("=", 1)
    exclude = set()
    if "~" in rest:
        rest, ex = rest.split("~", 1)
        exclude = set(int(x) for x in ex.split(","))
    tree = True
    if rest.endswith(":self"):
        tree, rest = False, rest[:-5]
    elif rest.endswith(":tree"):
        rest = rest[:-5]
    return label, [int(x) for x in rest.split("+")], tree, exclude

def main():
    dur, iv = float(sys.argv[1]), float(sys.argv[2])
    groups = [parse(s) for s in sys.argv[3:]]
    data = {g[0]: {"rss": [], "cpu": [], "n": []} for g in groups}
    prev = snap(); t0 = time.time()
    while time.time() - t0 < dur:
        time.sleep(iv)
        cur = snap()
        for label, roots, tree, ex in groups:
            mem = members(cur, roots, tree, ex)
            rss = sum(cur[p][1] for p in mem) / 1024
            cpu = sum(cur[p][2] - prev[p][2] for p in mem if p in prev) / iv * 100
            data[label]["rss"].append(rss); data[label]["cpu"].append(cpu); data[label]["n"].append(len(mem))
        prev = cur
    print(f"{'group':28} {'procs':>5} {'RSS avg MB':>10} {'RSS max':>8} {'CPU avg %':>9} {'CPU p95':>8}")
    for label, d in data.items():
        cpu = sorted(d["cpu"]); p95 = cpu[int(len(cpu) * .95) - 1] if cpu else 0
        print(f"{label:28} {max(d['n']):>5} {statistics.mean(d['rss']):>10.0f} {max(d['rss']):>8.0f} {statistics.mean(cpu):>9.1f} {p95:>8.1f}")

main()
