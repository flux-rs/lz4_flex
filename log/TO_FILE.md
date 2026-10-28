# To file — lz4_flex

**Nothing left to file.** Everything Fluxing lz4_flex found is already reported. Status
re-checked 2026-10-28.

## lz4_flex bug

| bug | reported | status |
|---|---|---|
| `duplicate_overlapping`: loop test `output_ptr.add(1)` forms a pointer two past the end when `match_length` is even (`ABORT.md` uid 65, `tests/duplicate_overlapping_ub.rs`) | private security advisory to lz4_flex, 2026-10-16 | awaiting upstream fix; keep it out of public discussion until the advisory is published. The function is `BUG:`-trusted |

## flux issues (all filed from lz4_flex)

| issue | kind | status 2026-10-28 | lz4_flex impact |
|---|---|---|---|
| [#1784](https://github.com/flux-rs/flux/issues/1784) | ICE, `marker_at_dominator` | **closed** 2026-10-14 (fixed) | `compress_internal` untrusted after the fix |
| [#1785](https://github.com/flux-rs/flux/issues/1785) | ICE, `pack_ty` free vars | open | 2 `ICE:` trusts in `frame/decompress.rs` (not compiled under the default features) |
| [#1786](https://github.com/flux-rs/flux/issues/1786) | ICE, fixpoint `Tuple0` vs `int` | open | blocks specs for `HashTable*::new` (3 `ABORT:`) |
| [#1859](https://github.com/flux-rs/flux/issues/1859) | ICE, `infer.rs:937` on `SliceSink::new` lent `&strg` to a generic | open | `decompress_into{,_with_dict}` (2 `ICE:` trusts); also blocks `compress_internal`'s `Result` spec (uid 132) |
| [#1860](https://github.com/flux-rs/flux/issues/1860) | `MulWithOverflow` in a `static` initializer | open | worked around (`static` → `const`) |
| [#1861](https://github.com/flux-rs/flux/issues/1861) | loop invariant not inferred (bound is an indexed local) | open | worked around |
| [#1862](https://github.com/flux-rs/flux/issues/1862) | a `<<`-initialised local stops loop-invariant inference | open | worked around (`1 << k` → `32`, `>> k` → `/ 32`) |
| [#1863](https://github.com/flux-rs/flux/issues/1863) | ICE, `qualifier!` naming a generic's assoc refinement | open | blocks `compress_internal`'s 7 output-capacity errors (`ABORT:`) |

Shared flux issues found in thin-vec/ringbuf (`size_of >= 0`, `from_raw_parts` specs) may also
apply here, but they didn't come up in lz4_flex's errors. Drafts are in those branches'
`log/TO_FILE.md`.

## Reopen when

- #1859 is fixed: untrust `decompress_into{,_with_dict}`, restore `compress_internal`'s `Result` spec.
- #1863 is fixed (or flux gets loop-invariant annotations): the 7 `compress_internal` capacity errors.
- #1786 is fixed: specs for the 3 `HashTable*::new`.
- lz4_flex fixes `duplicate_overlapping`: drop the `BUG:` trust.
