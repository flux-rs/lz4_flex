# lz4_flex — Flux candidacy triage

Baseline taken 2026-09-21 against flux driver `916dea78`
(branch `kris-gaudel/auto-strong-returned`), toolchain `nightly-2026-08-21`.
Default feature set: `std, safe-encode, safe-decode, frame, checked-decode`.

## Verdict: strong candidate — the best of the four crates tried so far

42 refinement errors across 41 unique source locations, in **exactly two
categories**, with **zero pointer-precondition errors**. For comparison with
`../../current_errors.md`:

| crate        | total errors | categories | pointer preconditions (unique locs) |
|--------------|-------------:|-----------:|------------------------------------:|
| smallvec     |          181 |          7 |                           157 (42)  |
| bytes        |          120 |          9 |                            76 (22)  |
| tinyvec      |           74 |          8 |                              0      |
| **lz4_flex** |       **42** |      **2** |                          **0**      |

(Category and pointer counts for the first three are from `current_errors.md`;
it does not report unique-location totals for the other categories, so those
columns are omitted rather than guessed. lz4_flex's 42 errors span 41 unique
locations.)

The near-1:1 error-to-location ratio matters: in smallvec/bytes a single
`ptr::copy` fails 4 sub-conditions, so the raw counts overstate the real work.
Here every error is its own distinct obligation.

## Error breakdown

| file                        | underflow | out-of-bounds | total |
|-----------------------------|----------:|--------------:|------:|
| src/block/decompress_safe.rs |         7 |             0 |     7 |
| src/block/hashtable.rs       |         0 |             6 |     6 |
| src/frame/compress.rs        |         6 |             0 |     6 |
| src/sink.rs                  |         2 |             4 |     6 |
| src/block/compress.rs        |         3 |             2 |     5 |
| src/frame/decompress.rs      |         5 |             0 |     5 |
| src/fastcpy.rs               |         2 |             2 |     4 |
| src/frame/header.rs          |         2 |             1 |     3 |
| **TOTAL**                    |    **27** |        **15** | **42** |

Spread over 22 functions. No `unwrap`/`expect` preconditions, no missing
associated refinements, no unsupported casts, no postcondition failures.

## Why it looks tractable

1. **Safe by default.** The crate's default features select the safe
   encode/decode paths; the 90 `unsafe` occurrences live in
   `decompress.rs` / `fastcpy_unsafe.rs`, which are `cfg`'d out. That is
   precisely what kept the pointer-precondition category empty.
2. **`hashtable.rs` is 6 errors of one shape** — `self.dict[hash >> SHIFT]`
   indexing a fixed-size array, repeated across `HashTable4K`/`4KU16`/`8K`
   × `get_at`/`put_at`. One `refined_by`/`invariant` pattern should clear
   all six.
3. **Underflows are the classic `len - offset` shape**, e.g.
   `input.len() - END_OFFSET`, `input.len() - start`. These need a
   precondition relating the two, which is exactly what Phase 1 type
   invariants are for.
4. **`SliceSink` (6 errors) is the central abstraction** — refining it with
   `refined_by(pos, cap)` plus `invariant(pos <= cap)` likely cascades into
   `decompress_safe.rs` and `fastcpy.rs`.

## Caveats

- **3 functions are trusted** to get past ICEs (see the `flux_thaw` commit):
  `block::compress::compress_internal`, and `read_to_string`/`read_to_end`
  in `frame/decompress.rs` plus a `trusted_impl` on the enclosing `io::Read`
  impl. `compress_internal` is the main compression driver, so that one is a
  real coverage gap, not a cosmetic one. Two distinct flux bugs:
  `checker.rs:2627 impossible case reached` and `type_env.rs:564`.
- The 42-error figure is for the **safe** configuration. Building with
  `--no-default-features` to reach the unsafe paths would reintroduce the
  pointer-precondition category and is a separate assessment.
- Errors are counted from a run that aborts compilation; later passes may
  surface more once earlier ones are discharged.

## Reproduce

```bash
cd crates/lz4_flex
cargo flux                                   # 42 errors, 0 ICEs
cargo flux check -p lz4_flex --only-check='def:<block::hashtable::HashTable4K as block::hashtable::HashTable>::get_at'
```

Setup note: the flux deps are pinned to `rev = "916dea78"` to match the
installed driver. Flux `main` (`c4605619`) has diverged — its `flux-alloc`
fails to build against this driver (`no AllocatorClone in alloc`).
