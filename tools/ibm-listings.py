#!/usr/bin/env python3
"""Read what IBM's compiler did with each CCVS85 program, from a directory of its compile listings,
and hold it against what tools/nist.py found ironwork doing.

A listing names its compiler in its first line, its program in "End of compilation 1, program NAME"
and its outcome in "Return code N"; a listing IBM's compiler stopped before its end names no
program, and is kept under its file's name. Each keeps its first S- or U-level message.

usage: ibm-listings.py <listing dir> [--results nist.tsv] [--json listings.json] [--source text]
                       [--note MESSAGE=why]...

--json writes IBM's side alone, a line per listing, for tools/conformance.py, which joins it with
the NIST results it is given. --results prints the join: the listings by return code and, for each
program IBM compiled at 4 or less, ironwork's class, naming every one ironwork refuses, fails or
does not run. --source says where the listings come from, and each --note why IBM's compiler gave
a message that stopped a listing, as found by reading it.
"""
import argparse, collections, json, os, re, sys

COMPILER = re.compile(r"IBM Enterprise COBOL for z/OS\s+(\S+)")
PROGRAM = re.compile(r"End of compilation \d+,\s+program (\S+?),")
RETURN_CODE = re.compile(r"^.?Return code (\d+)", re.M)
SEVERE = re.compile(r"\b(IGY[A-Z]{2}\d{4}-[SU])\b")
COMPILES = ("clean", "failed", "called", "compiled")

def read_listing(path):
    with open(path, "rb") as f:
        text = f.read().decode("latin-1")
    compiler = COMPILER.search(text)
    program = PROGRAM.search(text)
    codes = RETURN_CODE.findall(text)
    severe = SEVERE.search(text)
    return {
        "file": os.path.basename(path),
        "compiler": compiler.group(1) if compiler else None,
        "program": program.group(1) if program else None,
        "return_code": int(codes[-1]) if codes else None,
        "first_severe": severe.group(1) if severe else None,
    }

def read_results(path):
    with open(path) as f:
        rows = [line.rstrip("\n").split("\t") for line in f][1:]
    return {row[0]: row[1] for row in rows}

def summarise(listings, results):
    by_code = collections.Counter(str(l["return_code"]) for l in listings)
    compiled = [l for l in listings if l["return_code"] is not None and l["return_code"] <= 4]
    classes = collections.Counter(results.get(l["program"], "not run") for l in compiled)
    return {
        "compilers": sorted({l["compiler"] for l in listings if l["compiler"]}),
        "listings": len(listings),
        "return_codes": dict(sorted(by_code.items())),
        "compiled_by_ibm": len(compiled),
        "ironwork_classes": dict(sorted(classes.items())),
        "not_clean": sorted(
            ({"program": l["program"], "ibm_return_code": l["return_code"], "ironwork": results.get(l["program"], "not run")}
             for l in compiled if results.get(l["program"], "not run") not in ("clean", "called", "compiled")),
            key=lambda x: x["program"]),
        "rejected_by_ibm": sorted(
            ({"program": l["program"] or l["file"], "ibm_return_code": l["return_code"], "first_message": l["first_severe"],
              "ironwork": results.get(l["program"], "not run") if l["program"] else "unnamed"}
             for l in listings if l["return_code"] is None or l["return_code"] > 4),
            key=lambda x: x["program"]),
    }

def main():
    p = argparse.ArgumentParser(usage=__doc__.split("usage: ")[1].split("\n\n")[0])
    p.add_argument("listings")
    p.add_argument("--results")
    p.add_argument("--json")
    p.add_argument("--source", default="")
    p.add_argument("--note", action="append", default=[])
    a = p.parse_args()
    listings = [read_listing(os.path.join(a.listings, n)) for n in sorted(os.listdir(a.listings))
                if os.path.isfile(os.path.join(a.listings, n))]
    if a.json:
        with open(a.json, "w") as f:
            notes = dict(n.split("=", 1) for n in a.note)
            json.dump({"source": a.source, "notes": notes, "listings": listings}, f, indent=1)
            f.write("\n")
    if not a.results:
        return 0
    summary = summarise(listings, read_results(a.results))
    print(f"{summary['listings']} listings from {', '.join(summary['compilers'])}; return codes "
          + ", ".join(f"{code}: {n}" for code, n in summary["return_codes"].items()))
    print(f"{summary['compiled_by_ibm']} compiled by IBM at 4 or less; ironwork: "
          + ", ".join(f"{cls} {n}" for cls, n in summary["ironwork_classes"].items()))
    for row in summary["not_clean"]:
        print(f"  {row['program']}\tIBM {row['ibm_return_code']}\tironwork {row['ironwork']}")
    return 0

if __name__ == "__main__":
    sys.exit(main())
