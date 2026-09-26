//! CENTRIFUGE: flux-only refinement-level definitions and extern specs.
//! Nothing here affects compiled code.

flux_rs::defs! {
    // Bytes `write_integer(n)` emits: one 0xFF per full 255, plus a final byte.
    fn bytes_for_int(n: int) -> int { n / 255 + 1 }

    // Bytes `copy_literals_wild(.., len)` actually copies (it over-copies in 8-byte steps).
    fn wild_len(len: int) -> int {
        if len <= 8 { 8 } else if len <= 16 { 16 } else if len <= 24 { 24 } else { len }
    }
}
