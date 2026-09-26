# ABORT log — lz4_flex

## type uid 1 — sink::PtrSink (src/sink.rs)

Annotated with `refined_by(out: ptr, pos: int, cap: int)` and
`invariant(0 < out.addr && out.base == out.addr && out.size == cap && pos <= cap)`,
plus `pos`/`cap` assoc refinements and method specs matching the `Sink` trait.
All `PtrSink` methods verify against the invariant except the two sites below.

### `PtrSink::from_vec` — invariant cannot be established (4 errors, src/sink.rs:278)
- Error: `type invariant may not hold` for the struct literal
  `Self { output: output.as_mut_ptr(), pos, cap: output.capacity() }`.
- Root cause: flux-alloc refines `Vec` by `len` only and has no spec for
  `Vec::as_mut_ptr` or `Vec::capacity`, so nothing says the pointer addresses
  `capacity()` bytes.
- Strategies tried:
  1. Precondition on `from_vec`: nothing to state. The missing fact is about the
     `Vec` internals, not the arguments.
  2. Strengthen `refined_by` upstream: `Vec` is refined by flux-alloc; it can't be
     re-refined here.
  3. Intermediate callee spec: would have to be a `trusted` wrapper around
     `as_mut_ptr` + `capacity`, which the harness forbids.
- Upstream fix: add `cap` to `Vec`'s refinement in flux-alloc, with
  `as_mut_ptr(&mut Vec[@v]) -> *mut[b, b, v.cap * T::size_of()] T` and
  `capacity(&Vec[@v]) -> usize[v.cap]`.

### `<PtrSink as Sink>::extend_from_slice_wild` — `nonoverlapping` (1 error, src/sink.rs:369)
- Error: `nonoverlapping(src_p, dst_p, count)` for
  `copy_nonoverlapping(data.as_ptr(), self.output.add(self.pos), copy_len)`.
- Root cause: `data` and the sink's buffer are different allocations (the only
  live caller, `compress.rs:248`, copies caller input into a fresh `Vec`), but
  flux's `nonoverlapping` compares addresses and the two pointers have unrelated
  bases. `&[u8]` carries no address in its refinement, so disjointness can't be
  stated as a precondition either. See `FLUX_TRIAGE.md`, "`copy_nonoverlapping`:
  same base vs different base".
- Strategies tried:
  1. Precondition: can't mention `data`'s address.
  2. Refine `PtrSink` further: it already pins its own region; the missing fact
     is about `data`.
  3. Intermediate spec: the documented disjointness assumption needs a `trusted`
     helper, which the harness forbids.
- Needs: the region-disjointness assumption agreed with Ranjit/Evan.

### Source change made while doing this uid
`extend_from_slice_wild` now calls `self.output.add(self.pos)` instead of
`self.pos_mut_ptr()` (same value). Reborrowing the `&strg self` for the method
call lost the `pos`/`cap` index, which made `valid(dst)` unprovable. Marked
`CENTRIFUGE` in the source.
