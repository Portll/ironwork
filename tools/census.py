#!/usr/bin/env python3
"""Run `ironwork check` over a fixed sample of a COBOL corpus and tally the first reason each
program is refused.

Each directory under the corpus root is a repository, and each program is given, as -I libraries
in sorted order, the directories a z/OS build of its repository would concatenate in SYSLIB:

- every directory of the repository holding a .cpy or .copy file;
- every directory holding a member the program copies, directly or through another member. A
  COPY or EXEC SQL INCLUDE of NAME finds a file of the repository named NAME, or NAME with an
  extension ironwork tries or a BMS map's (.cpy, .copy, .cbl, .cob, .bms, in any case); where
  NAME is a path relative to a library, as in `COPY "swd/mod/int/v"`, the library is the
  directory that path starts from.

A .cbl or .cob file holding a PROGRAM-ID is a program, never a member, since SYSLIB holds
copybooks; an absolute or backslashed path names no member.

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

MEMBER_EXTENSIONS = ("", ".cpy", ".copy", ".cbl", ".cob", ".bms")
COPIES = re.compile(r"""(?<!\S)(?:COPY|INCLUDE)\s+("[^"\n]+"|'[^'\n]+'|[^\s"'.]+(?:\.[^\s"'.]+)*)""", re.I)

def read(path):
    try:
        with open(path, "rb") as f:
            return f.read().decode("latin-1")
    except OSError:
        return ""

def copied_names(path):
    """The names a file's COPY and EXEC SQL INCLUDE statements give, comment lines aside."""
    lines = [line.split("*>")[0] for line in read(path).splitlines() if not (len(line) > 6 and line[6] in "*/")]
    return {m.group(1).strip("'\"") for m in COPIES.finditer("\n".join(lines))}

class Repository:
    """A repository's files, found by lowercased file name."""

    def __init__(self, files):
        self.by_name = collections.defaultdict(list)
        for path in files:
            self.by_name[os.path.basename(path).lower()].append(path)
        self.programs = {}

    def is_program(self, path):
        if not path.lower().endswith((".cbl", ".cob")):
            return False
        if path not in self.programs:
            self.programs[path] = re.search(r"PROGRAM-ID", read(path), re.I) is not None
        return self.programs[path]

    def members(self, name):
        """Each (library, file) that holds the member `name`."""
        name = name[2:] if name.startswith("./") else name
        if not name or name.startswith("/") or "\\" in name:
            return
        for extension in MEMBER_EXTENSIONS:
            relative = (name + extension).lower()
            for path in self.by_name.get(os.path.basename(relative), ()):
                if path.lower().endswith(os.sep + relative) and not self.is_program(path):
                    yield path[: len(path) - len(relative) - 1], path

def member_libraries(program, repository):
    """The libraries that hold what `program` copies, and what those members copy in turn."""
    libraries, seen, pending = set(), {program}, [program]
    while pending:
        for name in copied_names(pending.pop()):
            for library, member in repository.members(name):
                libraries.add(library)
                if member not in seen and not member.lower().endswith(".bms"):
                    seen.add(member)
                    pending.append(member)
    return libraries

def repository_files(root):
    files = collections.defaultdict(list)
    for dirpath, _, names in os.walk(root):
        repo = os.path.relpath(dirpath, root).split(os.sep)[0]
        files[repo].extend(os.path.join(dirpath, n) for n in names)
    return files

RULES = [
    (r"(\S+) is not a statement ironwork for COBOL supports yet", r"statement \1"),
    (r"the (\S+) SECTION is not supported yet", r"\1 SECTION"),
    (r"FUNCTION (\S+) is not supported yet", r"FUNCTION \1"),
    (r"FUNCTION (\S+): neither an intrinsic function", r"FUNCTION \1"),
    (r"PICTURE .*: edited pictures are not supported yet", "edited PICTURE"),
    (r"PICTURE .*: scaling position P", "PICTURE with P"),
    (r"(\S+) is not a data description clause", r"data clause \1"),
    (r"USAGE (\S+) is not supported yet", r"USAGE \1"),
    (r"COPY (\S+): no such member", "COPY member not found"),
    (r"EXEC (\S+) needs a precompiler", r"EXEC \1 (precompiler)"),
    (r"(.+) is not supported yet", r"\1"),
    (r"\S+ is a reserved word, so it cannot name", "reserved word as a name"),
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
    files, repositories = repository_files(root), {}
    tally, accepted = collections.Counter(), 0
    for path in sample:
        repo = os.path.relpath(path, root).split(os.sep)[0]
        if repo not in repositories:
            repositories[repo] = Repository(files[repo])
        dirs = libraries.get(repo, set()) | member_libraries(path, repositories[repo])
        flags = [arg for d in sorted(dirs) for arg in ("-I", d)]
        try:
            # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-tainted-env-args.dangerous-subprocess-use-tainted-env-args -- an argument list, no shell
            r = subprocess.run([binary, "check", path, *flags], capture_output=True, text=True, timeout=20)
        except subprocess.TimeoutExpired:
            tally["(timeout)"] += 1
            continue
        if r.returncode in (0, 4):
            accepted += 1
        else:
            tally[reason(r.stderr)] += 1
    print(f"{accepted} of {len(sample)} programs compile ({100 * accepted / len(sample):.1f}%), sample of {len(population)} (seed {seed})")
    for why, n in tally.most_common(40):
        print(f"{n:6d}  {why}")

if __name__ == "__main__":
    main()
