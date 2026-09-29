#!/usr/bin/env python3
"""Run `ironwork check` over a fixed sample of a COBOL corpus and tally the first reason each
program is refused.

usage: census.py <ironwork binary> <corpus dir> [sample size] [seed]
"""
import collections, os, random, re, subprocess, sys

def copy_libraries(root):
    """For each repository under the corpus root, the directories that hold copybooks."""
    libraries = collections.defaultdict(set)
    for dirpath, _, files in os.walk(root):
        if any(f.lower().endswith((".cpy", ".copy")) for f in files):
            repo = os.path.relpath(dirpath, root).split(os.sep)[0]
            libraries[repo].add(dirpath)
    return libraries

def programs(root):
    for dirpath, _, files in os.walk(root):
        for f in files:
            if f.lower().endswith((".cbl", ".cob")):
                yield os.path.join(dirpath, f)

RULES = [
    (r"(\S+) is not a statement ironwork for COBOL supports yet", r"statement \1"),
    (r"the (\S+) SECTION is not supported yet", r"\1 SECTION"),
    (r"FUNCTION (\S+) is not supported yet", r"FUNCTION \1"),
    (r"PICTURE .*: edited pictures are not supported yet", "edited PICTURE"),
    (r"PICTURE .*: scaling position P", "PICTURE with P"),
    (r"(\S+) is not a data description clause", r"data clause \1"),
    (r"USAGE (\S+) is not supported yet", r"USAGE \1"),
    (r"COPY (\S+): no such member", "COPY member not found"),
    (r"EXEC (\S+) needs a precompiler", r"EXEC \1 (precompiler)"),
    (r"(.+) is not supported yet", r"\1"),
    (r"\S+ is not defined", "undefined name"),
    (r"\S+ is ambiguous", "ambiguous name"),
    (r"no paragraph named", "undefined paragraph"),
    (r"expected ([^,]+), found .*", r"expected \1"),
]

def reason(stderr):
    first = stderr.strip().splitlines()[0] if stderr.strip() else "(no message)"
    message = re.sub(r"^[^:]*(:\d+:\d+)?:\s*", "", first)
    for pattern, replacement in RULES:
        m = re.search(pattern, message)
        if m:
            return m.expand(replacement)
    return re.sub(r"\d+", "N", message)[:80]

def main():
    binary, root = sys.argv[1], sys.argv[2]
    size = int(sys.argv[3]) if len(sys.argv) > 3 else 3000
    seed = int(sys.argv[4]) if len(sys.argv) > 4 else 20260927
    population = sorted(programs(root))
    sample = random.Random(seed).sample(population, min(size, len(population)))
    libraries = copy_libraries(root)
    tally, accepted = collections.Counter(), 0
    for path in sample:
        repo = os.path.relpath(path, root).split(os.sep)[0]
        flags = [arg for d in sorted(libraries.get(repo, ())) for arg in ("-I", d)]
        try:
            r = subprocess.run([binary, "check", path, *flags], capture_output=True, text=True, timeout=20)
        except subprocess.TimeoutExpired:
            tally["(timeout)"] += 1
            continue
        if r.returncode == 0:
            accepted += 1
        else:
            tally[reason(r.stderr)] += 1
    print(f"{accepted} of {len(sample)} programs compile ({100 * accepted / len(sample):.1f}%), sample of {len(population)} (seed {seed})")
    for why, n in tally.most_common(40):
        print(f"{n:6d}  {why}")

if __name__ == "__main__":
    main()
