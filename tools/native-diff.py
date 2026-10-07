#!/usr/bin/env python3
"""Run each NIST CCVS85 program compiled to a load module both on the VM (`ironwork run X.iwm`) and as
native code (the `--native-harness` executable), and report where they differ: exit status, standard
output, standard error or a file either run leaves (codegen-runtime.md §10, invariant 1).

usage: native-diff.py <ironwork binary> <CCVS85 src directory> --runtime <crates dir>
                      [--workdir dir] [--timeout seconds] [--only NAME,...]
Exit status: 0 when no program differs, 1 when one does, 2 when the harness could not be built.
"""
import argparse, calendar, os, shutil, subprocess, sys, tempfile, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import nist  # noqa: E402

def data_files(workdir):
    """The work directory's files, but the sources, the modules and the harness."""
    return {k: v for k, v in nist.data_files(workdir).items() if not k.endswith(".iwm") and k != "harness"}

def restore(workdir, files):
    """The work directory's data files as `files` holds them, the modules and the harness left alone."""
    for name in set(data_files(workdir)) - set(files):
        os.remove(os.path.join(workdir, name))
    for name, data in files.items():
        with open(os.path.join(workdir, name), "wb") as f:
            f.write(data)

def execute(command, workdir, env, sysin, timeout):
    try:
        r = subprocess.run(command, cwd=workdir, env=env, input=sysin, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return None, b"", ""
    return r.returncode, r.stdout, nist.PANIC_THREAD.sub(r"\1", r.stderr.decode("utf-8", "replace").replace(workdir + os.sep, ""))

def main():
    p = argparse.ArgumentParser()
    p.add_argument("binary")
    p.add_argument("src")
    p.add_argument("--runtime", required=True)
    p.add_argument("--workdir")
    p.add_argument("--timeout", type=float, default=60)
    p.add_argument("--only")
    a = p.parse_args()
    binary = os.path.abspath(a.binary)
    if a.workdir:
        os.makedirs(a.workdir)
        workdir = os.path.abspath(a.workdir)
    else:
        workdir = tempfile.mkdtemp(prefix="native-diff-")
    library = os.path.join(workdir, "copy")
    os.makedirs(library)
    sources, members = [], []
    for path in sorted(nist.glob.glob(os.path.join(a.src, "*.CBL")) + nist.glob.glob(os.path.join(a.src, "*.CPY"))):
        name, extension = os.path.splitext(os.path.basename(path))
        if name == nist.EXECUTIVE:
            continue
        with open(path, errors="replace") as f:
            text = nist.substitute_x_cards(nist.select_options(f.read()))
        member = extension.upper() == ".CPY"
        with open(os.path.join(library if member else workdir, name + extension), "w") as f:
            f.write(text)
        (members if member else sources).append((name, text))
    only = set(a.only.split(",")) if a.only else None
    env = dict(os.environ)
    for _, text in sources + members:
        for dd in nist.X_CARD.findall(text):
            env[f"DD_{dd}"] = os.path.join(workdir, dd)
    env["SOURCE_DATE_EPOCH"] = str(calendar.timegm(time.strptime(nist.CLOCK, "%Y-%m-%dT%H:%M:%S")))
    called = {target for _, text in sources for target in nist.call_targets(text) - {nist.program_id(text)}}
    compiled = [name for name, text in sources if only is None or name in only or nist.program_id(text) in called]
    started = time.time()
    build = subprocess.run([binary, "compile", *(f"{n}.CBL" for n in compiled), "-I", library, "-o", workdir, "--native-harness", "--runtime", a.runtime], cwd=workdir, env=env, capture_output=True)
    harness = os.path.join(workdir, "harness")
    if not os.path.exists(harness):
        sys.stderr.write(build.stderr.decode("utf-8", "replace")[-4000:])
        print("native-diff: the harness was not built", file=sys.stderr)
        return 2
    print(f"native-diff: {len(compiled)} sources compiled and the harness built in {time.time() - started:.0f} s", file=sys.stderr)
    counts, differ = {"same": 0, "differ": 0, "skipped": 0}, []
    for name, text in sources:
        module = f"{name}.iwm"
        if (only is not None and name not in only) or nist.program_id(text) in called or nist.FLAGGING_TEST.match(name) or not os.path.exists(os.path.join(workdir, module)):
            counts["skipped"] += 1
            continue
        for f in (nist.PRINT_FILE, *(f"XXXXX{n}" for n in nist.ABSENT_FILES.get(name, ()))):
            if os.path.exists(os.path.join(workdir, f)):
                os.remove(os.path.join(workdir, f))
        flags = ["-L", workdir, "--clock", nist.CLOCK, *(("--parm", nist.UPSI_PARM) if "UPSI-" in text else ())]
        sysin = nist.sysin(a.src, name)
        before = data_files(workdir)
        status, out, err = execute([binary, "run", module, *flags], workdir, env, sysin, a.timeout)
        after = data_files(workdir)
        restore(workdir, before)
        native = (*execute([harness, module, *flags], workdir, env, sysin, a.timeout), data_files(workdir))
        restore(workdir, after)
        if status is None:
            counts["skipped"] += 1
            continue
        verdict, why = nist.compare((status, out, err, after), native)
        verdict = "same" if verdict == "same" else "differ"
        counts[verdict] += 1
        if verdict == "differ":
            differ.append(f"{name}: {why.replace('interpreter', 'VM').replace(', VM ', ', native ', 1)}")
    print(f"native-diff: {counts['same']} same on the VM and native, {counts['differ']} differ, {counts['skipped']} not compared")
    for line in differ:
        print(f"native-diff: DIFFER {line}")
    if not a.workdir:
        shutil.rmtree(workdir, ignore_errors=True)
    return 1 if differ else 0

if __name__ == "__main__":
    sys.exit(main())
