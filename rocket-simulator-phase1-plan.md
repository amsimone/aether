# Phase 1 — Design Editor — Plan & Task List

**Status:** Ready for execution
**Estimated duration:** 4 weeks (one focused engineer)
**Goal:** A user can build a complete model rocket visually and see live CG/CP readouts.

---

## Phase 1 goal & gate

Per the plan (§15), Phase 1's gate is:

> *A user can build a complete model rocket visually and sees live CG/CP.*

Concretely, by the end of Phase 1, a user can:

1. Open the Aether GUI (`aether-ui`).
2. Add components to a rocket design via the component tree (nose cone, body tube, fin set, transition, launch lug, mass object).
3. Edit each component's parameters in a properties panel (lengths, diameters, fin geometry, materials) with live validation.
4. See the rocket rendered in 3D in real time as they edit.
5. See live readouts of total mass, CG location, CP location (approximate, design-time), and static margin, updating as they edit.
6. Save and load `.rocket` files.

This phase delivers the design half of the "design → simulate → analyze" loop. It does **not** include flight simulation (Phase 2), full-fidelity aerodynamics (Phase 3), or Monte Carlo (Phase 5).

## Prerequisites (from Phase 0)

All met:

- ✅ Workspace scaffold, CI, ADRs (Task 1)
- ✅ egui + wgpu integration proven (Task 2)
- ✅ RASP parser + motor data types (Task 3)
- ✅ ThrustCurve API + cache (Task 4)
- ✅ Validation fixtures staged (Task 5)

## Open issues from SPIKE_REPORT.md to resolve in Phase 1

1. **3D library selection** (ADR-0010) — addressed in Task 1.1 below.
2. **Component data model** — Task 1.2.
3. **Procedural geometry pipeline** — Task 1.3.
4. **Design-time aero approximation** — Task 1.5.
5. **eframe 0.37 / egui_dock compatibility** — verified in Task 1.4.

---

## Tasks

### Task 1.1 — 3D library evaluation and ADR-0010 (2 days)

**Goal:** Decide between rend3, scena, and raw wgpu for the 3D viewport.

**Research findings (pre-filled):**

- **rend3 0.3.0** (Sep 2026): pinned to **wgpu 0.12**, while current wgpu is **30.0.1**. This is a ~20-major-version gap. rend3 appears effectively unmaintained or stuck on an ancient wgpu. The `rend3-egui` integration crate would carry the same version skew. **Rejected** — incompatible with our wgpu 30 / egui-wgpu 0.37 stack.
- **scena**: actively maintained (632 commits, recent release evidence), wgpu-native, MIT/Apache-2.0, with picking, cameras, glTF loading, headless rendering, and an explicit `prepare()`/`render()` lifecycle. Promising.
- **Raw wgpu** (what the spike uses): most control, most code. Sufficient for a cube; insufficient for procedural rocket geometry without significant investment.

**Deliverables:**

- [ ] A 1-day spike: build a minimal scena scene (a box) embedded in an egui panel, mirroring the Task 2 spike. If scena can share egui's wgpu device (or render to a texture egui can display), it's the winner.
- [ ] If scena works: file **ADR-0010 ("scena for 3D rendering")**, deprecating the three-d references in ADR-0002 and plan §12.
- [ ] If scena fails the spike: fall back to raw wgpu + a thin procedural-geometry layer (nose cone lathe, cylinder, fin instances). File ADR-0010 documenting the choice and the rationale.
- [ ] Update plan §5 (technology stack) and §12 (3D rendering) to reflect the chosen library.

**Acceptance criteria:**

1. ADR-0010 is filed with a clear decision and rationale.
2. A minimal 3D scene (box or cube) renders inside an egui panel using the chosen library.
3. The chosen library's wgpu version is compatible with egui-wgpu 0.37 (wgpu 30.x).

### Task 1.2 — Component data model in `rocket-core` (3 days)

**Goal:** Implement the full `Component` enum and supporting types from plan §8.

**Deliverables:**

- [ ] `Component` enum: `NoseCone`, `BodyTube`, `Transition`, `Coupler`, `FinSet`, `LaunchLug`, `Pod`, `MassObject`.
- [ ] Each variant with parametric geometry fields:
  - `NoseCone { shape: NoseShape, length, base_diameter, position, material }`
  - `BodyTube { length, outer_diameter, inner_diameter, position, material }`
  - `Transition { shape, length, fore_diameter, aft_diameter, position, material }`
  - `FinSet { count, planform, airfoil, thickness, mount, position, material }`
  - etc.
- [ ] `NoseShape` enum: `Conical`, `TangentOgive`, `Elliptical`, `Parabolic(f64)`, `VonKarman`, `LvHaack`, `SecantOgive { radius }`, `Custom(Vec<(f64, f64)>)` — each with a `profile(t: f64) -> f64` function giving the radius at normalized position `t ∈ [0, 1]` along the nose.
- [ ] `FinPlanform` enum: `Trapezoidal { root_chord, tip_chord, span, sweep }`, `Elliptical { root_chord, span }`, `Custom(Polygon)`.
- [ ] `Material` struct: `{ name, density: MassPerVolume, source }` with a built-in material library (balsa, plywood, cardboard, phenolic, fiberglass, carbon fiber, aluminum, brass, lead for ballast).
- [ ] `MaterialRef` (ID-based reference into a `MaterialLibrary`).
- [ ] Stable component IDs (`ComponentId`, a UUID or counter) per ADR-0008.
- [ ] `Position` type: axial offset + optional radial angle + reference (relative to nose tip, or relative to parent component).
- [ ] `Stage` struct: ordered `Vec<Component>`, motor mount, recovery devices, events, optional upper stage.
- [ ] `Rocket` updated from the Phase-0 stub to the full struct: `name, version, root: Stage, materials: MaterialLibrary, overrides, default_motor`.
- [ ] `serde` round-trip tests for every component type.
- [ ] Mass properties computation: `Rocket::mass_properties() -> (Mass, Length /* cg_x */, f64 /* I_axial */, f64 /* I_transverse */)` — geometric sum of each component's mass (density × volume) and CG, with parallel-axis theorem for inertia.
- [ ] Unit tests: mass of a cylinder matches `π·r²·h·ρ`; CG of a two-component stack matches the weighted average.

**Acceptance criteria:**

1. All component types serialize and deserialize correctly.
2. `mass_properties()` matches analytical calculations for simple geometries (cylinders, cones).
3. The built-in material library covers the common model/HPR materials.
4. Component IDs are stable across serialization.

### Task 1.3 — Procedural geometry pipeline in `rocket-render` (4 days)

**Goal:** Build wgpu meshes from the component tree for real-time 3D display.

**Deliverables:**

- [ ] A `RocketMesh` struct that holds the GPU resources (vertex buffer, index buffer, etc.) for the rendered rocket.
- [ ] A `RocketMeshBuilder` that walks the component tree and produces a list of `MeshPart`s, each with a transform (position + orientation) and a primitive mesh.
- [ ] Primitive mesh generators (each producing vertices, indices, normals):
  - `nose_cone_mesh(shape, length, base_diameter, segments)` — lathe of the profile function around the body axis.
  - `body_tube_mesh(length, outer_diameter, inner_diameter, segments, capped)` — hollow cylinder (or solid if `inner_diameter == 0`).
  - `transition_mesh(length, fore_diameter, aft_diameter, segments)` — conical frustrum.
  - `fin_mesh(planform, thickness, airfoil)` — extruded planform with optional airfoil cross-section.
  - `launch_lug_mesh(inner_diameter, length, segments)` — small tube.
- [ ] A "dirty flag" system: `RocketMesh` is rebuilt only when the component tree changes, not every frame.
- [ ] Integration with the chosen 3D library (scena or raw wgpu) from Task 1.1.
- [ ] Orbit camera: rotate (drag), zoom (scroll), pan (shift-drag). Reset view button.
- [ ] Grid floor and axis gizmo for spatial reference.
- [ ] CG marker (●) and CP marker (◆) rendered in the 3D scene, updated live.
- [ ] Cutaway/section view toggle (clip the front half of body tubes to show internal components).

**Acceptance criteria:**

1. A rocket with a nose cone, body tube, and fin set renders correctly.
2. Editing a component's length in the properties panel updates the 3D view within one frame.
3. The CG and CP markers are visible and positioned correctly.
4. The orbit camera works smoothly (drag, zoom, pan).
5. Mesh rebuild happens only on design changes, not every frame (verified via a debug counter).

### Task 1.4 — Design editor UI in `rocket-ui` (5 days)

**Goal:** The docking workspace, component tree, properties panel, and live readouts.

**Deliverables:**

- [ ] **Docking layout** via `egui_dock` 0.21 (verify compatibility with eframe 0.37 in the first hour; if incompatible, fall back to manual `Panel`-based layout).
- [ ] **Component tree** (left panel):
  - Hierarchical view of the rocket's stages and components.
  - Add component (context menu or toolbar): pick the type, auto-place at the end of the body.
  - Delete, duplicate, reorder (drag-and-drop).
  - Click to select → syncs the properties panel and highlights the 3D mesh.
- [ ] **3D viewport** (center panel): the `RocketMesh` from Task 1.3, with orbit camera and picking.
- [ ] **Properties panel** (right panel):
  - Dynamic form based on the selected component's type.
  - Typed inputs with unit toggles (mm/in for length, g/oz for mass).
  - Sliders with numeric fields for common parameters (length, diameter, fin count).
  - Live validation: red highlight when a constraint is violated (e.g., fin tip chord > root chord, negative length).
  - Material selector (dropdown from the material library).
- [ ] **Status bar** (bottom): total mass, CG location, CP location, static margin (calibers), with a green/amber/red color cue.
- [ ] **Bespoke widgets** (these are what separate "designed" from "generic"):
  - **Stability margin gauge**: a horizontal bar showing CG (●) and CP (◆) positions along the rocket length, with the SM in calibers and a green (≥1.0) / amber (0-1) / red (<0) zone.
  - **CG/CP side ruler**: a vertical ruler beside the 3D viewport showing CG and CP positions.
- [ ] **Menu bar**: File (New, Open, Save, Save As, Export STL), Edit (Undo, Redo — stub for now), Help (About).
- [ ] **Dark theme** matching plan §11.5 (charcoal `#1a1d23`, panels `#232830`, accent blue `#4aa8ff`, etc.).
- [ ] **Undo/redo** (stub): the `Edit` enum from ADR-0008 is defined but the undo stack is a Phase 1.x feature. For now, edits apply directly.
- [ ] **Save/load**: `.rocket` files via `rocket-io`.

**Acceptance criteria:**

1. A user can build the Estes Alpha (nose + body + 3 fins + launch lug) from scratch in under 5 minutes.
2. Dragging a fin's span slider updates the 3D view, mass, CG, CP, and SM within one frame.
3. The stability margin gauge correctly shows CG ahead of CP (green) for a stable design, and flips to red when the user makes the rocket unstable (e.g., moving fins forward).
4. A `.rocket` file saves and loads correctly, preserving the full design.
5. The layout persists across sessions (egui_dock's serialized state).

### Task 1.5 — Design-time aero approximation (2 days)

**Goal:** A fast, approximate Barrowman calculation for live CG/CP readouts.

The full aero module (Phase 3) is too slow for per-frame UI updates. Phase 1 needs a "design-time" approximation that:

- Computes `C_Nα` and `x_cp` at M=0.1 (subsonic, near-zero compressibility).
- Includes only the components that contribute to static stability: nose cone, fin sets, transitions. (Body tubes contribute zero `C_Nα` in linear Barrowman.)
- Runs in <1 ms so it can update every frame.
- Is clearly labeled as "design estimate" in the UI (the full sim uses the Phase 3 module).

**Deliverables:**

- [ ] `rocket-aero::design::estimate_cn_alpha(rocket: &Rocket) -> (f64, f64)` returning `(C_Nα_total, x_cp)` — the simple Barrowman buildup from aero-dynamics doc §2.
- [ ] Nose cone contributions: `C_Nα = 2` (slender limit), `x_cp = shape-specific` (⅔·L for conical, etc.).
- [ ] Fin set contributions: the Barrowman fin formula with fin-body interference.
- [ ] Transition contributions: the conical-frustum formula with sign-correct boattail handling.
- [ ] No compressibility, no drag, no dynamic derivatives — those are Phase 3.
- [ ] Unit tests against the SP-7224 conical-nose case (`C_Nα = 2`, `x_cp = ⅔·L` — these values are in the fixture and can be asserted immediately).
- [ ] Integration with the UI status bar: the SM gauge uses this estimate.

**Acceptance criteria:**

1. The conical-nose unit test passes (`C_Nα = 2 ± 0.001`, `x_cp = ⅔·L ± 0.1 mm`).
2. The estimate runs in <1 ms for a typical 10-component rocket.
3. The SM gauge in the UI updates live as the user edits.
4. The estimate is labeled "design estimate (M=0.1)" in the UI.

### Task 1.6 — Polish, testing, and Phase-1 gate review (2 days)

**Goal:** Tighten the experience and verify the gate.

**Deliverables:**

- [ ] **Keyboard shortcuts**: Delete (delete selected component), Ctrl+S (save), Ctrl+N (new), Ctrl+O (open).
- [ ] **Context menus**: right-click in the component tree for add/delete/duplicate.
- [ ] **Tooltips** on all properties panel fields explaining the parameter.
- [ ] **Error states**: if the user creates an invalid rocket (e.g., fin set without a body tube to mount on), show a clear error and a fix suggestion.
- [ ] **Empty state**: when no rocket is loaded, show a friendly "Create a new rocket" prompt with a button.
- [ ] **Integration tests** (`aether-tests`): build the Estes Alpha from the fixture via the data model (not the UI), verify mass/CG/CP match expectations.
- [ ] **Documentation**: a `docs/getting-started.md` for users, a `CONTRIBUTING.md` for developers.
- [ ] **Phase-1 gate review**: walk through the gate criteria with the user; file issues for any gaps.

**Acceptance criteria (Phase-1 gate):**

1. A user can build a complete model rocket (nose + body + fins + launch lug) visually.
2. The 3D view updates live as they edit.
3. Mass, CG, CP, and SM readouts are live and correct.
4. Save/load works.
5. The Estes Alpha fixture builds correctly via the data model and matches expected mass/CG/CP within tolerances.

---

## Risks & mitigations

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| scena can't share egui's wgpu device. | High | Medium | Fall back to raw wgpu + thin geometry layer (Task 1.1 has this as the documented fallback). |
| egui_dock 0.21 incompatible with eframe 0.37. | Medium | Low | Fall back to manual `Panel`-based layout (less elegant but functional). Verify in first hour of Task 1.4. |
| Design-time aero is too slow for per-frame updates. | Medium | Low | The simple Barrowman buildup is ~10 floating-point operations per component; <1 ms for 50 components is easy. If it's slow, cache and recompute only on edit. |
| Procedural geometry for nose cones is fiddly (lathe of arbitrary profile). | Medium | Medium | Start with conical and tangent-ogive (closed-form profiles); add the others incrementally. |
| The user finds the UI confusing. | High | Medium | The Task 1.6 gate review catches this. Iterate based on feedback. |

## What's explicitly out of scope for Phase 1

- Flight simulation (Phase 2).
- Full-fidelity aerodynamics: drag, compressibility, dynamic derivatives (Phase 3).
- Motor selection from the UI (the design editor works on the airframe; motor selection is a Phase 2 concern, though the data model supports a default motor).
- Monte Carlo, optimization (Phase 5).
- Recovery system design (Phase 2 events, but the data model supports recovery devices).
- STL/STEP export (Phase 6).
- Undo/redo (Phase 1.x stub).

---

## Phase 1 exit

Phase 1 is complete when:

1. ✅ All six tasks pass their acceptance criteria.
2. ✅ The Phase-1 gate is met (user can build a rocket, see live CG/CP, save/load).
3. ✅ The Estes Alpha fixture builds correctly via the data model.
4. ✅ ADR-0010 (3D library) is filed.
5. ✅ CI is green.

**Estimated total: 18 working days (~4 weeks).**

---

*Phase 1 plan version 1.0 — ready for execution after Phase 0 (SPIKE_REPORT.md).*
