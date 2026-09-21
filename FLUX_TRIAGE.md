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
