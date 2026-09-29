// The command-line driver is ironwork's own; `ironwork_tls` makes it hand its PostgreSQL backend
// the rustls provider this crate defines.
fn main() {
    println!("cargo::rustc-cfg=ironwork_tls");
}
