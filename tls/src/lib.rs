//! TLS for ironwork's PostgreSQL backend, through rustls with the ring provider. It is a workspace
//! of its own, as `fuzz/` is, so that ironwork's own build keeps no dependencies.

use exec::sql::{Stream, Tls};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::io;
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;

/// Verifies the server against the PEM roots given, or else the Mozilla roots in `webpki-roots`.
pub struct Rustls;

fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

impl Tls for Rustls {
    fn wrap(&self, mut socket: TcpStream, host: &str, roots: Option<&Path>) -> io::Result<Box<dyn Stream>> {
        let mut store = RootCertStore::empty();
        match roots {
            Some(path) => {
                for cert in CertificateDer::pem_file_iter(path).map_err(|e| invalid(format!("{}: {e}", path.display())))? {
                    store.add(cert.map_err(invalid)?).map_err(invalid)?;
                }
            }
            None => store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
        }
        let config = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(invalid)?
            .with_root_certificates(store)
            .with_no_client_auth();
        let name = ServerName::try_from(host.to_owned()).map_err(invalid)?;
        let mut tls = ClientConnection::new(Arc::new(config), name).map_err(invalid)?;
        // The handshake runs now, so that a server the roots do not vouch for fails the connection
        // rather than its first query.
        while tls.is_handshaking() {
            tls.complete_io(&mut socket)?;
        }
        Ok(Box::new(StreamOwned::new(tls, socket)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
    use rustls::pki_types::PrivateKeyDer;
    use rustls::{ServerConfig, ServerConnection};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A CA's PEM, and a server certificate it signed for localhost and 127.0.0.1 with its key.
    fn authority() -> (String, CertificateDer<'static>, PrivateKeyDer<'static>) {
        let mut ca = CertificateParams::new(Vec::new()).expect("CA parameters");
        ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_key = KeyPair::generate().expect("CA key");
        let ca_cert = ca.self_signed(&ca_key).expect("CA certificate");
        let issuer = Issuer::new(ca, ca_key);
        let server_key = KeyPair::generate().expect("server key");
        let server = CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()]).expect("server parameters");
        let server_cert = server.signed_by(&server_key, &issuer).expect("server certificate");
        (ca_cert.pem(), server_cert.der().clone(), PrivateKeyDer::try_from(server_key.serialize_der()).expect("server key DER"))
    }

    /// A one-connection TLS echo server: it answers the four bytes it reads with "pong".
    fn serve(cert: CertificateDer<'static>, key: PrivateKeyDer<'static>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
        let port = listener.local_addr().expect("has an address").port();
        let config = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("protocols")
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .expect("server config");
        std::thread::spawn(move || {
            let (socket, _) = listener.accept().expect("accepts");
            let mut tls = StreamOwned::new(ServerConnection::new(Arc::new(config)).expect("session"), socket);
            let mut ping = [0u8; 4];
            if tls.read_exact(&mut ping).is_ok() {
                let _ = tls.write_all(b"pong");
                let _ = tls.flush();
            }
        });
        port
    }

    fn roots_file(pem: &str) -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("ironwork-tls-test-{}-{n}.pem", std::process::id()));
        std::fs::write(&path, pem).expect("writes the roots");
        path
    }

    #[test]
    fn a_server_the_roots_vouch_for_is_reached() {
        let (ca, cert, key) = authority();
        let port = serve(cert, key);
        let roots = roots_file(&ca);
        let socket = TcpStream::connect(("127.0.0.1", port)).expect("connects");
        let mut stream = Rustls.wrap(socket, "localhost", Some(&roots)).expect("verified");
        stream.write_all(b"ping").expect("writes");
        stream.flush().expect("flushes");
        let mut pong = [0u8; 4];
        stream.read_exact(&mut pong).expect("reads");
        assert_eq!(&pong, b"pong");
    }

    #[test]
    fn a_wrong_name_or_unknown_issuer_is_refused() {
        let (ca, cert, key) = authority();
        let roots = roots_file(&ca);
        let port = serve(cert.clone(), key.clone_key());
        let wrong_name = Rustls.wrap(TcpStream::connect(("127.0.0.1", port)).expect("connects"), "db.example", Some(&roots));
        let wrong_name = wrong_name.err().expect("refused").to_string();
        assert!(wrong_name.contains("not valid for name"), "{wrong_name}");
        let port = serve(cert, key);
        let unknown = Rustls.wrap(TcpStream::connect(("127.0.0.1", port)).expect("connects"), "localhost", None);
        let unknown = unknown.err().expect("refused").to_string();
        assert!(unknown.contains("UnknownIssuer"), "{unknown}");
    }

    /// Against a live server named by IRONWORK_PG_TLS_URL, which `tools/pg-tls-test.sh` starts;
    /// without it this passes without running.
    #[test]
    fn postgresql_over_tls() {
        let Ok(url) = std::env::var("IRONWORK_PG_TLS_URL") else { return };
        let postgres = exec::sql::Postgres::connect(&url, Some(&Rustls)).expect("connects over TLS");
        assert!(postgres.source().ends_with("over TLS"), "{}", postgres.source());
    }
}
