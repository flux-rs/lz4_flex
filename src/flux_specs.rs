//! CENTRIFUGE: flux-only refinement-level definitions and extern specs.
//! Nothing here affects compiled code.

flux_rs::defs! {
    // Bytes `write_integer(n)` emits: one 0xFF per full 255, plus a final byte.
    fn bytes_for_int(n: int) -> int { n / 255 + 1 }

    // `p` can be read for `n` bytes (flux-core's `valid`, which isn't exported).
    fn readable(p: ptr, n: int) -> bool {
        p.addr != 0 && p.base <= p.addr && 0 <= p.size && n <= p.size
    }

    // Loop-invariant qualifiers. A local like `end = start + k` is indexed by the
    // sum, so the sums are spelled out. Raw-pointer indices are split into their
    // int fields (base, addr, size) during inference, so these take ints, not `ptr`.
    qualifier LeSum(v: int, a: int, b: int) { v <= a + b }
    qualifier Pos(x: int) { 0 < x }
    qualifier CoversSum(size: int, v: int, a: int, b: int) { a + b <= size + v }

    // Bytes `copy_literals_wild(.., len)` actually copies (it over-copies in 8-byte steps).
    fn wild_len(len: int) -> int {
        if len <= 8 { 8 } else if len <= 16 { 16 } else if len <= 24 { 24 } else { len }
    }
}
