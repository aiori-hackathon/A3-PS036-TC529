//! Raw FFI bindings to libMTL.
//!
//! This crate will contain `bindgen`-generated bindings to the libMTL C library.
//! Implemented in **Phase 2**.
//!
//! # Safety
//!
//! All functions in this crate are `unsafe` FFI calls. Use [`climb_mtl`] for a safe wrapper.

/// Placeholder — replaced in Phase 2 by bindgen-generated bindings.
#[doc(hidden)]
pub fn placeholder() -> &'static str {
    "climb-mtl-sys: FFI bindings to libMTL (Phase 2)"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_compiles() {
        assert_eq!(
            placeholder(),
            "climb-mtl-sys: FFI bindings to libMTL (Phase 2)"
        );
    }
}
