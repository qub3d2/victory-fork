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
3. **A GPU test for that form**: 512 threads in eight console waves, each wave adding the
   totals of the waves before it; compared with the closed form on the card.

Tests: 110 in `victory-gpu` and 211 in `victory-shader` pass on the test machine.

## Light in shadow: where it stands

The title lights what the sun does not reach by ray tracing done in its own compute programs
(see [fork-ray-tracing.md](fork-ray-tracing.md)). Its structure builder runs all or none of
its stages.

- Before this branch three of the builder's programs did not translate, so the builder stayed
  off and about 340 of its dispatches a frame were dropped.
- With change 2 every builder program has a host pipeline, and with `VICTORY_BVH_BUILDER=1` the
  builder runs, for the first time.
- **The title then stops within seconds, in its own code.** With the builder's stores kept out
  of title memory (`VICTORY_BVH_QUARANTINE=1`) it reads results it never got; with them
  written back, values in its registers are bounding-box bit patterns, so some stage stores
  out of place or with wrong bounds. The builder therefore stays off by default, and normal
  runs are unchanged.

Next:

1. Find the builder stage that stores out of place. Known gap to check first: an access to a
   title page that has no host copy is lost (a read gives zero, a store is dropped), and what a
   stage reaches only through pointers it reads out of memory is not made ready beforehand.
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
