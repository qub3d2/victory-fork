# Progress on this branch

This branch continues the fork described in the [README](../README.md). It contains no game
files, code or assets: everything is built from files the user supplies at runtime, and a
build of it does nothing without the user's own copy of the title. Licensed GPL-3.0-or-later,
as the fork and upstream are.

Test machine: Windows 11, NVIDIA RTX 4070 (waves of 32 lanes), the Series X profile.

## State (2026-11-24)

| Area | State |
|------|-------|
| Boot, menus, opening cutscene | work; the cutscene plays lit |
| Intro movie | a new picture every frame again (it showed one of three pictures in turn, flashing) |
| First gameplay scene | renders lit where the sun reaches; characters, hair, sky, fog, props |
| Light in shadow | still missing: everything the sun does not hit is black |
| Frame time in the world | 250 to 310 ms with `VICTORY_RELAY=2` on a warm shader cache; 360 to 450 ms without |
| Stability | no device loss in normal runs of five to nine minutes |

On a cold shader cache the first run is much slower and draws some characters wrongly while
pipelines are still being made; the second run draws them correctly.

## What this branch adds to the fork

1. **Descriptors read only as data.** A program may load a descriptor that nothing uses as one
   and read its dwords as numbers (a buffer's record count and stride field). Such loads were
   refused. The host now keeps, per view heap, a table of the console descriptor each slot's
   view stands for, bound as a root view, and those loads read from it by heap slot. Over the
   test pack on a 32-lane host: 39 more programs translate and validate, none fewer.
2. **Shared-memory reads that are alike in every lane.** A dword read from the thread group's
   shared memory at an address every lane holds is the same in every lane, so a first-lane read
   of it inside a lane branch needs no exchange between the two host waves of a console wave.
   The ray-tracing structure builder's last three programs translate with it (52 of 52).
3. **GPU tests for that form**: 512 threads in eight console waves, the whole prefix sum (five
   swizzle steps within each 32 lanes, lane 31's sum added by the upper half, the waves'
   totals through shared memory); compared with the sum on the CPU.
4. **Two pull requests of the fork, taken as they are** (their authors' commits, cherry-picked):
   the texture guard sees writes through every view of its pages (fork PR #115), which is what
   fixes the intro movie: a plane of the movie got its new picture only on its turn every
   eighth frame; and numbered copies of title memory are tracked per command list (fork PR
   #205).
5. **Diagnostics for the structure builder** (off unless asked for): its stages run dry (all,
   from the k-th on, or one: everything the host does around a dispatch but the dispatch), a
   log of every piece of title memory that takes shaders' bytes, a note of each new content of
   a big title texture, and the resources each present names.
6. **Opt-in** (`VICTORY_ROOT_BY_ADDRESS=1`): a program with a spilled user-data table makes its
   raw accesses through the buffer descriptors it builds around root views by address, not
   through one view of the copy the address lies in. It has not changed a run's outcome yet.

Tests: 114 in `victory-gpu`, 212 in `victory-shader` and 95 in `victory-hle` pass on the test machine.

## Light in shadow: where it stands

The title lights what the sun does not reach by ray tracing done in its own compute programs
(see [fork-ray-tracing.md](fork-ray-tracing.md)). Its structure builder runs all or none of
its stages.

- Before this branch three of the builder's programs did not translate, so the builder stayed
  off and about 340 of its dispatches a frame were dropped.
- With change 2 every builder program has a host pipeline, and with `VICTORY_BVH_BUILDER=1` the
  builder runs, for the first time.
- **The title then stops within seconds, in its own code**, reading bounding-box bit patterns
  where it expects a pointer (the fork's issue #16 is the same stop, seen on other 32-lane
  cards). The builder therefore stays off by default, and normal runs are unchanged.
- With every stage dry the title runs on: what the host does around the builder is not the
  cause, what its stages write is.
- Bisecting the stages by the order of their first dispatch: the title survives the first 18
  and stops once the 19th, the builder's largest program, also runs. (A later stage may be
  wrong as well.)
- No change from: the copy-back on the submitting thread instead of the relay, fork PR #205,
  change 6. No access was lost to a page without a host copy in any of these runs.

Next:

1. That one stage: what it is given, and whether its stores cover the records the title then
   reads.
2. A shader was seen taking the address of a buffer view that is not a buffer view (it reads
   0): find which trace program and which view.
3. Then the trace and shading programs, which have not run end to end either.

## Building and running

As in the README. A build needs the user's own files:

```
cargo build --release -p victory
victory run <path to your own copy's executable> --execute
```

`victory.toml` (git-ignored) names where your files and the cache live; see
`victory.example.toml`.
