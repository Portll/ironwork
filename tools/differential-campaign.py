#!/usr/bin/env python3
"""Run `ironwork fuzz --differential` over a corpus of batch programs until a stated number of
CPU-hours is spent, and report how many generated inputs the interpreter and the VM ran alike
(docs/lir.md §12.3).

Each repository given, or each directory under a --corpus directory, is one repository: its
programs (.cbl, .cob, .cobol) are fuzzed with -I for each directory of its copybooks and -L for each
directory of its programs, as `ironwork fuzz` is given them for a CALL to reach its subprogram. A
program is a candidate where it has a PROCEDURE DIVISION, no EXEC CICS, and a SELECT, an ACCEPT or a
PROCEDURE DIVISION USING; a program whose bytes match one already taken is a copy and is not run
again. Each candidate is probed with PROBE_RUNS runs: one fuzz refuses (nothing to vary, a USING
that is not a PARM, a source it cannot compile) or whose lowering is refused leaves the campaign,
with its reason in campaign.json, and so does one none of whose probe runs was compared, as each
timed out or reached what the VM does not run yet.

The campaign then runs rounds. Round k runs every program left with --seed k and --runs RUNS, and
rounds go on until the CPU time spent reaches the budget or --rounds is reached. CPU time is user
and system time from wait4 on each fuzz process, which holds its run processes' time as well, so a
loaded machine spends the same CPU-hours more slowly and the figure does not change with load.
The probes count towards it.

Each fuzz output with a divergence is kept as divergences/<program>/seed-<k>; the others are
removed. campaign.json holds the ironwork version, the budget and what was spent, and the totals:
runs, agree, both at the statement limit, stopped by the VM (and at what), differ, with each
program's own.

A line on standard error every PROGRESS seconds says what has been spent and counted so far.

usage: differential-campaign.py <ironwork binary> [repository]... [--corpus dir]... -o <dir>
                                [--cpu-hours H] [--jobs N] [--runs N] [--rounds N]
                                [--timeout seconds] [--shard i/n] [--label text]
       differential-campaign.py total <campaign.json>...

--shard i/n takes the i-th of n parts of the candidates, in the order of their digests, so n
machines can share a corpus. `total` adds up the campaign.json of each shard and prints the totals
as JSON. Exit status: 0 nothing differed, 1 some input differed, 2 usage.
"""
import argparse, concurrent.futures, hashlib, json, os, re, shutil, signal, socket, subprocess, sys, threading, time

PROGRAM = (".cbl", ".cob", ".cobol")
COPYBOOK = (".cpy", ".copy", ".cbk", ".inc")
PROBE_RUNS = 2
# A repository with more directories than this is given only its first ones, as corpus runs are.
DIRS = 30
SUMMARY = re.compile(r"^ironwork fuzz --differential: (\d+) runs, (\d+) agree \((\d+) at the statement limit\), (\d+) timed out, (\d+) stopped by the VM, (\d+) differ \((\d+) kept\)", re.M)
STOPPED = re.compile(r"^ironwork fuzz: (\d+) runs reached what the VM does not run yet: (.*)$", re.M)
TOTALS = ("runs", "agree", "atLimit", "timedOut", "stopped", "differ")
PROGRESS = 300

def walk(top):
    for dirpath, dirs, files in os.walk(top):
        dirs[:] = sorted(d for d in dirs if d not in (".git", "node_modules"))
        yield from (os.path.join(dirpath, f) for f in sorted(files))

def candidates(repository):
    files = list(walk(repository))
    copy_dirs = sorted({os.path.dirname(f) for f in files if f.lower().endswith(COPYBOOK)})[:DIRS]
    program_dirs = sorted({os.path.dirname(f) for f in files if f.lower().endswith(PROGRAM)})[:DIRS]
    for f in files:
        if not f.lower().endswith(PROGRAM):
            continue
        try:
            if os.path.getsize(f) > 2_000_000:
                continue
            data = open(f, "rb").read()
        except OSError:
            continue
        text = data.decode("latin-1")
        if not re.search(r"PROCEDURE\s+DIVISION", text, re.I) or re.search(r"EXEC\s+CICS", text, re.I):
            continue
        if not re.search(r"\bSELECT\b|\bACCEPT\b|PROCEDURE\s+DIVISION\s+USING", text, re.I):
            continue
        yield {"path": f, "repository": repository, "digest": hashlib.sha256(data).hexdigest(), "copyDirs": copy_dirs, "programDirs": program_dirs}

def key(program):
    rel = os.path.relpath(program["path"], os.path.dirname(program["repository"]))
    return re.sub(r"[^A-Za-z0-9._-]", "_", rel)

def fuzz(binary, program, out, runs, seed, timeout):
    """One `ironwork fuzz --differential`: its exit status, CPU seconds, and its output text."""
    argv = [binary, "fuzz", "--differential", program["path"], "-o", out, "--root", program["repository"], "--runs", str(runs), "--seed", str(seed), "--timeout", str(timeout)]
    for d in program["copyDirs"]:
        argv += ["-I", d]
    for d in program["programDirs"]:
        argv += ["-L", d]
    log = out + ".log"
    with open(log, "wb") as sink:
        child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=sink, stderr=subprocess.STDOUT, start_new_session=True)
        # Minimizing what differs spends run pairs beyond --runs; the cap only ends a fuzz that hangs.
        cap = threading.Timer((runs + 2_100) * 2 * timeout, lambda: os.killpg(child.pid, signal.SIGKILL))
        cap.start()
        _, status, usage = os.wait4(child.pid, 0)
        cap.cancel()
        child.returncode = os.waitstatus_to_exitcode(status)
    text = open(log, encoding="utf-8", errors="replace").read()
    os.remove(log)
    return child.returncode, usage.ru_utime + usage.ru_stime, text

def counted(text):
    m = SUMMARY.search(text)
    if not m:
        return None
    got = dict(zip(TOTALS, map(int, m.groups())))
    got["stoppedAt"] = {w: int(n) for n, w in STOPPED.findall(text)}
    return got

def refusal(text):
    lines = [l for l in text.strip().splitlines() if l.strip()]
    said = next((l for l in reversed(lines) if l.startswith("ironwork fuzz: ")), lines[-1] if lines else "no output")
    return re.sub(r"^ironwork fuzz: ", "", said)[:240]

def total(paths):
    shards = [json.load(open(p)) for p in paths]
    out = {"campaigns": len(shards), "ironwork": sorted({s["ironwork"] for s in shards}), "labels": sorted({s["label"] for s in shards})}
    for k in ("cpuHours", "candidates", "fuzzed", "refused", *TOTALS):
        out[k] = round(sum(s[k] for s in shards), 3)
    out["wallHours"] = max(s["wallHours"] for s in shards)
    stopped_at = {}
    for s in shards:
        for w, n in s["stoppedAt"].items():
            stopped_at[w] = stopped_at.get(w, 0) + n
    out["stoppedAt"] = dict(sorted(stopped_at.items(), key=lambda kv: -kv[1]))
    out["divergences"] = [d for s in shards for d in s["divergences"]]
    print(json.dumps(out, indent=2))
    return 1 if out["differ"] else 0

def main():
    if sys.argv[1:2] == ["total"]:
        return total(sys.argv[2:])
    ap = argparse.ArgumentParser(usage=__doc__.split("usage: ")[1].split("\n\n")[0])
    ap.add_argument("binary")
    ap.add_argument("repositories", nargs="*")
    ap.add_argument("--corpus", action="append", default=[])
    ap.add_argument("-o", dest="out", required=True)
    ap.add_argument("--cpu-hours", type=float, default=1.0)
    ap.add_argument("--jobs", type=int, default=os.cpu_count())
    ap.add_argument("--runs", type=int, default=50)
    ap.add_argument("--rounds", type=int, default=1_000_000)
    ap.add_argument("--timeout", type=float, default=60)
    ap.add_argument("--shard", default="1/1")
    ap.add_argument("--label", default="")
    a = ap.parse_args()
    shard, shards = map(int, a.shard.split("/"))
    if not 1 <= shard <= shards:
        ap.error("--shard i/n takes 1 <= i <= n")
    repositories = list(a.repositories)
    for c in a.corpus:
        repositories += [os.path.join(c, d) for d in sorted(os.listdir(c)) if os.path.isdir(os.path.join(c, d))]
    if not repositories:
        ap.error("no repository given")
    if os.path.exists(a.out) and os.listdir(a.out):
        ap.error(f"-o {a.out} is not empty")
    binary = os.path.abspath(a.binary)
    os.makedirs(os.path.join(a.out, "work"))

    taken, copies = {}, 0
    for r in repositories:
        for p in candidates(os.path.abspath(r)):
            if p["digest"] in taken:
                copies += 1
            else:
                taken[p["digest"]] = p
    programs = [taken[d] for d in sorted(taken)][shard - 1::shards]

    budget = a.cpu_hours * 3600
    began, load_before = time.time(), os.getloadavg()
    lock = threading.Lock()
    state = {"cpu": 0.0, "said": time.time()}
    totals = {k: 0 for k in TOTALS}
    stopped_at, per_program, refused, divergences = {}, {}, {}, []

    def spend(seconds):
        with lock:
            state["cpu"] += seconds

    def spent():
        with lock:
            return state["cpu"] >= budget

    def one(program, runs, seed):
        name = key(program)
        out = os.path.join(a.out, "work", f"{name}.seed-{seed}")
        code, cpu, text = fuzz(binary, program, out, runs, seed, a.timeout)
        spend(cpu)
        got = counted(text)
        with lock:
            if got:
                row = per_program.setdefault(name, {"path": program["path"], **{k: 0 for k in TOTALS}, "cpuSeconds": 0.0, "seeds": 0})
                for k in TOTALS:
                    row[k] += got[k]
                    totals[k] += got[k]
                row["cpuSeconds"] = round(row["cpuSeconds"] + cpu, 3)
                row["seeds"] += 1
                for w, n in got["stoppedAt"].items():
                    stopped_at[w] = stopped_at.get(w, 0) + n
            if got and got["differ"]:
                kept = os.path.join(a.out, "divergences", name, f"seed-{seed}")
                os.makedirs(os.path.dirname(kept), exist_ok=True)
                shutil.move(out, kept)
                divergences.append({"program": program["path"], "seed": seed, "differ": got["differ"], "kept": os.path.relpath(kept, a.out), "said": [l for l in text.splitlines() if "divergence-" in l]})
            else:
                shutil.rmtree(out, ignore_errors=True)
            if time.time() - state["said"] >= PROGRESS:
                state["said"] = time.time()
                print(f"{time.strftime('%H:%M:%S')} {state['cpu'] / 3600:.3f} of {a.cpu_hours} CPU-hours, seed {seed}: "
                      f"{totals['runs']} runs, {totals['agree']} agree, {totals['timedOut']} timed out, {totals['stopped']} stopped, {totals['differ']} differ", file=sys.stderr, flush=True)
        return code, got, text

    left = []
    with concurrent.futures.ThreadPoolExecutor(a.jobs) as pool:
        probes = {pool.submit(one, p, PROBE_RUNS, 1): p for p in programs}
        for f in concurrent.futures.as_completed(probes):
            p = probes[f]
            code, got, text = f.result()
            if got is None:
                refused[key(p)] = {"path": p["path"], "exit": code, "why": refusal(text)}
            elif got["agree"] or got["differ"]:
                left.append(p)
        left.sort(key=lambda p: p["digest"])
        work = ((p, seed) for seed in range(2, a.rounds + 2) for p in left)
        running, last_seed = {}, 1
        while True:
            while len(running) < a.jobs and not spent():
                p, seed = next(work, (None, None))
                if p is None:
                    break
                running[pool.submit(one, p, a.runs, seed)] = seed
            if not running:
                break
            done, _ = concurrent.futures.wait(running, return_when=concurrent.futures.FIRST_COMPLETED)
            for f in done:
                f.result()
                last_seed = max(last_seed, running.pop(f))
        rounds = last_seed - 1

    wall = time.time() - began
    report = {
        "label": a.label,
        "ironwork": subprocess.run([binary, "--version"], capture_output=True, text=True).stdout.strip(),
        "host": {"name": socket.gethostname(), "cpus": os.cpu_count(), "loadBefore": [round(x, 1) for x in load_before], "loadAfter": [round(x, 1) for x in os.getloadavg()]},
        "shard": a.shard,
        "budgetCpuHours": a.cpu_hours,
        "cpuHours": round(state["cpu"] / 3600, 3),
        "wallHours": round(wall / 3600, 3),
        "jobs": a.jobs,
        "runsPerRound": a.runs,
        "rounds": rounds,
        "timeoutSeconds": a.timeout,
        "repositories": len(repositories),
        "candidates": len(programs),
        "copiesSkipped": copies,
        "fuzzed": len(left),
        "refused": len(refused),
        **totals,
        "stoppedAt": dict(sorted(stopped_at.items(), key=lambda kv: -kv[1])),
        "divergences": divergences,
        "programs": dict(sorted(per_program.items())),
        "refusedPrograms": dict(sorted(refused.items())),
    }
    shutil.rmtree(os.path.join(a.out, "work"), ignore_errors=True)
    with open(os.path.join(a.out, "campaign.json"), "w") as f:
        json.dump(report, f, indent=2)
        f.write("\n")
    print(f"differential campaign: {report['cpuHours']} CPU-hours over {report['wallHours']} hours, {len(left)} programs, "
          f"{totals['runs']} runs: {totals['agree']} agree ({totals['atLimit']} at the statement limit), {totals['timedOut']} timed out, "
          f"{totals['stopped']} stopped by the VM, {totals['differ']} differ")
    return 1 if totals["differ"] else 0

if __name__ == "__main__":
    sys.exit(main())
