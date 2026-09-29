# Third-party notices for the TLS build

The TLS build of ironwork for COBOL is licensed under AGPL-3.0-or-later, as ironwork is (see
[`../LICENSE`](../LICENSE) and [`../THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md)). It also
compiles in the crates below, each under its own licence, at the versions `Cargo.lock` pins. Each
crate's source, as fetched from crates.io, carries its licence text.

## Compiled into the binary

| Crate | Version | Licence | What it does here |
|---|---|---|---|
| rustls | 0.23.45 | Apache-2.0 OR ISC OR MIT | TLS 1.2 and 1.3 |
| rustls-webpki | 0.103.15 | ISC | Certificate chain and name verification |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | Certificate and key types, PEM reading |
| ring | 0.17.14 | Apache-2.0 AND ISC | Cryptography, including code derived from BoringSSL and OpenSSL |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | The Mozilla root certificates, used when no `sslrootcert` is given |
| untrusted | 0.9.0 | ISC | Safe parsing of untrusted input |
| subtle | 2.6.1 | BSD-3-Clause | Constant-time comparison |
| zeroize | 1.9.0 | Apache-2.0 OR MIT | Clearing secrets from memory |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | Lazy statics |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | Conditional compilation |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | The operating system's random numbers |
| libc | 0.2.189 | MIT OR Apache-2.0 | System calls, on Unix |
| wasi | 0.11.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | System calls, on WASI only |
| windows-sys, windows-targets and the windows_* crates | 0.52 | MIT OR Apache-2.0 | System calls, on Windows only |

## Used to build it, not compiled in

| Crate | Version | Licence |
|---|---|---|
| cc | 1.5.1 | MIT OR Apache-2.0 |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |

The tests also use rcgen and its dependencies to make throwaway certificates; none of them reach
the binary.
