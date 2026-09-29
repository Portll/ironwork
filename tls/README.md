# ironwork with TLS

The same `ironwork` command, built with TLS for `--sql-db`. ironwork's own build keeps no
dependencies, so TLS lives here, in a workspace of its own, as `fuzz/` does.

    cargo build --release --manifest-path tls/Cargo.toml

The binary is `tls/target/release/ironwork` (or under `CARGO_TARGET_DIR`). Over TCP it connects
with `sslmode=verify-full` unless the URL says `sslmode=disable`: it checks the server's certificate
chain and its name, against `sslrootcert=path.pem` or else the Mozilla roots in `webpki-roots`.

    ironwork run PAY.cbl --sql-db 'postgres://app@db.example/payroll?sslrootcert=/etc/ssl/db-ca.pem'

TLS is rustls with the ring provider. `cargo test --manifest-path tls/Cargo.toml` checks a verified
connection, a wrong name and an unknown issuer against an in-process server;
[`tools/pg-tls-test.sh`](../tools/pg-tls-test.sh) checks PostgreSQL 14 accepting TCP logins only over
TLS.

What this build adds, and the licences it carries, are in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
