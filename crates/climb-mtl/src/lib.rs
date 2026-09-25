//! Safe Rust API for MTL mode operations.
//!
//! This crate provides a safe, idiomatic Rust wrapper around the raw FFI bindings
//! in [`climb_mtl_sys`]. Implemented in **Phase 3**.

/// Placeholder — replaced in Phase 3 by the safe MTL wrapper.
#[doc(hidden)]
pub fn placeholder() -> &'static str {
    "climb-mtl: safe MTL API wrapper (Phase 3)"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_compiles() {
        assert_eq!(placeholder(), "climb-mtl: safe MTL API wrapper (Phase 3)");
    }
}
