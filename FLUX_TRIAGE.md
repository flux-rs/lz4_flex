# lz4_flex — Flux candidacy triage

Baseline 2026-09-21. Driver `flux 3450fb82` (branch `kris-gaudel/auto-strong-returned`
rebased onto `main` c4605619), toolchain `nightly-2026-08-21`.

**Configuration matters enormously here — see "Two configurations" below.**

## Verdict: strong candidate

| | value |
|---|---|
| size (pointer config) | **2,910 LOC** across 8 files |
| `core::ptr` call sites | **65** (18 `copy_nonoverlapping`, 4 `copy`, 1 `write`, rest `.add`/`.offset_from`/`as_ptr`) |
| refinement errors | **357** over **120 unique locations** |
| of which pointer preconditions | **332** |
| distinct flux ICEs | **2** (1 live in this config, trusted out) |

Hits the ≤3000 LOC target and is dense in `core::ptr`. It does crash flux, but
only in **one** place: `block::compress::compress_internal` triggers
`checker.rs:2660 impossible case reached`. That one function is `trusted` to get
past it; everything else checks. See "ICEs" below — do not quote this crate as
ICE-free.

## The specs are the shape we wanted

| failed precondition | count |
|---|---:|
| `in_bounds(p) && count * T::size_of() <= p.size` | 151 |
| `p.base == op.base` (`offset_from` same-allocation) | 15 |
| `in_bounds(p) && count * T::size_of() <= p.addr - p.base` | 8 |
| `valid(p, T::size_of()) && aligned_to(p, T::align_of())` | 4 |
| (multi-line `requires` not parsed by the tally script) | 154 |

Essentially all of it is "is this pointer in bounds of its allocation" — no
aliasing, no lifetime, no temporal reasoning. Alignment barely appears (4) because
everything is `u8`, so `align_of() == 1` is trivially satisfied.

## Where the work is

| file | errors | unique locs |
|---|---:|---:|
| src/block/decompress.rs | 238 | 57 |
| src/fastcpy_unsafe.rs | 45 | 16 |
| src/block/compress.rs | 38 | 23 |
| src/sink.rs | 18 | 7 |
| src/block/hashtable.rs | 9 | 9 |
| src/fastcpy.rs | 8 | 7 |
| src/block/mod.rs | 1 | 1 |

Concentrated in a handful of functions: `decompress_internal` (122),
`duplicate_overlapping` (34), `copy_from_dict` (34), `wild_copy_from_src` (29),
`duplicate` (22). The 357→120 ratio is the usual inflation — one
`copy_nonoverlapping` fails `valid(src)`, `valid(dst)` and `nonoverlapping`
separately.

Likely leverage: `SliceSink` (7 locations) is the central abstraction; refining it
with `refined_by(pos, cap)` + `invariant(pos <= cap)` should cascade widely.
`hashtable.rs` is 9 errors of a single shape (`self.dict[hash >> SHIFT]` into a
fixed-size array, × 3 table types × get/put).

## `copy_nonoverlapping`: same base vs different base

Question from review: do the `copy_nonoverlapping` calls use pointers from the
**same** base (offset reasoning, good) or **different** bases (needs a
non-overlap assumption, bad)?

flux-core's `nonoverlapping(p, q, n)` compares `addr` only
(`p.addr + n <= q.addr || q.addr + n <= p.addr`). Every fresh pointer
(`slice.as_ptr()`, `&mut local as *mut _`) gets an existential `base == addr`
that is unrelated to every other pointer. So a different-base call can **never**
discharge `nonoverlapping` without an assumption. All 17 live sites currently
fail that conjunct (the 18th `copy_nonoverlapping` is commented out at
`fastcpy_unsafe.rs:58`).

| site | src → dst | base | why disjoint |
|---|---|---|---|
| `decompress.rs:49` `wild_copy_from_src_16` | `output - offset` → `output` | **same** | guarded by `output_ptr.offset_from(start) >= match_length + 15` (≥ 19 > 16) |
| `decompress.rs:286` | input → output | different | `&[u8]` vs `&mut [u8]` / fresh `Vec` |
| `decompress.rs:110` `copy_from_dict` | ext_dict → output | different | `&[u8]` vs `&mut [u8]` / fresh `Vec` |
| `fastcpy_unsafe.rs:78, 88, 111, 112` | input → output (only caller: `decompress.rs:367`) | different | same |
| `compress.rs:541–544` `copy_literals_wild` | input → output | different | `&[u8]` vs `&mut [u8]` / fresh `Vec` |
| `sink.rs:312` `PtrSink::extend_from_slice_wild` | input → fresh `Vec` | different | fresh allocation |
| `decompress.rs:173`, `compress.rs:824, 834, 844` | input → `&mut num` (stack local) | different | fresh stack slot |
| `compress.rs:519` `push_u16` | `to_le_bytes()` temporary → output | different | fresh stack slot |

**1 same-base, 16 different-base.** The 4 `core::ptr::copy` calls (all same-base,
self-referential match copies) carry no non-overlap obligation. The 15
`offset_from` obligations are all same-base in reality: output-vs-output. Two of
them fail anyway, for the reason in (b) below.

The different-base sites never rely on the allocator placing things "far
enough" apart. `valid(src, n)` and `valid(dst, n)` already confine each access
to its own allocation, so **disjoint allocations are sufficient**. That is
guaranteed by the language (a live `&[u8]` and `&mut [u8]` never alias; a fresh
`Vec` or stack slot is disjoint from every live allocation).

So Evan's formulation works. Verified in a probe crate against the same driver:

- Same-base with offsets (`copy_nonoverlapping(p, p.add(16), 16)` with `len >= 32`)
  **verifies with no annotations**.
- A precondition stating region disjointness, where region(p) = `[p.base, p.addr + p.size)`:
  ```
  requires valid(s, n) && valid(d, n) &&
           (s.addr + s.size <= d.base || d.addr + d.size <= s.base)
  ```
  **discharges `nonoverlapping`**, and it **survives `.add(k)`** because `.add`
  preserves `base` and `addr + size`. (It also needed `base > 0` to keep
  `addr != 0` after the add. `as_ptr` provides that.)
- Discharging it at an entry point takes one `#[trusted]` helper that returns
  `(i.as_ptr(), o.as_mut_ptr())` with the disjointness postcondition. That
  verifies too.

One caveat on phrasing it as `base1 != base2 -> disjoint`: in flux's model,
`base` is "the start of whatever slice `as_ptr` was called on", **not** the
allocation. `v.as_ptr()` and `v[2..].as_ptr()` get different bases but overlap.
Stated literally on `base`, the axiom is unsound. It also never fires, because
existential bases can never be proven unequal. State the assumption on
**regions**, at the point where Rust's aliasing rules justify it.

**Plan:** thread a disjointness precondition from the public API down to the 16
sites, and assume it once per public entry point (~5 in `decompress.rs`, ~6 in
`compress.rs`), plus the 4 stack-local helpers, which can be assumed locally.
That puts the documented assumption in roughly 15 places, each justified by
"`&T`/`&mut T` don't alias" or "fresh allocation", instead of an assume at
every copy.

### Two further gaps from the same probe (not about overlap, but they block `decompress_internal`)

(a) **`ptr as usize` is opaque.** `PointerExposeProvenance` casts produce an
unrefined `usize`, so the input-side bounds checks at `decompress.rs:269, 292,
355, 382` (`input_ptr_end as usize - input_ptr as usize`) tell flux nothing.
Pointer comparison (`p < end`) *is* tracked. The fix is a small source rewrite
to `input_ptr_end.offset_from(input_ptr)`, or a pointer comparison.

(b) **Two `as_ptr()` calls on the same slice get unrelated bases.**
`decompress.rs:232/233` (`input_ptr` vs `input_ptr_end`) and `227/229`
(`output_base` vs `output_start_pos_ptr`) each call `as_ptr`/`base_mut_ptr`
twice. flux cannot prove the results share a base, which breaks `offset_from`
(`:456`) and every `input_ptr >= input_ptr_end` check. The fix is to derive the
second pointer from the first with `.add`, or to refine `Sink` so that
`base_mut_ptr` returns a pointer indexed by the sink.

## Two configurations

(LOC below = source actually compiled in that configuration, not the
4,881 lines on disk.)

lz4_flex is effectively two crates behind one feature flag.

| config | LOC | live ptr sites | errors | ICEs |
|---|---:|---:|---:|---:|
| upstream default (`safe-encode`,`safe-decode`,`frame`) | 4,076 | **0** | 42 | 3 sites / 2 bugs |
| pointer config (`std`,`checked-decode`) | 2,910 | 65 | **357** | 1 site / 1 bug |

The upstream default `cfg`s out every pointer path (`lib.rs:135` even applies
`forbid(unsafe_code)` to `mod sink` under those features), and `frame/` is 1,476
LOC with zero pointer use. **`Cargo.toml`'s `default` has been flipped to
`["std", "checked-decode"]`** for this experiment; the original is preserved in a
comment directly above it.

`checked-decode` is deliberately kept on — it adds the runtime bounds checks that
make the obligations provable. Dropping it would raise the error count and lower
the chance of discharging them.

## REQUIRED setup step, missing from docs/design.md

`extern crate flux_core;` must appear in the crate root or **flux's `extern_spec`s
on `core::ptr` are silently inert** — no error, no warning, just zero pointer
obligations. This is not hypothetical: it cost this triage several wrong answers.
Before adding it, this crate reported 28 errors and 0 pointer preconditions.
After: 357 errors and 332 pointer preconditions.

`crates/bytes/src/lib.rs:80-81` and `crates/tinyvec/src/lib.rs:101-102` already
have these declarations; `smallvec` gets the same effect via `use flux_core::...`
in `src/flux_specs.rs`. lz4_flex needed:

```rust
#[allow(unused_extern_crates)]   // lz4_flex has #![deny(warnings)]
extern crate flux_core;
#[cfg(feature = "alloc")]
#[allow(unused_extern_crates)]
extern crate flux_alloc;
```

A minimal 40-line reproduction is in the session scratchpad: six deliberate
`core::ptr` violations report 1 error without `extern crate flux_core`, 15 with it.

## Caveats

## ICEs

**Update 2026-10-15:** the `checker.rs` ICE is fixed upstream (flux-rs/flux#1784)
and `compress_internal` is no longer `trusted`. The `type_env.rs` ICE is filed as
flux-rs/flux#1785 and accepted (stays `trusted`). Current status:
`log/NOTES.md`, "Flux upgrade". Original triage below.

Three ICE sites, **two distinct flux bugs**:

| bug | site | live in pointer config? |
|---|---|:-:|
| `checker.rs:2660: impossible case reached` | `block::compress::compress_internal` | **yes** |
| `type_env.rs:564` | `frame/decompress.rs` `read_to_string` / `read_to_end` (+ `trusted_impl` on the enclosing `io::Read` impl) | no — `frame` is `cfg`'d out |

`compress_internal` is `trusted`, and **that annotation is load-bearing**: remove
it and the ICE reproduces immediately, aborting the run (error count collapses
from 357 to 19 because flux bails early). Verified against driver `3450fb82`;
the line number moved from 2627 to 2660 across the rebuild, same bug.

So the 357 figure is measured **with `compress_internal` trusted**. It is the
main compression driver, so this is a genuine coverage gap, not cosmetic.
`flux_thaw` reported no ICEs in the pointer config only because the annotation
was already in place from the safe-config pass.

One bug, one function, worth reporting upstream. That is still a good result
compared to smallvec/bytes, but it is not "no ICEs".
- Compilation aborts on error, so more may surface as these are discharged.
- The 42-error safe-config figure is not comparable to smallvec/bytes in
  `current_errors.md`; the 357 figure is.

## Known blocker for the rest of the pipeline

`run_flux.py --make-targets` / `--check-uid` / `--check-type` need
`FLUXFLAGS=-Fdump-call-graph`, which **does not exist in any ref of the local flux
repo, including `main`**. That disables Phase 1/Phase 2 of `harness/CLAUDE.md`.
`--check-def` works (`-Fdump-checker-trace` does exist).

## Reproduce

```bash
cd crates/lz4_flex
cargo flux                       # 357 errors, 0 ICEs
cargo flux check -p lz4_flex --only-check='def:block::decompress::copy_from_dict'
```
