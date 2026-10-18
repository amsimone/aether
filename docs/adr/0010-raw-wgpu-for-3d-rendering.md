# ADR-0010: Raw wgpu for 3D rendering (rend3 and scena rejected)

**Status:** Accepted
**Date:** 2026-10-18

## Context

Phase-0 Task 2 (the egui + wgpu integration spike) established that a 3D
viewport can share egui's wgpu instance via `egui_wgpu::CallbackTrait`.
The spike used raw wgpu to render a cube. Phase 1 needs to scale this up
to procedural rocket geometry (nose cones via lathe, fin sets via
instancing, body tubes as cylinders, transitions as conical frustra),
which is substantial enough that a higher-level 3D library would help.

Three candidates were considered:

1. **three-d 0.19.0** (July 2026) — the original plan's choice (plan §5,
   §12; ADR-0002). **Rejected** in Phase 0: three-d is OpenGL-based
   (`glow ^0.17`), not wgpu. The wgpu backend requested in three-d issue
   #221 (April 2022) was never implemented. three-d cannot share egui's
   wgpu device — they are fundamentally different graphics APIs.

2. **rend3 0.3.0** (Sep 2026) — wgpu-native, designed to accept
   user-provided wgpu devices. **Rejected**: rend3 is pinned to
   **wgpu 0.12**, while current wgpu is **30.0.1** — a ~20-major-version
   gap. rend3 is effectively unmaintained or stuck on an ancient wgpu.
   The `rend3-egui` integration crate would carry the same version skew.
   Incompatible with our egui-wgpu 0.37 / wgpu 30 stack.

3. **scena 1.10.6** (Oct 2026) — wgpu-native, MIT/Apache-2.0, actively
   maintained (632 commits), with picking, cameras, glTF, and an explicit
   `prepare()`/`render()` lifecycle. **Rejected after a dependency
   resolution check**: scena depends on **wgpu ^29.0.3**, while
   egui-wgpu 0.37 depends on **wgpu 30.0.1**. Cargo resolves these as
   **two separate wgpu major versions in the same dependency tree**
   (`wgpu@29.0.4` for scena, `wgpu@30.0.1` for egui). The two wgpu
   versions expose **distinct, incompatible `Instance`/`Device`/`Queue`
   types** — scena's `Renderer` cannot share egui's wgpu device. The
   offscreen-render-to-texture fallback is also blocked: a scena
   `wgpu@29::Texture` cannot be copied to an egui `wgpu@30::Texture`
   within a single process (no cross-version texture sharing).

The remaining option is **raw wgpu** — what the Phase-0 spike already
uses. This means writing the procedural geometry layer (mesh generation,
camera, picking) ourselves rather than getting it from a library.

## Decision

Aether will use **raw wgpu** for 3D rendering, with a thin
procedural-geometry layer built in-house inside `rocket-render`.

Specifically:

- `rocket-render` will depend directly on `wgpu` (via `eframe::egui_wgpu::wgpu`,
  sharing egui's wgpu instance).
- Mesh generation (nose cone lathe, body tube cylinder, fin extrusion,
  transition frustrum) will be implemented as functions producing
  `Vec<Vertex>` + `Vec<u32>` (indices), uploaded to wgpu buffers.
- The orbit camera, picking, and scene graph will be implemented in-house.
- The 3D viewport will use the same `egui_wgpu::CallbackTrait` pattern
  proven in the Phase-0 spike.

This is more code than using a library, but it is the only option that
actually works with egui-wgpu 0.37 / wgpu 30.

## Consequences

**Positive:**

- **Single wgpu version** in the dependency tree (30.0.1), shared with
  egui. No version skew, no duplicate wgpu crates, no cross-version
  type incompatibilities.
- **Full control** over the rendering pipeline. We can optimize for our
  specific use case (procedural rocket geometry, dirty-flag mesh
  rebuilding, CG/CP markers) without fighting a library's abstractions.
- **No external 3D library dependency** to track for breaking changes.
  wgpu 30 is our only graphics dependency beyond egui.
- **The Phase-0 spike code is reusable** — the cube renderer's structure
  (GPU resources in `callback_resources`, `prepare` updates uniforms,
  `paint` issues draw calls) generalizes directly to the multi-mesh
  rocket renderer.
- **Smaller binary size** — no large 3D library pulled in.

**Negative:**

- **More code to write and maintain.** We implement mesh generation,
  camera, and picking ourselves rather than getting them from a library.
  Estimated: ~1500 lines for Phase 1's `rocket-render`, vs ~500 if scena
  had worked.
- **No glTF loading.** If we ever want to import 3D models (e.g., user-
  supplied fin shapes as mesh files), we'd need to add a glTF loader
  (the `gltf` crate) and write our own mesh import path. Out of scope
  for v1.
- **No built-in PBR materials / lighting.** The rocket render will use
  flat or simple Lambertian shading. Adequate for a design tool (we
  want to see geometry clearly, not photorealistically); would need
  upgrading for a "marketing render" feature later.
- **No built-in scene graph.** We manage the list of `MeshPart`s (each
  with its own transform) ourselves. For our use case (a single rocket
  with ~10-30 components) this is trivial; a complex scene would
  benefit from a real scene graph.

**Neutral:**

- The `rocket-render` module structure (from plan §6) is unchanged. It
  still depends on `rocket-core` and is consumed by `rocket-ui`.

## Alternatives considered

### scena (rejected — wgpu version mismatch)

The strongest candidate on paper: actively maintained, wgpu-native,
MIT/Apache-2.0, with picking, cameras, glTF, and an explicit lifecycle.
Rejected because scena 1.10.6 depends on wgpu ^29.0.3 while egui-wgpu
0.37 depends on wgpu 30.0.1. Cargo resolves these as two separate wgpu
major versions, exposing incompatible `Device`/`Queue` types. scena's
renderer cannot share egui's wgpu device, and the offscreen-texture
fallback is blocked by the lack of cross-version texture sharing.

This could change if scena updates to wgpu 30 (a one-line Cargo.toml
change on their end, though it may involve API migration). We will
revisit this ADR if/when scena supports wgpu 30+.

### rend3 (rejected — abandoned on wgpu 0.12)

rend3 0.3.0 is pinned to wgpu 0.12, ~20 major versions behind current
(wgpu 30). The crate appears effectively unmaintained. The `rend3-egui`
integration would carry the same version skew. Incompatible with our
stack.

### three-d (rejected — OpenGL, not wgpu)

three-d 0.19.0 uses `glow` (OpenGL), not wgpu. The wgpu backend
requested in issue #221 was never implemented. Cannot share egui's wgpu
device. This was the original plan's choice (ADR-0002, plan §5, §12) and
was caught by the Phase-0 spike.

### Offscreen-texture fallback with scena

Considered: render scena's scene to a `wgpu@29::Texture`, read it back
to CPU, upload as an egui `TextureHandle`. Rejected because:

- CPU readback per frame is slow (~5-10 ms for a 1080p texture, blowing
  the per-frame budget).
- Cross-version texture sharing (`wgpu@29::Texture` → `wgpu@30::Texture`)
  is not possible within a single process — they are distinct types.
- Even if it worked, the performance cost would defeat the purpose of
  using a high-level library.

### Wait for scena to update to wgpu 30

Considered: file an issue / PR upstream and wait. Rejected because:

- We don't control scena's release schedule.
- Phase 1 is blocked on this decision; waiting indefinitely is not
  acceptable.
- Raw wgpu is a viable path now, and the Phase-0 spike already proved
  the integration pattern.

If scena updates to wgpu 30+ later, we can revisit this ADR and migrate
`rocket-render` to scena if the in-house geometry layer becomes a
maintenance burden.

### Bevy / a full game engine

Considered and rejected: Bevy is a full ECS game engine, far too heavy
for a rocket design tool's 3D viewport. Pulls in a large dependency
tree and imposes an architecture (ECS) that doesn't fit our immediate-
mode UI.

## References

- Phase-0 SPIKE_REPORT.md (Task 2 findings on three-d)
- Phase-1 plan Task 1.1 (3D library evaluation)
- scena 1.10.6 on crates.io: https://crates.io/crates/scena
- rend3 0.3.0 on crates.io: https://crates.io/crates/rend3
- three-d issue #221 (wgpu backend request): https://github.com/asny/three-d/issues/221
- egui `custom3d_wgpu.rs` example (the pattern we follow): https://github.com/emilk/egui/blob/master/crates/egui_demo_app/src/apps/custom3d_wgpu.rs
