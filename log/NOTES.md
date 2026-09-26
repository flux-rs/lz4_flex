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
