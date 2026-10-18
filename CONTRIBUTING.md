# Contributing to Aether

Thank you for your interest in contributing to Aether! This document
covers the development setup and conventions.

## Development Setup

```sh
git clone https://github.com/amsimone/aether.git
cd aether
cargo build --workspace
cargo test --workspace
```

### Prerequisites

- Rust 1.95+ (stable)
- For the GUI: a system with GPU support (Metal, Vulkan, or DX12)
- For motor downloads: internet access (ThrustCurve.org API)

## Architecture

Aether is a layered Rust workspace (ADR-0001). The key rule:

> `rocket-core`, `rocket-physics`, and `rocket-aero` must NOT depend on
> any UI or networking crate.

The dependency DAG is enforced by `scripts/check_dag.sh`, which runs in
CI. See `docs/adr/` for the full decision records.

### Crate layout

```
crates/
├── rocket-core/          # Data model, units, materials, components
├── rocket-physics/       # Atmosphere, dynamics, gravity (Phase 2+)
├── rocket-aero/          # Aerodynamic coefficients (design estimate now)
├── rocket-sim/           # Flight loop, integrators (Phase 2+)
├── rocket-motors/        # ENG parser, ThrustCurve API, cache
├── rocket-monte-carlo/   # UQ (Phase 5)
├── rocket-optimizer/     # CMA-ES (Phase 5)
├── rocket-render/        # Procedural geometry, wgpu renderer
├── rocket-io/            # .rocket save/load, exports
├── rocket-ui/            # egui app (design editor)
├── rocket-cli/           # `aether` command-line tool
└── aether-tests/         # Integration & validation tests
```

## Coding Conventions

- `#![deny(unsafe_code)]` at the workspace root. No `unsafe` without a
  reviewed ADR.
- `clippy::pedantic` as warn, `clippy::unwrap_used` as deny (in
  non-test code). Test modules can `#![allow(clippy::unwrap_used)]`.
- All dimensional quantities use newtypes (`Length`, `Mass`, `Force`,
  etc.) — no bare `f64` in the data model.
- SI units internally; the UI converts for display.
- Every public function has a doc comment.
- Tests are required for physics and aero code.

## Testing

```sh
cargo test --workspace           # Run all tests
cargo nextest run --workspace    # With nextest (faster)
cargo test -p rocket-aero        # Just the aero module
```

Validation tests in `aether-tests/` are `#[ignore]` until the physics
code they test is implemented. They form the to-do list for Phases 2–4.

## Filing Changes

1. Create a feature branch: `git checkout -b my-feature`
2. Make your changes, with tests.
3. Ensure `cargo fmt --check`, `cargo clippy -- -D warnings`, and
   `cargo test --workspace` all pass.
4. Open a PR with a clear description.

## ADRs

Significant decisions are recorded as Architecture Decision Records in
`docs/adr/`. See `docs/adr/README.md` for the format and when to write
one.
