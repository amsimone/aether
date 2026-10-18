# ADR-0002: egui for the user interface

**Status:** Accepted
**Date:** 2026-10-18

## Context

Aether is a tool, not a consumer app. Its primary interaction loop is **design → simulate → analyze**, repeated dozens of times per session, with:

- A 3D viewport showing the rocket design or flight playback, redrawing every frame.
- A properties panel with dozens of typed inputs (lengths, masses, angles) whose edits must propagate to the 3D view in real time.
- Plots (altitude, velocity, Mach, stability margin, thrust) that scrub in sync with the 3D playback — hovering a plot highlights the rocket at that time.
- A docking workspace where the user arranges panels to their workflow and expects the layout to persist.
- A dark, professional theme with bespoke widgets (stability-margin gauge, CG/CP ruler, thrust-curve mini-chart).

The UI framework choice constrains all of this. The dominant forces:

1. **The 3D viewport and the UI must coexist on the same wgpu surface**, sharing a render loop, without fighting over the device or the swapchain.
2. **Per-frame redraw must be cheap**, because the playback mode redraws both the 3D scene and the linked plots every frame, with no "dirty region" optimization available.
3. **Iteration speed matters for the developer** — the UI will change often during development, and a retained-mode framework's state-management ceremony (model → message → update → view) slows every iteration.
4. **The framework must support a modern, custom look.** A default-styled framework that fights theming will leave the app looking "amateurish," which is the explicit anti-goal of the project.
5. **Docking and a properties-heavy layout must be first-class**, not bolted on.

The two serious Rust-native candidates in 2026 are **egui** (immediate mode) and **iced** (Elm-style retained mode). Tauri was rejected early — it targets web UIs, which we ruled out in ADR-0001.

## Decision

We will build the Aether UI with **egui** (specifically `eframe` 0.34+, `egui_dock`, `egui_plot`, and the `egui-wgpu` backend), with a custom dark theme and bespoke widgets.

The Phase-0 spike will confirm that `egui-wgpu` and `three-d` (our 3D renderer) coexist on a single wgpu instance. If they conflict, the fallback is to render the 3D scene to an offscreen texture and display it as an egui image — a known-good pattern that costs some performance but unblocks the integration.

## Consequences

**Positive:**

- Immediate mode redraws everything every frame by default, which is exactly what a real-time flight playback + scrubbing plots + 3D scene wants. No dirty-rect bookkeeping.
- State lives in the application, not the framework. A `Component` edit propagates to the 3D view by simply re-rendering — no message bus, no diffing, no model sync bugs. This is the dominant productivity win for a tool with live-updating previews.
- `egui_dock` provides a docking workspace (persistable layouts) out of the box, matching our §11.1 layout requirement.
- `egui_plot` covers our plotting needs (line plots, scrubbing, linked hover) without pulling in a separate charting library.
- egui 0.34 (2026) switched `eframe`'s default backend to `egui-wgpu`, making wgpu first-class rather than a bolt-on. This was the single biggest risk in v0.1 of the plan; the 0.34 release resolved it.
- A 2026 survey of Rust GUI libraries credits egui as the first wgpu-rendered library with good accessibility support — relevant because we want the tool usable by everyone, including visually-impaired rocketry enthusiasts.
- The community is large (~600k all-time crates.io downloads vs iced's ~180k), with active maintenance and many examples to learn from.

**Negative:**

- egui's default look is functional, not beautiful. We must invest in a custom theme (deep charcoal `#1a1d23`, accent electric-blue `#4aa8ff`, etc., per plan §11.5) and bespoke widgets to reach the "designed, not generic" bar. This is real work but is where polish actually comes from anyway — the default look of any framework is inadequate for a flagship tool.
- Immediate mode can be slower than retained mode for very dense UIs (every widget re-runs every frame). For Aether this is fine — our widget count is in the hundreds, not the tens of thousands — but contributors must understand that pathological UIs (e.g., a tree view of 10,000 components) need virtualization.
- egui's API is more procedural than iced's. Code that builds a panel is a sequence of `ui.add(...)` calls rather than a declarative tree. This reads less cleanly but is faster to write and modify.
- Accessibility, while improved in 0.34, is still less mature than a platform-native toolkit. We accept this for v1 and will track egui's accessibility progress.

**Neutral:**

- egui stores widget state (scroll positions, drag state) in an `Id`-keyed map. This is invisible in normal use but means widget `Id`s must be stable across frames — a footgun for dynamically-generated UIs. We will use stable component IDs (already required by the data model, ADR-0008) as the basis for widget IDs.

## Alternatives considered

### iced

Serious candidate. Iced is Elm-inspired (reactive, retained mode), uses wgpu on desktop, has cleaner architecture and theming defaults, and is the foundation of the COSMIC desktop environment. It would be the right choice for a structured end-user app with heavy theming.

Rejected for Aether because:

- Embedding a live wgpu 3D viewport inside iced's retained widget tree, with both redrawing every frame, is possible but requires more ceremony than egui's "redraw everything always" model. The playback + scrubbing + 3D scene interaction is our highest-frequency UI operation; we want it to be trivial, not a managed integration.
- Iced's retained architecture means state is owned by the framework and updated via messages. Every live-updating readout (mass, CG, CP, stability margin) becomes a message round-trip. For a design tool where dragging a slider must update six readouts and the 3D view, this is friction.
- Iced's Elm model is excellent for applications with a clear input → state → view flow. Aether's UI is more procedural ("if a component is selected, show its properties; if a sim is running, show the live plots; if both, show the playback controls") and immediate mode fits this branching naturally.

The decision is reversible in principle (the physics core is UI-agnostic by ADR-0001), but a switch after Phase 1 would be a substantial rewrite. The Phase-0 spike exists to confirm the choice before we build on it.

### Slint

A DSL-driven Rust GUI toolkit with a declarative .slint language and good performance. Rejected because:

- The DSL adds a build step and a learning curve for contributors who already know Rust.
- Slint's strength is embedded devices with custom-styled UIs; Aether is a desktop tool with a dense, docking-based workflow that egui_dock handles more directly.
- Slint's 3D viewport integration is less battle-tested than egui's wgpu backend.

### Tauri (web UI on Rust backend)

Rejected at the architecture stage (ADR-0001). A web UI would force a bridge for filesystem access (motor cache, design files) and sacrifice native threading performance for Monte Carlo. The native Rust path keeps all options open; a future web companion can target WASM from the same physics crates.

### Native toolkits (GTK via gtk-rs, Qt via cxx-qt)

Rejected. Both are mature and have excellent accessibility, but both pull in large C/C++ dependencies, complicate the build on Windows, and don't integrate as cleanly with a wgpu 3D viewport. The Rust-native path (egui or iced) is the right bet for a new project in 2026.

## References

- Plan: `rocket-simulator-plan.md` §5 (technology stack), §11 (UI/UX architecture), §18 Q1.
- "A 2026 Survey of Rust GUI Libraries": https://blog.wybxc.cc/blog/rust-gui-survey-2026
- egui 0.34 release notes (egui-wgpu as default backend).
- Phase-0 spike: `rocket-simulator-plan.md` §19.
