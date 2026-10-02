#!/usr/bin/env python3
"""Run the NIST CCVS85 audit routines under `ironwork run` and class what each one did.

The routines are the src/*.CBL files of a CCVS85 checkout split one program to a file, as in
z390development/nistcobol85; EXEC85, the executive routine that does the splitting, is not run.
Each source, and each COPY member (src/*.CPY, the copy library), goes through EXEC85 as its
defaults leave it:

- optional code marked A, E, H, L, Y or T in column 7 is kept (B10-1-INIT-OPTION-SWITCHES), and
  every other lettered line but D becomes a comment (D82-OPTIONAL-LETTER);
- an X-card, XXXX in columns 12-15, a number nnn in 17-19, and a space or period in 20
  (D85-X-CARD-CHECK), has columns 12-72 replaced by X-card nnn's text, whatever column 16 holds.
  The User Guide's Appendix D says what each X-card holds. Those that are values are in
  X_CARD_TEXT, chosen for an EBCDIC z/OS system; every other X-card is an implementor-name and
  is given as XXXXXnnn, so XXXXP024 and XXXXD024 name the same file as XXXXX024, the file of that
  name in the work directory, given to ironwork as DD_XXXXXnnn.

Every source is written to one work directory before any program runs, so a CALL finds its
subprogram there, and programs run in name order, since later routines read files earlier ones
wrote (the User Guide's Appendix B). A program another source CALLs, by literal or through an item
whose VALUE is its name, is a subprogram and runs only when called. ABSENT_FILES lists the
routines whose opening comments say a file is not present when they run, and that file is removed
before each of them. A program's standard input, which ACCEPT reads, is src/<name>.DAT where
there is one and empty otherwise. A routine reports on the print file XXXXX055, read as EBCDIC,
and on standard output.

  clean    exit 0 and no FAIL* line in the report
  failed   exit 0 and a FAIL* line
  refused  exit 12: a compile error
  abend    exit 16
  called   a subprogram, not run on its own
  compiled a flagging test (xx3nnM, xx4nnM) that `ironwork check` compiles without error; the
           User Guide's 3.7 says to compile these and not run them
  exit-N   any other exit status; timeout after --timeout seconds

The results file has a line per program: name, class, and the first line ironwork wrote to
standard error, with the work directory's path removed.

usage: nist.py <ironwork binary> <CCVS85 src directory> [--out results.tsv]
               [--baseline results.tsv] [--workdir dir] [--timeout seconds]

--workdir keeps the work directory, which must not exist yet; otherwise a temporary one is used
and removed. --baseline lists each program whose class differs from an earlier results file.
Exit status: 0; 1 when a program clean in the baseline now fails, is refused or abends; 2 usage.
"""
import argparse, collections, glob, os, re, shutil, subprocess, sys, tempfile

SELECTED_SWITCHES = set("AEHLYT")
PRINT_FILE = "XXXXX055"
EXECUTIVE = "EXEC85"
CLASSES = ("clean", "failed", "refused", "abend", "called", "compiled")
X_CARD = re.compile(r"\bXXXXX\d{3}\b")
FLAGGING_TEST = re.compile(r"^[A-Z]{2}[34]\d\dM$")
X_CARD_TEXT = {
    **{f"{n:03d}": f'"CCVS{n:03d}"' for n in range(30, 44)},  # queue names and passwords of a CD
    "051": "UPSI-0",
    "052": "UPSI-1",
    "056": "SYSOUT",
    "057": "SYSIN",
    # The COBOL character set in EBCDIC order, quote left out and $ twice; X-064 descends.
    "063": '" .<(+$$*);-/,>=ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"',
    "064": '"9876543210ZYXWVUTSRQPONMLKJIHGFEDCBA=>,/-;)*$$+(<. "',
    "065": "9000",
    "073": "C01",
    "081": '"@#%&?_!~"',
    "084": "OMITTED",
    "086": 'PIC X(6) VALUE "FILE 1"',
    "090": "194",  # ordinal numbers of A (X'C1') and 0 (X'F0') in EBCDIC
    "091": "241",
}
ABSENT_FILES = {
    "IX111A": ("025",),
    "IX216A": ("025",),
    "IX217A": ("024", "025"),
    "IX218A": ("024", "025"),
    "SQ129A": ("001",),
    "SQ130A": ("014",),
    "SQ141A": ("001",),
    "SQ142A": ("001",),
    "SQ225A": ("014",),
}

def select_options(text):
    lines = []
    for line in text.splitlines():
        if len(line) > 6 and line[6].isalpha() and line[6] not in "Dd":
            line = line[:6] + (" " if line[6] in SELECTED_SWITCHES else "*") + line[7:]
        lines.append(line)
    return "\n".join(lines) + "\n"

def substitute_x_card(line):
    if line[11:15] != "XXXX" or not line[16:19].isdigit() or line[19:20] not in ("", " ", "."):
        return line
    number = line[16:19]
    text = X_CARD_TEXT.get(number, f"XXXXX{number}") + line[19:20]
    return line[:11] + text.ljust(61) + line[72:]

def substitute_x_cards(text):
    return "\n".join(substitute_x_card(line) for line in text.splitlines()) + "\n"

def code_area(text):
    """Columns 8-72 of every line that is not a comment."""
    return "\n".join(line[7:72] for line in text.splitlines() if len(line) < 7 or line[6] not in "*/")

def program_id(text):
    m = re.search(r"PROGRAM-ID\.?\s+[\"']?([\w-]+)", code_area(text))
    return m.group(1) if m else None

def call_targets(text):
    """The program names a source CALLs: literals, and the VALUE of each item a CALL names."""
    code = code_area(text)
    targets = set(re.findall(r"\bCALL\s+[\"']([^\"']+)[\"']", code))
    for item in set(re.findall(r"\bCALL\s+([A-Z0-9][\w-]*)", code)):
        targets |= set(re.findall(rf"\b{re.escape(item)}\b[^.]*?\bVALUE\s+(?:IS\s+)?[\"']([^\"']+)[\"']", code))
    return targets

def classify(flagging, code, report):
    if flagging and code in (0, 4):
        return "compiled"
    if code == 0:
        return "failed" if "FAIL*" in report else "clean"
    return {12: "refused", 16: "abend"}.get(code, f"exit-{code}")

def run(binary, workdir, name, env, include, sysin, timeout):
    for f in (PRINT_FILE, *(f"XXXXX{n}" for n in ABSENT_FILES.get(name, ()))):
        if os.path.exists(os.path.join(workdir, f)):
            os.remove(os.path.join(workdir, f))
    print_file = os.path.join(workdir, PRINT_FILE)
    flagging = FLAGGING_TEST.match(name) is not None
    try:
        r = subprocess.run([binary, "check" if flagging else "run", os.path.join(workdir, f"{name}.CBL"),
                            "-I", include],
                           cwd=workdir, env=env, input=sysin, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    report = r.stdout.decode("utf-8", "replace")
    if os.path.exists(print_file):
        with open(print_file, "rb") as f:
            report += f.read().decode("cp037", "replace")
    stderr = r.stderr.decode("utf-8", "replace").strip().splitlines()
    first = stderr[0].replace(workdir + os.sep, "").replace("\t", " ") if stderr else ""
    return classify(flagging, r.returncode, report), first

def sweep(binary, src, workdir, timeout):
    library = os.path.join(workdir, "copy")
    os.makedirs(library)
    sources, members = [], []
    for path in sorted(glob.glob(os.path.join(src, "*.CBL")) + glob.glob(os.path.join(src, "*.CPY"))):
        name, extension = os.path.splitext(os.path.basename(path))
        if name == EXECUTIVE:
            continue
        with open(path, errors="replace") as f:
            text = substitute_x_cards(select_options(f.read()))
        member = extension.upper() == ".CPY"
        with open(os.path.join(library if member else workdir, name + extension), "w") as f:
            f.write(text)
        (members if member else sources).append((name, text))

    # A called subprogram opens its own files, so every run is given every X-card's DD.
    env = dict(os.environ)
    for _, text in sources + members:
        for dd in X_CARD.findall(text):
            env[f"DD_{dd}"] = os.path.join(workdir, dd)
    called = {target for _, text in sources for target in call_targets(text) - {program_id(text)}}
    return [(name, "called", "") if program_id(text) in called
            else (name, *run(binary, workdir, name, env, library, sysin(src, name), timeout))
            for name, text in sources]

def sysin(src, name):
    path = os.path.join(src, f"{name}.DAT")
    if not os.path.exists(path):
        return b""
    with open(path, "rb") as f:
        return f.read()

def read_results(path):
    with open(path) as f:
        rows = [line.rstrip("\n").split("\t") for line in f][1:]
    return {row[0]: row[1] for row in rows}

def main():
    p = argparse.ArgumentParser(usage=__doc__.split("usage: ")[1].split("\n\n")[0])
    p.add_argument("binary")
    p.add_argument("src")
    p.add_argument("--out")
    p.add_argument("--baseline")
    p.add_argument("--workdir")
    p.add_argument("--timeout", type=float, default=120)
    a = p.parse_args()
    binary = os.path.abspath(a.binary)
    if not os.path.isdir(a.src):
        p.error(f"{a.src} is not a directory")
    if a.workdir:
        os.makedirs(a.workdir)
        workdir = os.path.abspath(a.workdir)
    else:
        workdir = tempfile.mkdtemp(prefix="nist-")
    try:
        results = sweep(binary, os.path.abspath(a.src), workdir, a.timeout)
    finally:
        if not a.workdir:
            shutil.rmtree(workdir, ignore_errors=True)

    if a.out:
        with open(a.out, "w") as f:
            f.write("program\tclass\tstderr\n")
            for row in results:
                f.write("\t".join(row) + "\n")
    tally = collections.Counter(cls for _, cls, _ in results)
    others = sorted(set(tally) - set(CLASSES))
    print(f"{len(results)} programs: " + ", ".join(f"{cls} {tally[cls]}" for cls in (*CLASSES, *others)))

    regressed = False
    if a.baseline:
        before = read_results(a.baseline)
        for name, cls, _ in results:
            if before.get(name, "new") != cls:
                print(f"{name}\t{before.get(name, 'new')} -> {cls}")
                regressed |= before.get(name) == "clean" and cls not in ("called", "compiled")
    return 1 if regressed else 0

if __name__ == "__main__":
    sys.exit(main())
