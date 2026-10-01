//! The JNI member: ironwork's own declaration of the JNI function table (`rt::jni`) in COBOL, for a
//! COPY JNI no library answers.

use rt::jni::{FUNCTIONS, RESERVED};

pub fn member() -> String {
    let mut out = "       01  JNIENV POINTER.\n       01  JNINATIVEINTERFACE.\n".to_owned();
    for _ in 0..RESERVED {
        out.push_str("           02 POINTER.\n");
    }
    for f in FUNCTIONS {
        out.push_str(&format!("           02 {f} FUNCTION-POINTER.\n"));
    }
    out
}
