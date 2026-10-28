# ABORT log — lz4_flex

Every function below that still has errors is now `#[flux_rs::trusted(reason = "ABORT: …")]`
(`duplicate_overlapping`: `"BUG: …"`; `decompress_into{,_with_dict}`: `"ICE: … #1859"`), so
`cargo flux` is clean. Entries marked DONE are not trusted.

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

### Note
`extend_from_slice_wild`'s `valid(dst)` was first unprovable because nothing said
`pos >= 0` (`pos` is an `int` index). Fixed by adding `0 <= pos` to both sink
invariants and the `Sink` trait's promises; no source change was needed.

## function uid 9 — block::compress::write_integer (src/block/compress.rs:226) — DONE (2026-10-15)

**Resolved 2026-10-15** by the "rewrite the loop" fix below: the unsafe-encode
`write_integer` now writes the `n / 255` `0xFF` bytes with one `ptr::write_bytes`
and verifies `pos(v) == pos(s) + bytes_for_int(n)` exactly. The common case
(`n < 255`) does the same work as the loop did. `write_integer_matches_loop`
checks it writes the same bytes as the loop (also passes under Miri); the
`safe-encode` build keeps the loop. Original entry:

Verified: every `push_byte` is in bounds, under
`requires pos(s) + bytes_for_int(n) <= cap(s)` (`bytes_for_int(n) = n / 255 + 1`,
in `src/flux_specs.rs`), and `cap` is preserved.

Not verified: an upper bound on `pos` afterwards
(`pos(v) == pos(s) + bytes_for_int(n)`), which callers need to know how much
room is left.

- Error: `a postcondition cannot be proved` on the `pos(v)` conjunct.
- Root cause: the loop invariant `pos(cur) + n_cur / 255 == pos(s) + n / 255`
  relates the generic sink's assoc refinement `<S as Sink>::pos` at two points
  in time. Flux has to infer it, and its qualifiers can't mention assoc
  refinements of a generic `S`.
- Strategies tried:
  1. Plain spec: every conjunct about `output` failed after the loop, including
     `cap(v) == cap(s)`.
  2. `#[flux_rs::opts(scrape_quals = true)]`: recovers the precondition-shaped
     invariant `pos(cur) + bytes_for_int(n_cur) <= cap(s)` and `cap` preservation,
     but not the `pos` relation.
  3. Reshaping the ensures so a scraped atom matches the invariant, including
     planting the exact atom as a tautological `requires` conjunct: no effect.
  4. A caller-chosen slack (`requires pos + bytes_for_int(n) + k <= cap ensures
     pos' + k <= cap`) needs a refinement parameter bound by no argument; not
     expressible.
- Possible fixes: a loop-invariant annotation in flux; or rewrite the loop so the
  count is computed first. Left as-is because the loop is the hot path's idiom.

## function uid 20 — block::compress::handle_last_literals (src/block/compress.rs:250) — DONE (2026-10-15)

**Resolved 2026-10-15** with uid 9; now also `ensures pos(s) < pos(v)`. Original entry:

Spec: `requires start <= n && pos(s) + 1 + bytes_for_int(n - start) + (n - start) <= cap(s)`.
The token `push_byte` and the `write_integer` call verify. The final
`output.extend_from_slice(&input[start..])` fails its `pos + len <= cap`
precondition, because uid 9 can't say how far `write_integer` advanced `pos`.
Fixing uid 9's postcondition should fix this with no further change.

## function uids 55/56 — fastcpy_unsafe::wild_copy_from_src (src/fastcpy_unsafe.rs:64)

Spec kept (callers are checked against it): `requires 0 < SIZE && SIZE <= num_bytes
&& readable(source, num_bytes) && writable(dst, num_bytes) && disjoint(source, dst)`.

- Errors: `valid` / `nonoverlapping` on the in-loop `copy_nonoverlapping(source, dst, SIZE)`,
  and `count <= p.size` on both `.add(SIZE)`.
- Root cause: the loop runs while `dst < dst_ptr_end`, where
  `dst_ptr_end = dst0 + (num_bytes / SIZE) * SIZE`. It's safe only because
  `dst - dst0` stays a multiple of `SIZE`, so `dst < end` implies
  `dst + SIZE <= end`. With `SIZE` a const generic, that's modular/nonlinear
  arithmetic over a symbolic value, and no qualifier can mention `SIZE`. (The
  only instantiation is `SIZE = 16`, but flux checks the generic body once.)
- Strategies tried:
  1. Precondition: already as strong as the contract allows.
  2. `refined_by`: no struct involved.
  3. Loop qualifiers `LeSum`/`Pos`/`CoversSum`: carry the lockstep bounds but
     not the `% SIZE` fact.
- Possible fixes: rewrite the exit test to `dst.add(SIZE) > dst_ptr_end`
  (equivalent, but changes the hot loop), or monomorphize to `16`.

## function uid 65 — block::decompress::duplicate_overlapping (src/block/decompress.rs:66) — LIKELY REAL BUG

Spec: `requires writable(o, match_length) && readable(s, match_length)`,
`ensures *output_ptr == o + match_length` (same base, size reduced).
Everything verifies (both `ptr::copy`s and the exact postcondition) except the
loop test `output_ptr.add(1) < dst_ptr_end` (src/block/decompress.rs:80):
`add`'s `count <= p.size` fails.

Why it's real, not a flux gap: when `match_length` is even, `*output_ptr` reaches
exactly `dst_ptr_end`, and the test then computes `dst_ptr_end.add(1)`. If the
output has exactly `match_length` bytes left, that's two past the end of the
allocation, which `ptr::add` documents as UB (the result must be in bounds or one
past the end). The slow path in `decompress_internal` only checks
`match_length <= output_end - output_ptr`, so this is reachable.

Reproducer, derived by hand and **not yet run under Miri** (Miri isn't installed
for `nightly-2026-08-21`):
```rust
// token 0x10: 1 literal, match length 4; literal 'a'; offset 1 (LE)
let block = [0x10u8, b'a', 0x01, 0x00];
let mut out = [0u8; 5];
let _ = lz4_flex::block::decompress_into(&block, &mut out);
// Slow path (input too short for the fast loop). duplicate() sees
// offset 1 < 4 + 15, so it calls duplicate_overlapping(match_length = 4) with
// exactly 4 bytes left: after two 2-byte steps *output_ptr == end, then
// end.add(1) = out.as_ptr() + 6. The decoder then returns ExpectedAnotherByte.
```
Confirm with `rustup component add --toolchain nightly-2026-08-21 miri` then
`cargo miri test` on a test containing the above.

Possible fix: test `dst_ptr_end.offset_from(*output_ptr) > 1` (or compare
addresses as `usize`) instead of forming `output_ptr.add(1)`.

Not "fixed" here: requiring `writable(o, match_length + 1)` would only push the
same impossible obligation to the caller.

## function uids 86, 91, 97 — HashTable8K / HashTable4K / HashTable4KU16 `::new` (src/block/hashtable.rs)

- Error: `unwrap` precondition (`Result[true]`) on
  `alloc::vec![0; N].into_boxed_slice().try_into().unwrap()`.
- Root cause: needs `vec![0; N]` to have length `N`, `into_boxed_slice` to keep
  it, and `TryFrom<Box<[T]>> for Box<[T; N]>` to succeed exactly when the length
  is `N`. flux-alloc has none of these, and a local spec for the analogous
  `TryFrom<&[T]> for &[T; N]` crashes flux (ICE 3 in
  `~/Desktop/Code/flux/flux-ice-issues.md`).
- **Accepted flux ICE** (2026-10-15):
  [flux-rs/flux#1786](https://github.com/flux-rs/flux/issues/1786), still open;
  repro still crashes on the rebased driver (`fixpoint_encoding.rs:809`). Leave
  this ABORT as is until #1786 is fixed.
- Strategies tried: (1) no precondition applies (no arguments); (2) no struct
  refinement helps (the array length is already in the type); (3) extern specs:
  blocked by the crash above.
- Not rewritten: `Box::new([0; N])` would lose the zeroed-allocation codegen the
  source comment says is intentional.
- Trivially safe by inspection: the `vec!` has exactly `N` elements.

## function uid 132 — block::compress::compress_into_vec_with_dict (src/block/compress.rs) — ABORT (re-done 2026-10-15)

**2026-10-15 result:** still 1 error (the `unwrap`), now for two reasons other
than ICE 1:
1. `compress_internal` *can* carry the needed fact. This spec verifies:
   ```
   #[flux_rs::spec(fn(input: &[u8][@n], input_pos: usize, output: &strg S[@s], dict: &mut T, ext_dict: &[u8], input_stream_offset: usize)
                   -> Result<usize, CompressError>[<S as Sink>::pos(s) + crate::flux_specs::max_out(n - input_pos) <= <S as Sink>::cap(s)]
                   ensures output: S)]
   ```
   But it isn't kept: with it, `compress_into_with_table`'s
   `compress_internal(.., &mut SliceSink::new(output, 0), ..)` hits flux-rs/flux#1859
   (`infer.rs:937`), turning a clean function into an internal error. The same
   would happen in `compress_into{,_with_dict}` if the fact were passed on through
   `compress_into_sink_with_dict`. Restore once flux-rs/flux#1859 is fixed.
2. Even then, the sink is `PtrSink::from_vec(&mut Vec::with_capacity(max), ..)`,
   and flux-alloc refines `Vec` by `len` only (no capacity), so nothing says the
   sink has `max` bytes of room. Same gap as `PtrSink::from_vec` (type uid 1).

Earlier note:

**Update 2026-10-15:** ICE 1 is fixed upstream
([flux-rs/flux#1784](https://github.com/flux-rs/flux/issues/1784)) and the
`trusted` on `compress_internal` is removed. `compress_internal` now checks (14
errors), so redo `compress_internal` (uids 22, 108, 109, 123, 125, 127, 128,
150, 151) and then this uid. Original entry:


- Error: `unwrap` precondition on `compress_into_sink_with_dict(..).unwrap()` (line ~705).
- Root cause: the `unwrap` is safe because the `Vec` has `get_maximum_output_size`
  capacity, so `compress_internal` never returns `OutputTooSmall`. But
  `compress_internal` is `trusted` (flux ICE 1, `checker.rs:2660`), so its result
  is unrefined and nothing links capacity to success.
- Fix path: fix ICE 1, verify `compress_internal`, and give it
  `ensures ... Result[cap - pos >= get_maximum_output_size(len)]`.

## function uids 79, 81 — block::decompress::decompress_into{_with_dict} — flux internal error

- Error: `internal flux error: crates/flux-infer/src/infer.rs:887:22` (line 937
  on the 2026-10-15 driver; still reproduces, **not yet filed upstream**),
  `incompatible types: SliceSink { output: .., pos: usize[0] } - SliceSink<'_>[?]`
  on `decompress_internal::<_, _>(input, &mut SliceSink::new(output, 0), ..)`.
- Root cause: flux bug. A value from a constructor whose spec returns an indexed
  type (`SliceSink::new -> SliceSink[pos, n]`), lent to a generic `&strg S`
  parameter, fails to unify. Filed as flux-rs/flux#1859.
- Tried: binding the sink to a local first (same error). Not tried: dropping
  `new`'s indexed return spec, which would lose the `pos <= cap` guarantee that
  `SliceSink`'s invariant relies on.

## Disjointness-assumption sites (`copy_nonoverlapping` across different allocations)

Each of these functions verifies except for `nonoverlapping(src_p, dst_p, n)` on
a copy between two different allocations. The pointers have unrelated `base`s,
so flux can't prove it without the documented region-disjointness assumption
(see `FLUX_TRIAGE.md`), which needs a `trusted` helper the harness forbids.
One entry per function; the justification is why the regions can't overlap.

| uid | function | copy | justification |
|---|---|---|---|
| 1 (type) | `<PtrSink as Sink>::extend_from_slice_wild` | caller slice → sink buffer | only caller copies input into a fresh `Vec` |
| 10 | `block::compress::push_u16` | `el.to_le_bytes()` → sink | source is a fresh stack temporary |
| 11 | `block::compress::copy_literals_wild` | input → sink (4 copies, one per `match` arm) | `input: &[u8]` vs the output buffer |
| 13 | `block::compress::read_u16_ptr` | input → `&mut num` | destination is a fresh stack local |
| 14 | `block::compress::read_u32_ptr` | input → `&mut num` | destination is a fresh stack local |
| 15 | `block::compress::read_usize_ptr` | input → `&mut num` | destination is a fresh stack local (spec says 8 bytes: 64-bit target) |
| 67 | `block::decompress::copy_from_dict` | `ext_dict` → output | `ext_dict: &[u8]` vs the output buffer (`&mut [u8]` or a fresh `Vec`) |
| 69 | `block::decompress::read_match_offset` | input → `&mut num` | destination is a fresh stack local |
| 71 (+77, 80, 82: same def, other instantiations) | `block::decompress::decompress_internal` | input → output: hot-path `copy_nonoverlapping(input_ptr, output_ptr, 16)` and `slice_copy`'s `disjoint(input, output)` precondition | `input: &[u8]` vs the sink's buffer (`&mut [u8]` or a fresh `Vec`). This is the one place the decoder needs the assumption; `fastcpy_unsafe` proves non-overlap *from* it. |

## function uids 22, 108, 109, 123, 125, 127, 128, 150, 151 — block::compress::compress_internal (src/block/compress.rs) — ABORT (2026-10-15)

First check after ICE 1 was fixed: 14 errors → **9**. Fixed along the way:
`count_same_bytes` exports `cur + END_OFFSET <= n`; `backtrack_match` keeps
`literal_start <= cur`; `get_maximum_output_size` has an exact spec (`max_out`);
`write_integer`/`handle_last_literals` give `pos` (uids 9, 20); and
`1 << INCREASE_STEPSIZE_BITSHIFT` / `>> ..` became `32` / `/ 32` (a shifted value
in the loop stopped flux inferring `literal_start <= cur`; flux-rs/flux#1862).
All input-side bounds now verify: hashing at `cur` and `cur - 2`, literal copies,
`lit_len`, and the in-input match candidate.

Remaining, by cause:

**(a) Output capacity in the main loop — 7 errors** (`push_byte(token)`, both
`write_integer`s, `copy_literals_wild`'s sink side, `push_u16`, the in-loop
`handle_last_literals`, and `output.pos() - output_start_pos` on that return).
The function checks `cap - pos >= max_out(n - input_pos)` once, up front; each
sequence writes less than `max_out` shrinks by. Proving that needs the outer-loop
invariant `pos(output) + max_out(n - literal_start) <= cap(output)` (and
`output_start_pos <= pos(output)`), which mentions the generic sink's
`<S as Sink>::pos`.
- Hand-written qualifiers can't name a generic's assoc refinement (as uid 9 found).
- The new local `qualifier!` macro can, syntactically, but crashes flux:
  `<S as Sink>::pos(out)` → `desugar.rs:1057` unwrap on `None`; `S::pos(out)` →
  `conv/mod.rs:1530` (flux-rs/flux#1863).
- `#[flux_rs::opts(scrape_quals = true)]`: z3 ran 40+ minutes without finishing.
- The pre-loop part (the `input.len() - input_pos < LZ4_MIN_LENGTH` early
  return) verifies, so the bound itself is right.

**(b) External-dictionary match candidates — 2 errors** (`get_batch(candidate_source,
candidate)` and `count_same_bytes`'s `candidate <= m`, both in the `USE_DICT` branch).
`candidate` comes from `dict.get_at`, which can return anything. In the
in-input branch the code itself bounds it; in the ext-dict branch it only proves
`candidate - ext_dict_stream_offset < ext_dict.len()`, but the read needs 4
(and the match `MINMATCH`) bytes. Safe by inspection in the block API:
`init_dict` only stores `i` with `i + 8 <= dict.len()`, and `compress_internal`
only stores positions at most `input_stream_offset + n - 8`. Proving it needs an
invariant on the hash table's contents.
- Related, not an error here: `candidate >= ext_dict_stream_offset` is only a
  `debug_assert!` (flux uses it; release would wrap on `candidate -=
  ext_dict_stream_offset`). Checked by inspection 2026-10-16, **not reachable**:
  a candidate is only used within `MAX_DISTANCE` (65535) of `cur`. The block API
  starts from a fresh table with `ext_dict_stream_offset == 0`. The frame
  encoder's linked mode keeps `src_start + ext_dict_len >= WINDOW_SIZE` (65536)
  when it shrinks the ext dict (`src/frame/compress.rs:342`), and `cur >=
  src_start`, so `candidate >= input_stream_offset - ext_dict_len`. The 4-byte
  read near the dict end is safe because stored positions are at least `MFLIMIT`
  bytes before their buffer's end. No test drives linked frames into the shrink
  branch.
