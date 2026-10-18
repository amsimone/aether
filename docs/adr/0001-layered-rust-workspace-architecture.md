# ADR-0001: Layered Rust workspace architecture

**Status:** Accepted
**Date:** 2026-10-18

## Context

Aether targets a wide fidelity range — model rockets to amateur/sounding — and a wide user range — hobbyists running one sim to engineers running 10,000-run Monte Carlo batches. The architecture must serve both without making either feel compromised.

The dominant forces shaping the architecture:

1. **The physics core must be reusable.** A flight-controller firmware should be able to link it for hardware-in-the-loop; a CI pipeline should be able to run it headlessly; a future WASM build should embed it in a web tool. If the physics code depends on UI code (even transitively, via shared types), these uses are blocked.
2. **Performance is a hard constraint.** The Monte Carlo target — 10,000 runs of a 60 s flight in under 60 s — implies a per-sim budget under 5 ms. This demands control over memory layout, allocation, and parallelism that interpreted or JIT'd languages make difficult.
3. **Correctness must be verifiable.** Every aero and dynamics module needs unit tests, property tests, mutation testing, and cross-validation against published data. The architecture must make these easy to write and impossible to skip.
4. **Scope will grow.** T0 (model rockets) ships first; T2 (NRLMSISE-00, round-Earth, TVC) ships years later. The architecture must allow these layers to be added without rewriting the core.
5. **The team is small.** We cannot afford to maintain multiple implementations of the same physics in different languages for different front-ends.

The reference point is OpenRocket: a single Java codebase that mixes physics, UI, and persistence, with the physics layer accessible only by embedding the whole application. This is the model we want to avoid.

## Decision

We will build Aether as a **Cargo workspace of layered Rust crates**, with strict dependency direction:

```
rocket-core        — data model, units, materials. No deps on physics or UI.
rocket-physics     — atmosphere, aero, dynamics, propulsion. Depends on rocket-core.
rocket-aero        — aerodynamic coefficient prediction (kept separate for clarity & testing). Depends on rocket-core.
rocket-sim         — flight loop, integrators, event system, telemetry. Depends on rocket-physics, rocket-aero, rocket-core.
rocket-motors      — ENG/RASP parsers, catalog, thrust-curve interpolation. Depends on rocket-core.
rocket-monte-carlo — sampling, parallel execution, statistical aggregation. Depends on rocket-sim.
rocket-optimizer   — CMA-ES, objective builders, constraint handling. Depends on rocket-sim, rocket-monte-carlo.
rocket-render      — wgpu/three-d scene, procedural geometry. Depends on rocket-core.
rocket-ui          — egui app. Depends on all of the above.
rocket-io          — save/load, CSV/KML/STL/PDF export. Depends on rocket-core.
rocket-cli         — headless batch runner. Depends on rocket-sim, rocket-motors, rocket-io.
```

Key rules:

- **`rocket-core` and `rocket-physics` depend on nothing UI-related.** No rendering types, no UI state, no egui.
- **The aero and dynamics modules are pure functions** of `(Rocket, query)`. No time, no mutable state, no UI. This is what makes caching, Monte Carlo, and property testing tractable.
- **One concern per crate.** `rocket-aero` is split from `rocket-physics` not because they're unrelated, but because the aero code is the highest-risk, most-tested code in the project and deserves its own test boundary.
- **The UI is a thin shell.** `rocket-ui` orchestrates the other crates; it contains no physics or aero logic.
- **`#![deny(unsafe_code)]` at the workspace root.** No `unsafe` unless a reviewed ADR justifies it.
- **Units are newtyped** (`Length`, `Mass`, `Velocity`, …) — SI internally, displayed in the user's chosen units. No bare `f64` in the data model.

## Consequences

**Positive:**

- The physics core can be linked into a flight controller, a CI pipeline, or a WASM build without dragging in UI code.
- Each crate has a focused test suite and clear ownership boundary.
- The CLI and GUI cannot drift apart — they share `rocket-sim` as their common substrate.
- Cross-crate dependency cycles are impossible by construction (Cargo enforces the DAG).
- A contributor working on, say, the aero buildup does not need to understand the UI or even the integrator.
- Performance-critical code lives in crates with no UI overhead, and can be benchmarked in isolation (`criterion`).

**Negative:**

- More crates means more `Cargo.toml` files, more version coordination, and slightly slower builds. This is manageable at our scale (10 crates) but would become painful past ~30.
- The strict layering sometimes forces indirection: a UI change that needs new physics state must propagate through `rocket-core` types rather than being added inline.
- Contributors must learn the layering rules. We will document them in `CONTRIBUTING.md` and enforce with a `cargo-deny` style check (or a custom CI script verifying the dependency DAG).

**Neutral:**

- The workspace shape makes future extraction (e.g., publishing `rocket-physics` as a standalone library) trivial if we ever choose to.

## Alternatives considered

### Single monolithic crate

Rejected. A single `src/` tree would be simpler to set up but loses the dependency-direction guarantee: nothing prevents a physics module from accidentally importing a UI type, and over time the layers rot. OpenRocket's codebase is the cautionary example. The workspace cost is paid once; the layering benefit compounds over the project's life.

### Separate repositories per crate (polyrepo)

Rejected. Cross-crate changes (e.g., adding a field to a `rocket-core` type that `rocket-ui` consumes) would require coordinated PRs across repos, with version bumps and changelog discipline. A single workspace allows atomic cross-crate commits and unified CI. We will publish individual crates to crates.io only if external demand emerges, and only then face the polyrepo question.

### C++ or C for the physics core, Rust for the UI

Rejected. Mixing languages introduces an FFI boundary that doubles the testing surface, complicates the build, and makes the "link the physics core into a flight controller" use case harder (Rust's `cdylib` is fine; a C++ ABI is not). Rust's performance is sufficient for our needs (verified against the per-sim budget in the plan), and the type system catches whole classes of bugs (units, null, data races) that C++ would let through.

### In-process physics with a web UI (Tauri or pure web)

Rejected at the architecture stage, though it remains a future option. A web UI would broaden reach but sacrifices: (a) direct access to the filesystem for the motor cache and design files (requires a bridge); (b) native performance for Monte Carlo (Web Workers are slower than native threads and harder to reason about); (c) the ability to link the physics core into embedded or headless contexts without a browser. A native Rust app keeps all options open; a future web build can target WASM from the same `rocket-physics` crate.

## References

- Plan: `rocket-simulator-plan.md` §4 (architecture diagram), §6 (crate breakdown), §14 (conventions).
- The Cargo Book, "Workspaces": https://doc.rust-lang.org/cargo/reference/workspaces.html
- "Why you should use ADRs" (Michael Nygard, original template): https://github.com/joelparkerhenderson/architecture-decision-record
