//! CENTRIFUGE: flux-only refinement-level definitions and extern specs.
//! Nothing here affects compiled code.

flux_rs::defs! {
    // Bytes `write_integer(n)` emits: one 0xFF per full 255, plus a final byte.
    fn bytes_for_int(n: int) -> int { n / 255 + 1 }

    // `p` can be read for `n` bytes (flux-core's `valid`, which isn't exported).
    fn readable(p: ptr, n: int) -> bool {
        0 < p.base && p.base <= p.addr && 0 <= p.size && n <= p.size
    }

    // Loop-invariant qualifiers. A local like `end = start + k` is indexed by the
    // sum, so the sums are spelled out. Raw-pointer indices are split into their
    // int fields (base, addr, size) during inference, so these take ints, not `ptr`.
    qualifier LeSum(v: int, a: int, b: int) { v <= a + b }
    qualifier LtSum(v: int, a: int, b: int) { v < a + b }
    qualifier Pos(x: int) { 0 < x }
    qualifier CoversSum(size: int, v: int, a: int, b: int) { a + b <= size + v }
    qualifier Lockstep(a: int, b: int, c: int, d: int) { a - b == c - d }
    qualifier Mod16(x: int, y: int) { (x - y) % 16 == 0 }
    qualifier EqInt(a: int, b: int) { a == b }
    qualifier LeInt(a: int, b: int) { a <= b }
    qualifier LtInt(a: int, b: int) { a < b }
    qualifier SumEq(a: int, b: int, c: int, d: int) { a + b == c + d }

    // `p` can be written for `n` bytes.
    fn writable(p: ptr, n: int) -> bool { readable(p, n) }

    // The allocations `p` and `q` point into, [base, addr + size), don't overlap.
    // This is Evan's region form of the non-overlap assumption: it survives `.add`
    // because `add` keeps `base` and `addr + size`.
    fn disjoint(p: ptr, q: ptr) -> bool {
        p.addr + p.size <= q.base || q.addr + q.size <= p.base
    }

    fn min(a: int, b: int) -> int { if a <= b { a } else { b } }

    // Bytes `copy_from_dict` copies: the match, cut off at the end of the dictionary.
    fn dict_copied(match_length: int, offset: int, out_len: int) -> int {
        min(match_length, offset - out_len)
    }

    // `n` rounded up to a multiple of 16: what a 16-byte wild copy of `n` bytes touches.
    fn wild16(n: int) -> int { (n + 15) / 16 * 16 }

    // Bytes `copy_literals_wild(.., len)` actually copies (it over-copies in 8-byte steps).
    fn wild_len(len: int) -> int {
        if len <= 8 { 8 } else if len <= 16 { 16 } else if len <= 24 { 24 } else { len }
    }
}

