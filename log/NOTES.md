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
  (driver `3450fb82` + Ranjit's `dump-callgraph` + a local fix for empty call
  graph dumps on cyclic graphs). See `~/Desktop/Code/flux/callgraph-dump-issues.md`.
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
| `block/decompress.rs` `decompress_internal`, `does_token_fit` | `token >> 4` → `token / 16`, `token & 0xF` → `token % 16`, masks in `does_token_fit` likewise; `does_token_fit` checked in `strict` overflow mode | flux doesn't model `>>`/`&`; `strict` supplies `u8 <= 255`. This was the key to uid 71: unbounded token fields broke every loop invariant (64 → 2 errors) |
- uid 71 `decompress_internal`: the per-def check takes ~90-150s, past
  `run_flux.py`'s 30s `CHECK_TIMEOUT`, so it was run with `cargo flux
  --only-check=def:...` directly.
