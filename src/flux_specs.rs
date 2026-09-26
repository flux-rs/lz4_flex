//! CENTRIFUGE: flux-only refinement-level definitions and extern specs.
//! Nothing here affects compiled code.

flux_rs::defs! {
    // Bytes `write_integer(n)` emits: one 0xFF per full 255, plus a final byte.
    fn bytes_for_int(n: int) -> int { n / 255 + 1 }
}
