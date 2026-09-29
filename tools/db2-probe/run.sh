#!/bin/sh
# Runs the probes that settle sql-runtime.md's SQ assumptions against Db2 for Linux, and prints what
# Db2 answered. observed-12.1.5.txt is the output of 2026-09-30.
#
# It needs a running Db2 Community Edition container whose test database is TESTDB. Starting one
# accepts IBM's licence for it (for 12.1.5, License Information L-VRXW-RZEJNU: internal
# non-production development and test, at most 4 cores and 8 GB), so read that first:
#
#   docker run -d --name ironwork-db2 --platform linux/amd64 --privileged=true --cpus=4 --memory=7g \
#       -e LICENSE=accept -e DB2INST1_PASSWORD=<password> -e DBNAME=testdb icr.io/db2_community/db2
#
# usage: tools/db2-probe/run.sh [container]    (default ironwork-db2)
set -eu
container=${1:-ironwork-db2}
here=$(dirname "$0")
docker exec "$container" mkdir -p /tmp/ironwork-probe
docker cp "$here/." "$container:/tmp/ironwork-probe/"
docker exec "$container" chown -R db2inst1 /tmp/ironwork-probe
docker exec "$container" su - db2inst1 -c '
    cd /tmp/ironwork-probe
    db2 connect to testdb >/dev/null
    db2 -tf setup.sql >/dev/null
    # db2 exits 2 on a warning, as the short INTO list in cols.sqc draws, so only gcc is checked.
    build() { db2 prep "$1.sqc" bindfile $2 >/dev/null; db2 bind "$1.bnd" >/dev/null
        gcc -I"$HOME/sqllib/include" -o "$1" "$1.c" -L"$HOME/sqllib/lib64" -ldb2 || exit 1; }
    for p in probe s1 cols; do build $p; done
    build iso "datetime iso"
    db2 prep whenever.sqc >/dev/null
    echo "== the SQLCA after each statement"; ./probe
    echo "== a normal end without COMMIT, then exit(8) (SQ1)"
    ./s1; db2 connect to testdb >/dev/null; echo "row kept: $(db2 -x "select count(*) from emp where id = 50")"
    ./s1 fail; db2 connect to testdb >/dev/null; echo "row kept: $(db2 -x "select count(*) from emp where id = 50")"
    echo "== DATETIME(ISO) (SQ11)"; ./iso
    echo "== an INTO list shorter than the select list"; ./cols
    echo "== blank-padded comparison (SQ12): $(db2 -x "select count(*) from emp where note = '"'"'SHORT   '"'"'")"
    echo "== the precompiler'"'"'s WHENEVER tests (SQ2)"
    grep -E "sqlcode|sqlwarn|goto" whenever.c
'
