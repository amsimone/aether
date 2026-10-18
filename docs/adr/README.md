# Architecture Decision Records

This directory contains the Architecture Decision Records (ADRs) for the Aether rocket simulator. Each ADR captures a single significant decision: the context that led to it, the decision made, the consequences, and the alternatives that were considered and rejected.

## When to write an ADR

Write an ADR whenever you make a decision that:

- Affects the architecture or the structure of the codebase (new crate, new module boundary, change in dependency direction).
- Is hard to reverse (licensing, persistence format, public API).
- Has multiple reasonable alternatives that a future contributor might want to revisit.
- Sets a precedent other parts of the codebase are expected to follow.

Trivial decisions (variable names, which crate to use for X when there's an obvious default) do not need ADRs.

## Format

We follow the Michael Nygard ADR template:

- **Title line:** `# ADR-NNNN: Short noun phrase`
- **Status:** Proposed | Accepted | Deprecated | Superseded by ADR-XXXX
- **Date:** ISO 8601 (YYYY-MM-DD)
- **Context:** the problem and the forces that bear on the decision.
- **Decision:** the change we're making. Written in active, imperative voice.
- **Consequences:** what becomes easier, what becomes harder. Both positive and negative.
- **Alternatives considered:** the other options and why they were rejected. Keep these — future readers will want to know what was *not* chosen and why.

## Index

| # | Title | Status | Date |
|---|---|---|---|
| 0001 | Layered Rust workspace architecture | Accepted | 2026-10-18 |
| 0002 | egui for the user interface | Accepted | 2026-10-18 |
| 0003 | Dormand-Prince RK45 as the primary integrator | Accepted | 2026-10-18 |
| 0004 | Per-OS installers, no app store for v1 | Accepted | 2026-10-18 |
| 0005 | MPL-2.0 license for all workspace crates | Accepted | 2026-10-18 |
| 0006 | ThrustCurve API fetch-on-demand, no bundled motors | Accepted | 2026-10-18 |
| 0007 | Tier progressive disclosure with auto-suggestion | Accepted | 2026-10-18 |
| 0008 | Local-only files for v1, format designed for future sync | Accepted | 2026-10-18 |
| 0009 | Rhai scripting for v1.x hooks; no native FFI plugins | Accepted | 2026-10-18 |
| 0010 | Raw wgpu for 3D rendering (rend3 and scena rejected) | Accepted | 2026-10-18 |
| 0011 | [Shared model and execution targets](0011-shared-model-and-execution-targets.md) | Accepted for direction; backend selection gated | 2026-10-18 |

## Superseded ADRs

When a decision is superseded, do not delete the ADR. Mark its status as `Superseded by ADR-XXXX`, add a paragraph at the bottom linking to the new ADR, and create the replacement ADR explaining what changed.
