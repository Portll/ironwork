#!/bin/sh
# Runs the PostgreSQL backend's live tests against a throwaway PostgreSQL 14 container.
# usage: tools/pg-test.sh    (needs Docker; IRONWORK_PG_PORT picks the host port, default 55433)
set -eu
name=ironwork-pg-test
port=${IRONWORK_PG_PORT:-55433}
docker rm -f "$name" >/dev/null 2>&1 || true
docker run -d --name "$name" -e POSTGRES_USER=ironwork -e POSTGRES_PASSWORD=ironwork -e POSTGRES_DB=ironwork \
    -p "127.0.0.1:$port:5432" postgres:14.19-bookworm >/dev/null
trap 'docker rm -f "$name" >/dev/null' EXIT
# The image's first-start server listens only on its socket, so TCP readiness means the real one.
until docker exec "$name" pg_isready -q -h 127.0.0.1 -U ironwork 2>/dev/null; do
    if [ "$(docker inspect -f '{{.State.Running}}' "$name")" != true ]; then docker logs "$name" 2>&1 | tail -5; exit 1; fi
    sleep 1
done
IRONWORK_PG_URL="postgres://ironwork:ironwork@127.0.0.1:$port/ironwork" cargo test -p ironwork-exec --locked sql::postgres
