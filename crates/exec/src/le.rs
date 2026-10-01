//! Language Environment callable services are `rt::le`; the CALL side that builds their arguments
//! is machine/le_services.rs.

pub use rt::le::*;

#[cfg(test)]
mod tests;
