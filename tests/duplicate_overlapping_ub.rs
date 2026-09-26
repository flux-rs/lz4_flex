//! CENTRIFUGE: regression test for out-of-bounds pointer arithmetic in the
//! unsafe block decoder's `duplicate_overlapping` (found by Flux; see
//! `lz4_flex-duplicate-overlapping-ub.md` and `log/ABORT.md`, uid 65).
//!
//! When a match of even length exactly fills the rest of the output buffer,
//! the loop test `output_ptr.add(1) < dst_ptr_end` computes a pointer two past
//! the end of the buffer, which is UB for `pointer::add`. The pointer is never
//! dereferenced, so a normal run passes; run under Miri to see the UB:
//!
//!     cargo +nightly miri test --test duplicate_overlapping_ub
//!
//! Only the unsafe decoder (no `safe-decode` feature) is affected.
#![cfg(not(feature = "safe-decode"))]

use lz4_flex::block::{decompress_into, DecompressError};

/// Sequence 1: token 0x10 = 1 literal, match length 4 (MINMATCH + 0);
///             literal b'a'; match offset 1 (u16 LE).
/// Sequence 2: token 0x00 = 0 literals, end of block.
const BLOCK: [u8; 5] = [0x10, b'a', 0x01, 0x00, 0x00];

#[test]
fn match_exactly_fills_output() {
    let mut out = [0u8; 5];
    // Under Miri this call reports UB in duplicate_overlapping.
    let r = decompress_into(&BLOCK, &mut out);
    assert!(matches!(r, Ok(5)), "{r:?}");
    assert_eq!(&out, b"aaaaa");
}

#[test]
fn match_exactly_fills_output_truncated_block() {
    // Same path without the trailing token: the UB happens before the decoder
    // notices the block is truncated.
    let mut out = [0u8; 5];
    let r = decompress_into(&BLOCK[..4], &mut out);
    assert!(matches!(r, Err(DecompressError::ExpectedAnotherByte)), "{r:?}");
    assert_eq!(&out, b"aaaaa");
}
