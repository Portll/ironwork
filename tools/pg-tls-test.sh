#!/bin/sh
# Runs the TLS build's live test against PostgreSQL 14 in a container that accepts TCP logins only
# over TLS, with a certificate for localhost from a throwaway CA.
# usage: tools/pg-tls-test.sh    (needs Docker and openssl; IRONWORK_PG_PORT picks the host port)
set -eu
name=ironwork-pg-tls-test
port=${IRONWORK_PG_PORT:-55434}
certs=$(mktemp -d)
docker rm -f "$name" >/dev/null 2>&1 || true
trap 'docker rm -f "$name" >/dev/null 2>&1; rm -rf "$certs"' EXIT

cat > "$certs/ca.cnf" <<'EOF'
[req]
distinguished_name = dn
x509_extensions = ca
prompt = no
[dn]
CN = ironwork test CA
[ca]
basicConstraints = critical, CA:TRUE
keyUsage = critical, keyCertSign, cRLSign
EOF
printf 'subjectAltName = DNS:localhost, IP:127.0.0.1\nextendedKeyUsage = serverAuth\n' > "$certs/server.ext"
openssl ecparam -name prime256v1 -genkey -noout -out "$certs/ca.key"
openssl req -x509 -new -sha256 -key "$certs/ca.key" -days 2 -config "$certs/ca.cnf" -out "$certs/ca.pem"
openssl ecparam -name prime256v1 -genkey -noout -out "$certs/server.key"
openssl req -new -key "$certs/server.key" -subj /CN=localhost -out "$certs/server.csr"
openssl x509 -req -sha256 -in "$certs/server.csr" -CA "$certs/ca.pem" -CAkey "$certs/ca.key" -CAcreateserial \
    -days 2 -extfile "$certs/server.ext" -out "$certs/server.pem" 2>/dev/null
printf 'local all all trust\nhostssl all all all scram-sha-256\n' > "$certs/pg_hba.conf"

# PostgreSQL refuses a key other users can read, so the files are copied in and given to postgres.
docker run -d --name "$name" -e POSTGRES_USER=ironwork -e POSTGRES_PASSWORD=ironwork -e POSTGRES_DB=ironwork \
    -p "127.0.0.1:$port:5432" -v "$certs:/certs:ro" --entrypoint sh postgres:14.19-bookworm -c \
    'mkdir -p /tls && cp /certs/server.pem /certs/server.key /certs/pg_hba.conf /tls/ && chown -R postgres /tls &&
     chmod 600 /tls/server.key && exec docker-entrypoint.sh postgres -c ssl=on -c ssl_cert_file=/tls/server.pem \
     -c ssl_key_file=/tls/server.key -c hba_file=/tls/pg_hba.conf' >/dev/null
until docker exec "$name" pg_isready -q -h 127.0.0.1 -U ironwork 2>/dev/null; do
    if [ "$(docker inspect -f '{{.State.Running}}' "$name")" != true ]; then docker logs "$name" 2>&1 | tail -5; exit 1; fi
    sleep 1
done
IRONWORK_PG_TLS_URL="postgres://ironwork:ironwork@localhost:$port/ironwork?sslmode=verify-full&sslrootcert=$certs/ca.pem" \
    cargo test --locked --manifest-path "$(dirname "$0")/../tls/Cargo.toml" postgresql_over_tls -- --nocapture
