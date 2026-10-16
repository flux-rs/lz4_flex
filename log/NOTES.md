# Harness notes — lz4_flex

Decisions that don't fit a DONE/ABORT checkpoint message.

## Phase 1 (types)

| uid | type | outcome |
|---|---|---|
| 0 | `sink::SliceSink` | DONE: `refined_by(pos, cap)`, `invariant(pos <= cap)`; `Sink` trait gets assoc `pos`/`cap` + method specs |
| 1 | `sink::PtrSink` | ABORT (see ABORT.md): `refined_by(out: ptr, pos, cap)` + region invariant; `from_vec` and one `nonoverlapping` unprovable |
| 2 | `block::hashtable::HashTable4KU16` | DONE, no annotation: `Box<[u16; 4096]>`, length fixed by the type. Remaining errors are method-level (Phase 2): `get_at`/`put_at` need `hash < 65536` on the `HashTable` trait; `new()` needs `vec![0; N].into_boxed_slice()` to have length `N` for `try_into().unwrap()` |
| 3 | `block::hashtable::HashTable4K` | DONE, no annotation (as uid 2; `Box<[u32; 4096]>`) |
| 4 | `block::hashtable::HashTable8K` | DONE, no annotation (as uid 2; `Box<[u32; 8192]>`, shift 3, so also `hash < 65536`) |
| 5 | `block::DecompressError` | DONE: SAFE unannotated |
| 6 | `block::CompressError` | DONE: SAFE unannotated |
| 7 | `block::compress::CompressTable` | DONE: SAFE unannotated |

Crate-wide errors after Phase 1: 306 (baseline 357).

## Setup changes (not uid-specific)

- `src/lib.rs`: `#![cfg_attr(flux, allow(unused_features))]`. Flux injects
  `#![feature(custom_inner_attributes)]`, and once a check has no other errors
  `deny(warnings)` fails the build on the unused-feature lint.
- Flux driver: local branch `kris-gaudel/auto-strong-callgraph` in the flux repo
  (auto-strong-returned + Ranjit's `dump-callgraph` + a local fix for empty call
  graph dumps on cyclic graphs). See `~/Desktop/Code/flux/callgraph-dump-issues.md`.
  Rebased 2026-10-15; see "Flux upgrade" below.
- Running flux needs the sandbox off when `FLUXFLAGS` changes: the flux deps
  rebuild and flux writes a tempdir into their `~/.cargo` checkout.

## Phase 2 observations

- uid 11 `copy_literals_wild`: the `debug_assert!`s bound the copy by
  `round_up_8(len)`, which is 0 when `len == 0`, but that arm copies 8 bytes.
  The spec uses the true amount (`wild_len` in `src/flux_specs.rs`). Whether
  `compress_internal` ever calls it with `len == 0` near the end of the input is
  unchecked, because `compress_internal` is `trusted` (flux ICE).
- Loop invariants (found on uid 16 `count_same_bytes`): flux's default
  qualifiers can't express `cur <= start + k` when `end = start + k` is a local,
  because the local is indexed by the sum. Raw pointers in loops need their
  bounds carried too. `src/flux_specs.rs` defines `LeSum`, `Pos` and
  `CoversSum`; attach them with `#[flux_rs::qualifiers(...)]`. Qualifier params
  must be `int`: a raw pointer's index is split into its `base`/`addr`/`size`
  fields during inference, so `ptr`-sorted qualifiers never match.
  `#[flux_rs::opts(scrape_quals = true)]` didn't find these.
- uid 61 `block::uncompressed_size`: rewrote `input.get(..4).ok_or(..)?` +
  `try_into::<&[u8; 4]>().unwrap()` as an explicit `len < 4` check + indexing
  (same result and error). An extern spec for `TryFrom<&[T]> for &[T; N]`
  (`succeeds(n, out) = out.is_ok == (n == N)`) **crashes flux**: the blanket
  `impl TryInto<U> for T` passes the *reference's* index, which is unit-sorted,
  into `succeeds(n: int, ..)`, and fixpoint dies with
  `Cannot unify Tuple0 with int`. Worth an upstream report.
- `cargo test` after the Phase 2 source changes: 22 unit + 39 integration pass.
  The 2 doctest failures are pre-existing: they use `lz4_flex::frame`, which the
  flipped default features (`std`, `checked-decode`) compile out.

## Source changes (all marked `CENTRIFUGE`)

| where | change | why |
|---|---|---|
| `src/lib.rs` | `cfg_attr(flux, allow(unused_features))`; `mod flux_specs` | flux-injected feature vs `deny(warnings)`; spec defs |
| `block/compress.rs` `push_byte`, `push_u16`, `write_integer`, `copy_literals_wild`, `handle_last_literals` | `output: &mut impl Sink` → `<S: Sink>(output: &mut S)` | specs must name `S` to use `S::pos`/`S::cap` |
| `block/mod.rs` `uncompressed_size` | explicit length check instead of `try_into::<&[u8; 4]>().unwrap()` | see uid 61 above |
| `block/decompress.rs` `decompress_internal` | `input_ptr_end` / `output_start_pos_ptr` derived from the first `as_ptr()` / `base_mut_ptr()` instead of a second call | fresh `as_ptr()` results get unrelated bases in flux |
| `block/decompress.rs` `decompress_internal` | `ptr as usize` comparisons/differences → pointer `<=` / `offset_from` | `ptr as usize` is an unrefined integer in flux |
| `block/hashtable.rs` `hash`, `hash5`, `get_at`/`put_at` ×3 | `>> 16` → `/ 65536`, `>> 48` → `/ 2^48`, `hash >> SHIFT` → `hash / 16` (`/ 8`), with `const _: () = assert!(1 << SHIFT == 16)` keeping the constants in sync; `hash`/`hash5` checked in `strict` overflow mode | flux doesn't model `>>` (and `1 << SHIFT` is opaque too) |
| `block/decompress.rs` `decompress_internal`, `does_token_fit` | `token >> 4` → `token / 16`, `token & 0xF` → `token % 16`, masks in `does_token_fit` likewise; `does_token_fit` checked in `strict` overflow mode | flux doesn't model `>>`/`&`; `strict` supplies `u8 <= 255`. This was the key to uid 71: unbounded token fields broke every loop invariant (64 → 2 errors) |
- uid 71 `decompress_internal`: the per-def check takes ~90-150s, past
  `run_flux.py`'s 30s `CHECK_TIMEOUT`, so it was run with `cargo flux
  --only-check=def:...` directly.

## Phase 2 result (2026-09-26)

Crate-wide: **357 → 29 errors, 0 ICEs** (with `compress_internal` and the
`frame` `Read` impl still `trusted` for flux ICEs 1 and 2). Every remaining
error is logged in `ABORT.md`:

| category | errors | functions |
|---|---:|---|
| different-base `copy_nonoverlapping` / `disjoint` (the documented assumption) | 13 | `PtrSink::extend_from_slice_wild`, `push_u16`, `copy_literals_wild` (4), `read_u16/u32/usize_ptr`, `copy_from_dict`, `read_match_offset`, `decompress_internal` (2) |
| **real bug** (out-of-bounds `ptr::add`) | 1 | `duplicate_overlapping`; test in `tests/duplicate_overlapping_ub.rs` |
| flux-alloc gaps (`Vec` capacity, `vec!` → `Box<[T; N]>`) | 7 | `PtrSink::from_vec` (4), `HashTable*::new` (3) |
| loop invariant flux can't express (const-generic modulus; generic assoc `pos`) | 5 | `fastcpy_unsafe::wild_copy_from_src` (4), `handle_last_literals` (via `write_integer`) |
| blocked on a flux bug | 3 | `compress_into_vec_with_dict` (ICE 1), `decompress_into{,_with_dict}` (flux-rs/flux#1859, `infer.rs:887`) |

Functions verified with no assumptions include the whole decompression path's
bounds reasoning (`decompress_internal` apart from its 2 input/output copies,
`duplicate`, `wild_copy_from_src_16`, `copy_from_dict`'s bounds, the input
readers), both sinks, both `slice_copy`s, the compression helpers'
bounds, the hash tables' indexing, and `uncompressed_size`.

## Flux upgrade (2026-10-15)

Rebased the driver to pick up the fix for ICE 1
([flux-rs/flux#1784](https://github.com/flux-rs/flux/issues/1784), fixed by
flux-rs/flux#1851).

- Driver: `kris-gaudel/auto-strong-callgraph` rebased onto upstream `main`
  `736010e1` (now `b472d892`; the old tip is kept as
  `kris-gaudel/auto-strong-callgraph-prerebase`). The 19 local commits carried over
  unchanged apart from import/doc-comment conflicts. Installed with `cargo xtask install`.
- Toolchain: `nightly-2026-08-21` → `nightly-2026-10-11` (`rust-toolchain.toml`,
  to match the driver).
- flux libs: `cargo update -p flux-rs --precise 736010e1…`. `Cargo.lock` is
  gitignored, so the pin is local only; the old `c4605619` libs fail to compile
  against the new toolchain.
- liquid-fixpoint: the driver now needs the `:self` feature (flux-rs/flux#1829),
  so the June 8 `fixpoint` binary fails with `expected value at line 1 column 1`.
  Installed the 2026-10-13 nightly into `~/.cargo/bin/fixpoint` (old binary at
  `fixpoint.bak-20260608`).

### ICE status (each minimal repro rerun on the new driver)

| ICE | upstream | status |
|---|---|---|
| 1 `marker_at_dominator` (`compress_internal`) | #1784 closed | **fixed**; `trusted` removed from `compress_internal` |
| 2 `type_env.rs` free vars (frame `read_to_string`/`read_to_end`) | #1785 open | **accepted**: stays `trusted`, reasons cite the issue. `frame` is `cfg`'d out in this config anyway |
| 3 fixpoint `Tuple0` vs `int` (`TryFrom` extern spec) | #1786 open | **accepted**: `HashTable*::new` stays ABORTed (ABORT.md) |
| 4 `infer.rs:937` incompatible types (`decompress_into{,_with_dict}`) | #1859 open (filed 2026-10-16) | still errors; uids 79/81 stay ABORTed |

"Accepted" means: known, filed upstream, worked around (`trusted` or ABORT), not
a reason to stop. Don't rerun `flux_thaw` on these.

New flux problems found on this driver, filed 2026-10-16: #1860 (`static`
initializer `MulWithOverflow`), #1861 (loop bound indexed by an expression; the
`count_same_bytes` regression), #1862 (`<<`-initialised local breaks an unrelated
invariant), #1863 (ICE: `qualifier!` with a generic's assoc refinement).

### Full sweep after the upgrade: 29 → 57 errors, 0 ICEs

All 29 earlier errors are unchanged. New:

| errors | where | cause | action |
|---:|---|---|---|
| 14 | `compress_internal` | newly checked (was `trusted` for ICE 1) | Phase 2: redo uids 22, 108, 109, 123, 125, 127, 128, 150, 151, then 132 |
| 13 | `count_same_bytes` (uid 16, was DONE) | **regression**: the loop invariant is no longer inferred. Commit `b01d35b` attached the `qualifiers` to the `safe-encode` copy (compiled out, and in a `crate::flux_specs::` path form flux rejects), so the unsafe copy never had them; the old kvar encoding inferred the invariant anyway, the `:self` encoding doesn't. Adding `LeSum, Pos, CoversSum` or the full decompress set to the unsafe copy still leaves 13 errors | Phase 2: redo uid 16 |
| 1 | `block/mod.rs:78` `static LZ4_64KLIMIT = (64 * 1024) + …` | **new**: `unsupported statement` (`MulWithOverflow`); the driver now checks this initializer | rewrite as `65536 + (MFLIMIT - 1)` or ABORT |

### uid 16 `count_same_bytes` re-verified (2026-10-15): two loop-invariant rules on the new driver

Found by reducing the regression in a scratch crate and running `fixpoint` on the
dumped constraint (`FLUXFLAGS=-Fdump-constraint`). Removing `:self` doesn't help,
so it isn't the new encoding as such.

> **Correction (2026-10-16):** rule 1 is wrong. On a clean reduction the
> original `LeSum` order works, and so do no qualifiers at all; rule 2 is the
> whole cause (flux-rs/flux#1861). `SizeCovers` is still what `count_same_bytes`
> uses, together with `match_end`.

1. ~~**A qualifier's first parameter must be loop-varying.**~~ Fixpoint binds a
   qualifier's first argument to one of the kvar's own (`:self`) arguments, i.e.
   the values that change around the loop. `LeSum(v, a, b) { v <= a + b }` with
   `v = end` (fixed) never instantiates. Put the varying value first:
   `SizeCovers(size, cur, end) { end <= cur + size }` (`src/flux_specs.rs`).
2. **A loop bound must be a named value, not an expression.** `input_end = *cur +
   a.saturating_sub(b).min(c)` is indexed by `c + clamp(..)`, which isn't a kvar
   argument, so no invariant can mention it. Fix: compute it in a helper whose
   spec returns a fresh `usize{v: ..}` (`match_end`, a CENTRIFUGE change; same
   computation).

The old driver hid both (its `KVarEncoding::Conj` gave every argument its own
kvar). Expect the same in `compress_internal`'s loops.

## Rework after the upgrade (2026-10-15): 57 → 37 errors, 0 ICEs

| item | result |
|---|---|
| `LZ4_64KLIMIT` | DONE: `static` → `const` (unused) |
| uid 16 `count_same_bytes` | DONE: `match_end` helper + `SizeCovers` (see above) |
| uid 9 `write_integer` | DONE (was PARTIAL): loop-free rewrite (`ptr::write_bytes` for the `0xFF` run), exact `pos` postcondition; `write_integer_matches_loop` test, Miri-clean |
| uid 20 `handle_last_literals` | DONE (was PARTIAL): follows from uid 9 |
| `compress_internal` (uids 22, 108, 109, 123, 125, 127, 128, 150, 151) | ABORT, 14 → 9: all input-side bounds verify; 7 output-capacity errors need a generic-sink loop invariant (inexpressible; `qualifier!` crashes flux), 2 ext-dict candidate errors need a hash-table invariant |
| uid 132 `compress_into_vec_with_dict` | ABORT: `compress_internal`'s `Result` spec verifies but triggers flux-rs/flux#1859 at a caller, and flux-alloc has no `Vec` capacity |

Other source changes (all `CENTRIFUGE`): `1 << INCREASE_STEPSIZE_BITSHIFT` /
`>> INCREASE_STEPSIZE_BITSHIFT` → `32` / `/ 32` with a const assert (a shifted
value in a loop broke an unrelated invariant, flux-rs/flux#1862);
`get_maximum_output_size` spec via `flux_specs::max_out`; `backtrack_match` now
requires `literal_start <= cur` (all callers satisfy it) and keeps it.

The 37 remaining errors, by category (all logged in ABORT.md):

| category | errors |
|---|---:|
| different-base `copy_nonoverlapping` / `disjoint` (documented assumption) | 13 |
| `compress_internal`: generic-sink capacity invariant (7) + hash-table contents (2) | 9 |
| flux-alloc gaps (`Vec` capacity, `vec!` → `Box<[T; N]>`) | 7 |
| `fastcpy_unsafe::wild_copy_from_src` const-generic modular invariant | 4 |
| blocked on flux bugs: #1859 (`decompress_into{,_with_dict}`), uid 132 | 3 |
| **real bug** (`duplicate_overlapping`, advisory filed 2026-10-16) | 1 |

Nothing is left to try with the current flux: every remaining error is either a
documented assumption, a real bug, or blocked on a flux/flux-alloc limitation.

## Wrap-up (2026-10-16): lz4_flex Fluxing done

37 errors, 0 ICEs on the `736010e1` driver; every error is in ABORT.md. One real
bug (`duplicate_overlapping`), reported to lz4_flex as a private security
advisory. Flux issues filed: #1859–#1863 (plus #1784–#1786 earlier). Reopen when:
- #1859 is fixed: re-check uids 79/81 and restore `compress_internal`'s `Result`
  spec (text in ABORT.md, uid 132).
- #1863 is fixed or flux gets loop-invariant annotations: the 7
  `compress_internal` output-capacity errors.
- lz4_flex fixes `duplicate_overlapping`: drop that error.
