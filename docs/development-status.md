# Professional workbench development status

The direction is a professional rocket vehicle modeling and flight-analysis
workbench. The [architecture review](architecture-and-roadmap-review.md) proposes
the longer roadmap. This document records implemented increments separately from
those proposals.

## Foundation increment

Implemented on 18 October 2026. The subsequent increment below supersedes
the initial body-only editing scope.

| Change | Verification | Scope |
|---|---|---|
| Shared 6-DOF translational force evaluation for integration and telemetry | Stowed/drogue/main force checks, including relative wind; integrated parachute descent within 0.5% of local analytical terminal speed | Recovery uses the evolving run state and `F = 0.5 rho V² Cd A`, without the previous extra factor of two. Atmospheric state is evaluated at each RK4 substage. |
| Tighter flight-level recovery regression | Existing recovery integration test now compares terminal descent against the analytical result within 2% | The reference flight includes small body-aero and atmospheric-gradient effects. This replaces a broad plausibility band. |
| UI-independent engineering document boundary | Revision, undo/redo, saved-baseline, invalid-input, drag-coalescing, and history-barrier tests | Body-tube property edits are validated commands. Other component operations pass through a transitional boundary and clear body history. |
| Atomic design saving and explicit schema dispatch | Successful replacement; injected partial-write failure; replacement failure; unsupported schema; temporary-file cleanup; Unix permission preservation | Save syncs a temporary file beside the destination before replacement. Schema 1 remains compatible. Unknown versions are rejected. |

The document boundary is currently an internal `rocket-ui::editing` module with
no egui dependency. Its engineering model is read-only to the widgets; edits go
through methods on the document. Extract it into a reusable application crate
when the CLI starts using the same document/case operations.

## Document and flight-case increment

Implemented on 18 October 2026 after the foundation increment:

- All eight component variants use shared semantic checks for finite dimensions,
  material references, and mounting references. Duplicate component IDs are
  rejected across stages. The same checks protect project loading and saving.
- Property edits, additions, duplication, deletion, and case changes use one
  undo/redo history. Commands preserve ordering, IDs, and related mount data.
  Deleting a tube with mounted fins is rejected; deleting an otherwise removable
  motor-mount tube also removes its mount assignment, and undo restores both.
- New, Open, reference replacement, File → Quit, and native window close share a
  Save / Discard / Cancel decision. A cancelled picker or failed write preserves
  the document and pending action. Only a successful save resumes that action.
- A schema-2 `.rocket` project envelope contains the schema-1 vehicle and one
  named `SimulationCase`. It stores the actual motor curve/mass/source comments,
  launch angle, pad altitude, wind vector, recovery configuration, model set,
  integration step, telemetry interval, and maximum duration. No motor catalog
  index or machine-local path is needed to rerun the case.
- Schema-1 designs import unchanged with a default case and an explicitly
  unresolved motor. The UI requires a motor selection before running. New editor
  documents explicitly resolve the reference A8 motor. Older releases reject
  project schema 2 rather than silently discarding case data.
- The UI edits the document's case and executes `SimulationCase::run`; its panel
  retains only presentation state and the current result. Inputs are checked
  before execution. Each run starts with stowed recovery. The current model set
  explicitly identifies rigid 6-DOF, built-in aero v1, ISA, constant gravity, and
  fixed-step RK4. Upper-stage dynamics are rejected by this model set.

Use **Edit → Undo/Redo**, Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z, or Ctrl+Y. Text fields
retain their native keyboard editing. Pointer drags coalesce; typed dimensions
commit on Enter or loss of focus. Dirty state covers both vehicle and case and
compares them with the last successful save. History is bounded to 256 commands
using complete input snapshots; it is not a persisted collaboration log.

Try [the portable reference project](../examples/aether-one-project.rocket).
It is a software fixture, not a flight-qualified design or a professional
validation dataset.

Verification: **255 tests passed, 19 ignored**. Tests cover all component variants,
invalid references, mixed-command undo/redo and save baselines, drag coalescing,
save/discard/cancel decisions, schema migration/rejection, malformed cases and
motor curves, preservation of existing files on invalid saves, and identical
telemetry after a complete case round-trip on the same implementation. A
precision regression checks input bits across JSON round-trips; persistence
enables the exact float parser. Workspace build, strict Clippy, documentation
generation, formatting, and dependency-boundary checks pass.
Native macOS checks confirmed duplication and nose-edit undo, cancelled document
replacement, cancelled Save As with the pending action preserved, successful
Save and continue, and reopening the edited vehicle/case/motor/wind values.
Native window close was intercepted; Cancel kept the window usable, and undo
returned the disposable edit to the saved baseline.

Runs still execute synchronously and results are not saved. Only one case is
stored per project. A run ending at its duration limit is not necessarily a
landing; the UI labels its final time and distance as run-end values. Structured
termination status, retained results, multiple-case studies, and background jobs
remain separate increments. Model fidelity limitations below remain open.

Design/project writes retain the foundation's atomic replacement guarantees;
these do not promise directory-metadata durability across power loss. External
file-change detection, crash recovery, and merging are not implemented. The
full dependency audit remains pending because cross-platform dependencies were
absent from the offline cache during the foundation checks.

## Evidence inventory

| Capability or claim | Current evidence | Status |
|---|---|---|
| Recovery force and descent calculation | Focused numerical regressions plus analytical terminal-speed comparisons | Verified for these test conditions; independent flight validation remains separate. |
| Component and case edit integrity | Shared semantic validation, document history tests, save-decision tests, and native interaction checks | Implemented for the editor’s root-stage operations. |
| Project schema and file replacement | Migration, complete-case round-trip, repeatable execution, failure-injection tests | Project schema 2 and legacy design schema 1. |
| Phase 2 OpenRocket apogee agreement | Existing broad reference/plausibility tests; no complete matched-case dataset | Accuracy gate not demonstrated. |
| Phase 3 RASAero CP/Cd agreement and measured HPR apogee agreement | Reference CSV/transcriptions remain incomplete | Accuracy gate not demonstrated. |
| General 6-DOF fidelity | Existing regression tests; mass/inertia, motor installation, damping, and moment-reference findings remain open | Experimental implementation. |
| DOPRI5 and US-1976 in the production flight loop | Separate implementations exist; flight loop still uses RK4 and ISA | Not integrated. |
| Persisted simulation case | Full input and telemetry round-trip tests; native case reload | One case per project implemented. |
| Retained results, background jobs, simulation CLI | Not implemented | Next architectural increments. |
| Monte Carlo and optimization | Crate scaffolding | Planned. |

## Next increments

1. Execute the saved case through a cancellable worker and a headless entry point.
2. Preserve exact inputs, solver version, termination status, diagnostics, and
   results as a run record.
3. Add multiple named cases and compare retained runs in an analysis workspace.

Numerical follow-ups remain high priority: installed motor location and loaded
mass/CG/inertia, geometry-specific mass properties, aerodynamic moment reference,
rate normalization for damping, explicit model validity, and run termination
status. The new recovery regressions address one identified defect; they do not
establish the broader professional validation gates.
