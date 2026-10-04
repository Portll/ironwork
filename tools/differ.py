#!/usr/bin/env python3
"""Run each COBOL program under `ironwork run` and compiled by gcobol, or with --cobc by GnuCOBOL's
cobc, and report where the two disagree: what DISPLAY wrote, the return code, or which one abended.

Neither compiler is an oracle. Both keep storage in ASCII and have their own numeric model, so a
difference is either behaviour that changes when a program leaves z/OS, or an ironwork bug;
Enterprise COBOL settles which. CBL and PROCESS cards are removed for the other compiler, which does
not read them. gcobol runs with -dialect ibm; cobc with -x -std=ibm-strict, and ironwork then with
--dialect gnucobol, so what differs is what docs/dialect.md lists as not switched. The other
compiler's clock cannot be fixed, so a program that reads the date differs by design.

--vm runs each program under `ironwork run --vm` in gcobol's place, and reports where the VM and the
interpreter differ in exit status, standard output or standard error (docs/lir.md §12.3). A program
lowering refuses, or a run the VM stops at what it does not run yet, is vm-stops, with what stopped
it.

usage: differ.py <ironwork binary> <program or directory>... [-I dir]... [--stdin file]
                 [--cobc] [--gcobol command] [--exec command] [--timeout seconds] [--vm]

--gcobol names the compiler (default gcobol, or cobc with --cobc). --exec is what runs the
executable it links: tools/gcobol/gcobol installed as gcobol-exec runs it in the same container; on
Linux, and with --cobc, leave it out. Exit status: 0 every program agrees, 1 some differ (with --vm,
only a difference counts), 2 usage.
"""
import argparse, calendar, os, re, shlex, shutil, subprocess, sys, tempfile, time

CLOCK = "2026-01-02T03:04:05"
PROGRAM = (".cbl", ".cob")
# ironwork run's exit statuses besides a RETURN-CODE (README, Exit status): 244 is an abend whose
# code is ironwork's own, IRONWORK, EXEC or JAVA.
IRONWORK_REFUSED, IRONWORK_ABEND, IRONWORK_PANIC = (241, 242, 243, 245, 246), (240, 244), 255
# The status a RETURN-CODE outside 0-238, or of 239, exits with; standard error gives its value.
OUTSIDE = re.compile(r"^ironwork: RETURN-CODE (-?\d+) exits 239$", re.M)
# Lowering refused the program, or the VM stopped at what it does not run yet; stderr says which.
VM_STOPPED_STATUS = (242, 243)
# A gcobol program that raises a fatal exception condition ends in abort(): SIGTRAP, which a
# container reports as 128 + 5.
SIGTRAP_STATUSES = (133, -5)
OPTION_CARD = re.compile(r"^(?:.{6}[ D]?)?\s*(?:CBL|PROCESS)\b", re.I)

def programs(paths):
    for p in paths:
        if os.path.isdir(p):
            for dirpath, _, files in sorted(os.walk(p)):
                yield from (os.path.join(dirpath, f) for f in sorted(files) if f.lower().endswith(PROGRAM))
        else:
            yield p

def without_option_cards(text):
    """The source with the CBL and PROCESS cards before its first line of code blanked, so line
    numbers stay where they were."""
    lines = text.split("\n")
    for i, line in enumerate(lines):
        if not line.strip() or (len(line) > 6 and line[6] in "*/"):
            continue
        if not OPTION_CARD.match(line):
            break
        lines[i] = ""
    return "\n".join(lines)

def run(argv, stdin, timeout, cwd=None):
    with open(stdin, "rb") if stdin else open(os.devnull, "rb") as f:
        try:
            r = subprocess.run(argv, stdin=f, capture_output=True, timeout=timeout, cwd=cwd)
        except subprocess.TimeoutExpired:
            return None, "", f"no answer in {timeout}s"
    return r.returncode, r.stdout.decode("utf-8", "replace"), r.stderr.decode("utf-8", "replace")

def first_line(text, pattern=None):
    lines = [l for l in text.splitlines() if l.strip()]
    if pattern:
        lines = [l for l in lines if re.search(pattern, l)] or lines
    return lines[0].strip()[:160] if lines else "(no message)"

TRANSLATED = re.compile(r"^.{6}[ D].*?\bEXEC\s+(CICS|SQL|DLI)\b", re.I | re.M)

def compile_failure(source, err):
    """Why gcobol did not produce a program: a translator it does not have, a runtime routine its
    library lacks, or its first error."""
    translated = TRANSLATED.search(source)
    if translated:
        return f"EXEC {translated.group(1).upper()}: gcobol has no translator for it"
    missing = re.search(r"undefined reference to `(__gg__\w+)'", err)
    if missing:
        return f"libgcobol has no {missing.group(1)}"
    return first_line(err, r"error:")

def ironwork(binary, path, libraries, stdin, timeout, flags):
    status, out, err = run([binary, "run", path, "-silent", "--clock", CLOCK, *flags, *libraries], stdin, timeout)
    if status is None:
        return ("timeout", err, "")
    if status in IRONWORK_REFUSED:
        return ("refused", first_line(err), out)
    if status in IRONWORK_ABEND:
        return ("abend", first_line(err), out)
    if status == IRONWORK_PANIC:
        return ("crash", first_line(err, r"panicked at"), out)
    said = OUTSIDE.search(err) if status == 239 else None
    return ("ran", int(said.group(1)) if said else status, out)

def gcobol(compiler, runner, path, libraries, stdin, timeout, scratch, cobc):
    source = os.path.join(scratch, os.path.basename(path))
    with open(path, encoding="latin-1") as f:
        text = f.read()
    with open(source, "w", encoding="latin-1") as f:
        f.write(without_option_cards(text))
    exe = os.path.join(scratch, "program")
    dialect = ["-x", "-std=ibm-strict"] if cobc else ["-dialect", "ibm"]
    argv = [*compiler, *dialect, "-I", os.path.dirname(os.path.abspath(path)), *libraries, "-o", exe, source]
    status, _, err = run(argv, None, timeout, cwd=scratch)
    if status is None:
        return ("timeout", err, "")
    if status != 0:
        return ("refused", compile_failure(text, err).replace(scratch + os.sep, ""), "")
    status, out, err = run([*runner, exe], stdin, timeout, cwd=scratch)
    if status is None:
        return ("timeout", err, "")
    if status in SIGTRAP_STATUSES:
        return ("abend", first_line(err, r"exception"), out)
    return ("ran", status, out)

def compare(iw, gc, name):
    """One verdict and what it rests on."""
    if iw[0] == "ran" and gc[0] == "ran":
        a, b = iw[2].splitlines(), gc[2].splitlines()
        for n, (x, y) in enumerate(zip(a, b), 1):
            if x.rstrip() != y.rstrip():
                return "differ", f"line {n}: ironwork {x.rstrip()!r}, {name} {y.rstrip()!r}"
        if len(a) != len(b):
            return "differ", f"ironwork wrote {len(a)} lines, {name} {len(b)}"
        if iw[1] % 256 != gc[1] % 256:
            return "differ", f"return code: ironwork {iw[1]}, {name} {gc[1]}"
        return "agree", ""
    if iw[0] == gc[0] == "abend":
        return "agree", f"both abend: ironwork {iw[1]}; {name} {gc[1]}"
    if iw[0] == "refused" and gc[0] == "refused":
        return "both-refuse", f"ironwork {iw[1]}; {name} {gc[1]}"
    if iw[0] == "refused":
        return "ironwork-refuses", iw[1]
    if gc[0] == "refused":
        return f"{name}-refuses", gc[1]
    return "differ", f"ironwork {iw[0]}: {iw[1]}; {name} {gc[0]}: {gc[1]}"

VM_STOPPED = re.compile(r"the VM does not run (.+) yet; run it without --vm|: (?:IW[A-Z]\d{4}-[IWESU] )?(lowering: .+)")
# A Rust panic names its thread by a number that differs from run to run.
PANIC_THREAD = re.compile(r"^(thread '[^']*') \(\d+\)(?= panicked at )", re.M)

def compare_vm(iw, vm):
    """One verdict on a program's runs on the interpreter and the VM, each (status, stdout,
    stderr), and what it rests on."""
    iw, vm = ((s, out, PANIC_THREAD.sub(r"\1", err)) for s, out, err in (iw, vm))
    if iw[0] is None or vm[0] is None:
        return ("both-timeout", "") if iw[0] is vm[0] is None else ("differ", f"{'the interpreter' if iw[0] is None else 'the VM'} timed out")
    stopped = VM_STOPPED.search(vm[2])
    if vm[0] in VM_STOPPED_STATUS and stopped:
        return "vm-stops", stopped.group(1) or stopped.group(2)
    if iw == vm:
        return ("both-refuse", first_line(iw[2])) if iw[0] in IRONWORK_REFUSED else ("agree", "")
    for what, a, b in (("standard output", iw[1], vm[1]), ("standard error", iw[2], vm[2])):
        x, y = a.splitlines(), b.splitlines()
        n = next((i for i, (p, q) in enumerate(zip(x, y)) if p != q), min(len(x), len(y)))
        if x != y:
            return "differ", f"{what} line {n + 1}: interpreter {x[n] if n < len(x) else None!r}, VM {y[n] if n < len(y) else None!r}"
    return "differ", f"exit status: interpreter {iw[0]}, VM {vm[0]}"

def main():
    ap = argparse.ArgumentParser(usage=__doc__.split("usage: ")[1].split("\n\n")[0])
    ap.add_argument("binary")
    ap.add_argument("paths", nargs="+")
    ap.add_argument("-I", dest="libraries", action="append", default=[])
    ap.add_argument("--stdin")
    ap.add_argument("--cobc", action="store_true")
    ap.add_argument("--gcobol")
    ap.add_argument("--exec", dest="runner", default="")
    ap.add_argument("--timeout", type=float, default=60)
    ap.add_argument("--vm", action="store_true")
    try:
        args = ap.parse_args()
    except SystemExit:
        return 2
    libraries = [arg for d in args.libraries for arg in ("-I", os.path.abspath(d))]
    if args.vm:
        # Each run compiles afresh, so WHEN-COMPILED is fixed at the clock's time too.
        os.environ["SOURCE_DATE_EPOCH"] = str(calendar.timegm(time.strptime(CLOCK, "%Y-%m-%dT%H:%M:%S")))
        print(f"# the interpreter and the VM; clock and WHEN-COMPILED {CLOCK}")
        tally = {}
        for path in programs(args.paths):
            argv = [args.binary, "run", path, "-silent", "--clock", CLOCK, *libraries]
            verdict, why = compare_vm(run(argv, args.stdin, args.timeout), run([*argv, "--vm"], args.stdin, args.timeout))
            tally[verdict] = tally.get(verdict, 0) + 1
            print(f"{verdict}\t{path}" + (f"\t{why}" if why else ""))
        print("# " + ", ".join(f"{n} {v}" for v, n in sorted(tally.items(), key=lambda kv: -kv[1])))
        return 1 if tally.get("differ") else 0
    compiler, runner = shlex.split(args.gcobol or ("cobc" if args.cobc else "gcobol")), shlex.split(args.runner)
    name, flags = ("cobc", ["--dialect", "gnucobol"]) if args.cobc else ("gcobol", [])
    if not shutil.which(compiler[0]) or (runner and not shutil.which(runner[0])):
        print(f"differ: {compiler[0] if not shutil.which(compiler[0]) else runner[0]} is not on PATH", file=sys.stderr)
        return 2
    # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-tainted-env-args.dangerous-subprocess-use-tainted-env-args -- the compiler named on the command line, as an argument list
    version = subprocess.run([*compiler, "--version"], capture_output=True, text=True).stdout.splitlines()
    print(f"# {version[0] if version else compiler[0]}; ironwork clock {CLOCK}")
    tally, differ = {}, False
    for path in programs(args.paths):
        with tempfile.TemporaryDirectory(prefix="differ-") as scratch:
            iw = ironwork(args.binary, path, libraries, args.stdin, args.timeout, flags)
            gc = gcobol(compiler, runner, path, libraries, args.stdin, args.timeout, scratch, args.cobc)
        verdict, why = compare(iw, gc, name)
        tally[verdict] = tally.get(verdict, 0) + 1
        differ |= verdict != "agree"
        print(f"{verdict}\t{path}" + (f"\t{why}" if why else ""))
    print("# " + ", ".join(f"{n} {v}" for v, n in sorted(tally.items(), key=lambda kv: -kv[1])))
    return 1 if differ else 0

if __name__ == "__main__":
    sys.exit(main())
