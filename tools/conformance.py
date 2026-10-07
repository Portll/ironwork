#!/usr/bin/env python3
"""Write ironwork's conformance report: what each kind of evidence says about how closely ironwork
follows IBM, with no claim that an IBM compiler has witnessed it.

It reads, beside the binary's own register of assumptions (`ironwork assumptions`):

  docs/conformance/nist.tsv          tools/nist.py --vm --out, unless --nist names a newer run
  docs/conformance/ibm-listings.json tools/ibm-listings.py --json: IBM's compile listings of CCVS85
  docs/conformance/census.json       tools/census.py --json: counts over a third-party corpus
  docs/conformance/hercules.txt      ironwork-oracle hercules: the machine model against Hercules
  docs/conformance/differential.json tools/differential-campaign.py total: generated inputs run on
                                     the interpreter and the VM, when a campaign has been kept
  tools/db2-probe/observed-12.1.5.txt  what Db2 for Linux answered the SQL probes
  docs/messages.md                   the catalogue, for each refusal's text and area

usage: conformance.py <ironwork binary> [--nist results.tsv] [--commit sha] [--out report.md]
"""
import argparse, collections, json, os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INPUTS = os.path.join(ROOT, "docs", "conformance")
DIFFERENTIAL = os.path.join(INPUTS, "differential.json")
MODULES = {
    "NC": "Nucleus",
    "SQ": "Sequential I-O",
    "RL": "Relative I-O",
    "IX": "Indexed I-O",
    "ST": "Sort-Merge",
    "SM": "Source Text Manipulation",
    "IC": "Inter-Program Communication",
    "CM": "Communication",
    "DB": "Debug",
    "SG": "Segmentation",
    "RW": "Report Writer",
    "IF": "Intrinsic Function",
    "OB": "Obsolete Elements",
}
CLASSES = ("clean", "failed", "refused", "abend", "called", "compiled")
FLAGGING_TEST = re.compile(r"^[A-Z]{2}[34]\d\dM$")
BASES = {
    "documented": "stated in IBM's documentation and checked against its text",
    "chosen": "stated nowhere: IBM's documentation leaves it to the generated code, and ironwork chose",
    "observed": "seen on a related system the claim names, rather than on the one that settles it",
    "recalled": "stated by IBM and written from memory of it, not yet checked against the manual",
}
SETTLED_BY = {
    "enterprise-cobol": "Enterprise COBOL on the pinned target",
    "db2": "Db2 for z/OS",
    "hercules": "Hercules",
}

def table(header, rows):
    lines = ["| " + " | ".join(header) + " |", "|" + "|".join("---" for _ in header) + "|"]
    lines += ["| " + " | ".join(str(c).replace("|", "\\|") for c in row) + " |" for row in rows]
    return "\n".join(lines)

def read_tsv(path):
    with open(path) as f:
        header, *rows = [line.rstrip("\n").split("\t") for line in f]
    return [dict(zip(header, row)) for row in rows]

def read_json(path):
    with open(path) as f:
        return json.load(f)

def assumptions(binary):
    out = subprocess.run([binary, "assumptions"], capture_output=True, text=True, check=True).stdout
    return [dict(zip(("id", "basis", "settled_by", "claim"), line.split("\t", 3))) for line in out.splitlines() if line]

def catalogue():
    with open(os.path.join(ROOT, "docs", "messages.md")) as f:
        text = f.read()
    areas = dict(re.findall(r"^\| ([A-Z]) \| (.+?) \|$", text, re.M))
    messages = {m: body.strip("`") for m, body in re.findall(r"^\| (IW[A-Z]\d{4}) \| [IWESU] \| (.+?) \|$", text, re.M)}
    return areas, messages

def nist_section(results):
    by_module = collections.defaultdict(collections.Counter)
    for r in results:
        by_module[r["program"][:2]][r["class"] if r["class"] in CLASSES else "other"] += 1
    order = [m for m in MODULES if m in by_module] + sorted(set(by_module) - set(MODULES))
    rows = [[f"{m} {MODULES.get(m, '')}".rstrip(), sum(by_module[m].values()), *(by_module[m][c] for c in (*CLASSES, "other"))] for m in order]
    total = collections.Counter(r["class"] if r["class"] in CLASSES else "other" for r in results)
    rows.append(["**All**", len(results), *(total[c] for c in (*CLASSES, "other"))])
    flagging = [r for r in results if FLAGGING_TEST.match(r["program"])]
    standalone = len(results) - total["called"] - len(flagging)
    out = [
        "## NIST CCVS85, module by module",
        "",
        f"NIST's COBOL 85 test suite, {len(results)} programs as tools/nist.py runs them. Of the {standalone} programs that run "
        f"on their own, {total['clean']} pass every test, {total['failed']} report a failed test, {total['refused']} are refused "
        f"and {total['abend']} abend. {total['called']} subprograms run when another program calls them, and of the "
        f"{len(flagging)} flagging tests, which are compiled and not run, {total['compiled']} compile without error; "
        "\"other\" counts the rest, which give a compile error.",
        "",
        table(["Module", "Programs", *CLASSES, "other"], rows),
        "",
    ]
    rest = [r for r in results if r["class"] not in ("clean", "called", "compiled")]
    if rest:
        out += ["Every program that does not pass, with the first line ironwork wrote to standard error:", "",
                table(["Program", "Class", "ironwork said"], [[r["program"], r["class"], r.get("stderr", "") or "(nothing; the report names the failed test)"] for r in rest]), ""]
    return out

def vm_section(results):
    ran = [r for r in results if r.get("vm", "-") != "-"]
    if not ran:
        return ["## Interpreter and VM", "", "The NIST results given were not run on the VM.", ""]
    verdict = collections.Counter(r["vm"] for r in ran)
    out = ["## Interpreter and VM", "",
           f"Each CCVS85 program that runs on the interpreter is run again on the VM from the same files, and the two runs' exit "
           f"status, standard output, standard error and every file they leave are compared. Of {len(ran)} programs, "
           f"{verdict['same']} agree in all of these, {verdict['differ']} differ and {verdict['stopped']} stop on the VM at a "
           "construct it does not run yet.", ""]
    odd = [r for r in ran if r["vm"] != "same"]
    if odd:
        out += [table(["Program", "VM", "What it rests on"], [[r["program"], r["vm"], r.get("vm detail", "")] for r in odd]), ""]
    return out

def differential_section(campaign):
    by_commit = collections.Counter(l.split()[0][:8] for l in campaign.get("labels", []) if l)
    shards = ", ".join(f"{c}, {n} shard{'s' if n > 1 else ''}" for c, n in by_commit.items())
    out = [f"A differential campaign runs `ironwork fuzz --differential` on the batch programs of CCVS85 for a budget of CPU "
           f"time: each generated input runs on the interpreter and on the VM, and the two runs are compared. "
           f"{', '.join(campaign['ironwork'])}, {campaign['cpuHours']:,} CPU-hours"
           + (f" ({shards})" if shards else "") + f": of {campaign['candidates']:,} candidate programs "
           f"{campaign['fuzzed']:,} were fuzzed and {campaign['refused']:,} left the campaign, as fuzz refused them or lowering did. "
           f"Of {campaign['runs']:,} runs, {campaign['agree']:,} agree, {campaign['atLimit']:,} of them with both executors at the "
           f"statement limit; {campaign['timedOut']:,} timed out, {campaign['stopped']:,} stopped on the VM and "
           f"{campaign['differ']:,} differ.", ""]
    if campaign.get("stoppedAt"):
        out += [table(["The VM stopped at", "Runs"], [[w, f"{n:,}"] for w, n in campaign["stoppedAt"].items()]), ""]
    if campaign.get("divergences"):
        out += [table(["Program", "Seed", "Inputs that differ", "What differed"],
                      [[os.path.basename(d["program"]), d["seed"], d["differ"], "; ".join(d.get("said", []))] for d in campaign["divergences"]]), ""]
    return out

def listings_section(listings, results):
    classes = {r["program"]: r["class"] for r in results}
    rows = listings["listings"]
    codes = collections.Counter(str(l["return_code"]) for l in rows)
    compiled = [l for l in rows if l["return_code"] is not None and l["return_code"] <= 4]
    ironwork = collections.Counter(classes.get(l["program"], "not run") for l in compiled)
    compilers = sorted({l["compiler"] for l in rows if l["compiler"]})
    out = ["## IBM's compile listings of CCVS85", "",
           f"{len(rows)} listings Enterprise COBOL {', '.join(compilers)} wrote for CCVS85 programs, from {listings['source']}. "
           "They show what IBM's compiler did with each program's source; they hold no run, so no IBM run is witnessed.", "",
           table(["IBM's return code", "Listings"], [[c, n] for c, n in sorted(codes.items(), key=lambda x: (not x[0].isdigit(), int(x[0]) if x[0].isdigit() else 0))]), "",
           f"Of the {len(compiled)} programs IBM compiled at return code 4 or less, ironwork's results: "
           + ", ".join(f"{c} {n}" for c, n in sorted(ironwork.items())) + ".", ""]
    rest = [l for l in compiled if classes.get(l["program"], "not run") not in ("clean", "called", "compiled")]
    if rest:
        out += [table(["Program", "IBM's return code", "ironwork"], [[l["program"], l["return_code"], classes.get(l["program"], "not run")] for l in rest]), "",
                "\"failed\" compiles and reports a failed test when run; \"not run\" is a program tools/nist.py does not run, "
                "such as EXEC85, the suite's own executive routine.", ""]
    rejected = [l for l in rows if l["return_code"] is None or l["return_code"] > 4]
    if rejected:
        why = listings.get("notes", {})
        out += ["The listings where IBM's compiler stopped:", "",
                table(["Program", "IBM's return code", "First message", "Why", "ironwork"],
                      [[l["program"] or l["file"], l["return_code"], l["first_severe"] or "", why.get(l["first_severe"] or "", ""),
                        classes.get(l["program"], "-") if l["program"] else "-"] for l in rejected]), ""]
    return out

def census_section(census, areas, messages):
    def basis(reason):
        if reason == "(timeout)":
            return "The check took longer than 20 seconds"
        if "COPY member not found" in reason:
            return "The corpus lacks a member the program copies"
        m = re.match(r"^IW([A-Z])\d{4}", reason)
        return areas.get(m.group(1), "") if m else "A message with no id"
    def text(reason):
        m = re.match(r"^(IW[A-Z]\d{4})", reason)
        return messages.get(m.group(1), "") if m else ""
    refused = census["refused"]
    by_area = collections.Counter()
    for r in refused:
        by_area[basis(r["reason"])] += r["programs"]
    codes = census.get("return_codes", {})
    out = ["## Census of a public corpus", "",
           f"{census['sample']:,} programs drawn at random (seed {census['seed']}) from the {census['population']:,} in "
           f"{census['corpus']}, public repositories of COBOL, checked with {census['ironwork']} under `--compliance strict`. "
           f"{census['compiled']:,} compile with no message above a warning. The corpus holds programs written for other "
           "compilers and broken ones as well as Enterprise COBOL, so a refusal is often right; each is counted under the "
           "rule its message states. Only these counts are published, never a program's path.", ""]
    if codes:
        out += [table(["`ironwork check` return code", "Programs"], [[c, f"{n:,}"] for c, n in codes.items()]), "",
                "Return code 8 is a compile with E-level messages, which still gives a program to run, as Enterprise COBOL's "
                "does; it is counted below among the refusals by its first message.", ""]
    out += ["The refusals by what their verdict rests on:", "",
            table(["Basis", "Programs"], [[b, f"{n:,}"] for b, n in by_area.most_common()]), "",
            "The reasons that refuse 5 programs or more:", "",
            table(["Programs", "Reason", "Message"], [[r["programs"], r["reason"], text(r["reason"])] for r in refused if r["programs"] >= 5]), ""]
    return out

def assumptions_section(register):
    count = collections.Counter(a["basis"] for a in register)
    settled = collections.Counter((a["basis"], a["settled_by"]) for a in register)
    targets = [t for t in SETTLED_BY if any(a["settled_by"] == t for a in register)]
    rows = [[f"**{b.capitalize()}**", BASES.get(b, ""), count[b], *(settled[(b, t)] for t in targets)] for b in BASES if count[b]]
    rows.append(["**All**", "", len(register), *(sum(1 for a in register if a["settled_by"] == t) for t in targets)])
    out = ["## Assumptions", "",
           f"`ironwork assumptions` lists {len(register)} claims about what IBM's compiler, Language Environment, the machine, "
           "Db2 and CICS do, each naming what would settle it.", "",
           table(["Basis", "Meaning", "Claims", *(f"settled by {SETTLED_BY[t]}" for t in targets)], rows), ""]
    for b in ("recalled", "observed", "chosen"):
        entries = [a for a in register if a["basis"] == b]
        if entries:
            out += [f"### {b.capitalize()} ({len(entries)})", "",
                    table(["Id", "Settled by", "Claim"], [[a["id"], SETTLED_BY.get(a["settled_by"], a["settled_by"]), a["claim"]] for a in entries]), ""]
    return out

def hercules_section(text, register, commit):
    lines = text.splitlines()
    rows = [l.split() for l in lines if re.match(r"^[A-Z]+\s+\d+\s+\d+$", l)]
    total = re.search(r"^(\d+) agree, (\d+) disagree$", text, re.M)
    agree, disagree = (int(total.group(1)), int(total.group(2))) if total else (0, 0)
    differing = [r[0] for r in rows if r[2] != "0"]
    settled = [a for a in register if a["settled_by"] == "hercules"]
    out = ["## The machine against Hercules", "",
           f"{lines[0]}: a bare-metal program runs each case of the decimal instructions and the HFP arithmetic, on edge "
           f"and random operands, and every result, condition code and program interruption is compared with ironwork's "
           f"model. {agree:,} of {agree + disagree:,} cases agree.", "",
           table(["Instruction", "Agree", "Disagree"], rows), ""]
    if differing:
        readme = f"https://github.com/Portll/ironwork/blob/{commit or 'main'}/README.md#hercules-a-second-reading-of-the-machine"
        out += [f"Hercules implements the same manual, so agreement is evidence, not proof, and where the two differ the manual "
                f"decides. The cases that differ are {', '.join(differing)}; the README's [Hercules section]({readme}) gives "
                "the page of the *Principles of Operation* that decides each.", ""]
    if settled:
        out += ["The register's claims that Hercules settles:", "",
                table(["Id", "Basis", "Claim"], [[a["id"], a["basis"], a["claim"]] for a in settled]), ""]
    return out

def db2_section(register):
    path = os.path.join(ROOT, "tools", "db2-probe", "observed-12.1.5.txt")
    with open(path) as f:
        sections = [l[3:].split(":")[0].strip() for l in f.read().splitlines() if l.startswith("== ")]
    observed = [a for a in register if a["settled_by"] == "db2" and a["basis"] == "observed"]
    return ["## Db2 for Linux", "",
            "The SQL runtime's assumptions are settled by Db2 for z/OS, which no session here can reach. tools/db2-probe "
            "runs probes against Db2 12.1.5 for Linux (Community Edition) and keeps what it answered "
            f"(tools/db2-probe/observed-12.1.5.txt): {', '.join(sections)}. These claims rest on it:", "",
            table(["Id", "Claim"], [[a["id"], a["claim"]] for a in observed]), ""]

def main():
    p = argparse.ArgumentParser(usage=__doc__.split("usage: ")[1].split("\n\n")[0])
    p.add_argument("binary")
    p.add_argument("--nist", default=os.path.join(INPUTS, "nist.tsv"))
    p.add_argument("--commit")
    p.add_argument("--out")
    a = p.parse_args()
    binary = os.path.abspath(a.binary)
    version = subprocess.run([binary, "--version"], capture_output=True, text=True, check=True).stdout.strip()
    commit = a.commit or subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    results = read_tsv(a.nist)
    register = assumptions(binary)
    areas, messages = catalogue()
    with open(os.path.join(INPUTS, "hercules.txt")) as f:
        hercules = f.read()
    report = [
        f"# Conformance of {version}",
        "",
        f"Written by tools/conformance.py{f' at {commit}' if commit else ''}.",
        "",
        "**No IBM compiler has witnessed these results.** ironwork has not run its programs on Enterprise COBOL for z/OS, "
        "so nothing here is scored against IBM's own output for them. The evidence is NIST's test suite, IBM's manuals, "
        "IBM's compile listings found in public repositories, Hercules for the machine and Db2 for Linux for SQL, each "
        "with what it can and cannot show.",
        "",
        *nist_section(results),
        *vm_section(results),
        *(differential_section(read_json(DIFFERENTIAL)) if os.path.exists(DIFFERENTIAL) else []),
        *listings_section(read_json(os.path.join(INPUTS, "ibm-listings.json")), results),
        *assumptions_section(register),
        *census_section(read_json(os.path.join(INPUTS, "census.json")), areas, messages),
        *hercules_section(hercules, register, commit),
        *db2_section(register),
    ]
    text = "\n".join(report).rstrip() + "\n"
    if a.out:
        with open(a.out, "w") as f:
            f.write(text)
    else:
        sys.stdout.write(text)
    return 0

if __name__ == "__main__":
    sys.exit(main())
