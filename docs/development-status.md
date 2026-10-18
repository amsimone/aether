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

This increment initially executed synchronously without retained results. The
analysis increment below supersedes that execution/persistence scope. Only one
editable case is stored per project; retained runs preserve their own cases.
Multiple named editable cases remain future work. Model fidelity limitations
below remain open.

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
| Project schema and file replacement | Migration, complete-case round-trip, repeatable execution, failure-injection tests | Project schema 3; explicit migration from project 2 and legacy design 1. |
| Phase 2 OpenRocket apogee agreement | Existing broad reference/plausibility tests; no complete matched-case dataset | Accuracy gate not demonstrated. |
| Phase 3 RASAero CP/Cd agreement and measured HPR apogee agreement | Reference CSV/transcriptions remain incomplete | Accuracy gate not demonstrated. |
| General 6-DOF fidelity | Existing regression tests; mass/inertia, motor installation, damping, and moment-reference findings remain open | Experimental implementation. |
| DOPRI5 and US-1976 in the production flight loop | Separate implementations exist; flight loop still uses RK4 and ISA | Not integrated. |
| Persisted simulation case | Full input and telemetry round-trip tests; native case reload | One case per project implemented. |
| Retained results, background jobs, simulation CLI | Input/output round-trips, cancellation/outcome tests, shared execution path, native run/save/reopen checks | Implemented in the analysis increment. |
| Monte Carlo and optimization | Crate scaffolding | Planned. |

## Analysis and CFD increment

Implemented on 18 October 2026:

- A visible **Save** action and saved/modified indicator cover vehicle, case, and
  retained runs. Project schema 3 embeds immutable run records; schema 2 migrates
  with no runs and schema 1 imports an unresolved case. Undo/redo affects inputs
  while retaining completed runs. Save operations remain atomic.
- Flight jobs capture inputs before execution, run off the UI thread, and support
  cancellation. Replacing/closing a project is blocked during an active analysis;
  cancel or finish it first. Workers preserve explicit ground impact, duration
  limit, cancelled, model-domain, numerical-failure, or invalid-input outcomes.
- **Results** retains and compares runs, plots 20 channels with zoom/pan/hover and
  event markers, exposes a telemetry table, and exports all channels to CSV.
  Attitude, body rates, local Mach, dynamic pressure, Reynolds number, atmosphere,
  angle of attack/sideslip, acceleration, recovery state, and coefficient source
  accompany the original position/velocity/force/mass outputs. Peak local Mach
  and dynamic pressure use integration states. Events are step-resolved rather
  than root-localized. No-liftoff classification is still a follow-up.
- Portable run JSON contains the full vehicle/case, solver identifier and Git
  source revision (including dirty status), output, events, and termination.
  Import/export and the headless `aether simulate` command share the same core
  path. A source identifier supports tracing; it does not bundle source code.
- **Aerodynamics** previews dataset provenance, reference quantities, vehicle
  binding, and built-in/data comparisons before deliberate activation. Imported
  tables are embedded in the flight case and retained run, not referenced by a
  machine-local filename.
- `rocket-cfd` prepares fresh self-contained jobs and runs the bundled adapter
  against user-selected external Python/Gmsh and OpenFOAM tools. The UI and
  `aether cfd-prepare` / `aether cfd-run` use that same backend. Tool arguments are
  passed as arguments; study JSON cannot supply commands. Driver and generated
  input geometry/binding are checked before execution. Existing job directories
  are never overwritten or reused. Cancellation retains partial artifacts.

### Aerodynamic dataset contract

Schema 1 JSON contains `version`, `metadata`, and a complete rectangular `rows`
grid over Mach × alpha × beta. Metadata requires `name`, `source`, `evidence`,
`vehicle_sha256`, `axes`, `reference_area_m2`, `reference_length_m`,
`moment_reference_station_m`, `reynolds_min`, and `reynolds_max`. Each row holds
`mach`, `alpha_rad`, `beta_rad`, `ca`, `cy`, `cn`, `cl`, `cm`, and `cn_yaw`.

The axes string is `body_x_nose_y_right_z_rh`: +x toward the nose, +y right, +z
completes the right-handed frame. Forces are qA(−CA,CY,CN); moments qAL(Cl,Cm,Cn).
Angles are radians. The moment station is measured aft from the nose in metres;
the simulation translates moments to the current loaded CG using the physical
cross product. Reynolds bounds use the declared reference length. Vehicle SHA256
is computed from sorted serialized vehicle inputs, excluding its display name.
Geometry, material, mount, or ID changes invalidate the binding.

Interpolation is trilinear with no implicit extrapolation. The default policy
stops outside Mach/angle/Re validity; an explicit built-in fallback is available
and its use appears in telemetry. Static data supplies no aerodynamic damping.
One CFD point is useful for inspection/comparison; it does not cover a flight.
A flight-quality table needs an adequate operating envelope and evidence for it.

### CFD setup and supported scope

The tested toolchain is **Gmsh 4.15.2**, Python 3, and **OpenFOAM v2606**
(OpenCFD distribution), using `gmshToFoam`, `checkMesh`, and `rhoSimpleFoam`.
Install Gmsh into an isolated Python environment with `python -m pip install
gmsh==4.15.2`; install OpenFOAM separately. In **Installed CFD tools**, select
that Python executable and OpenFOAM's `etc/bashrc`. `AETHER_CFD_PYTHON` and
`WM_PROJECT_DIR` can supply initial paths. Other OpenFOAM distributions/versions
have not been checked. The mounted macOS distribution needs its actual mounted
`/Volumes/OpenFOAM-v2606/etc/bashrc` path.

The first backend supports one stage, Mach 0.05–0.7, alpha/beta within ±15°,
perfect-gas compressible steady RANS, k–ω SST with wall functions, and first-order
upwind schemes. The exterior is built from nose/body/transition profiles and
finite-thickness fins, with optional lugs and simplified pods. Internal objects
are excluded and documented. Lugs default off because their placement is
approximate and small contacts require finer resolution. The mesher uses HXT
plus relocation/Netgen optimization on a tetrahedral farfield domain.

`checkMesh -allTopology -allGeometry` must report **Mesh OK** before solving.
An activation dataset is emitted only after the solver reports convergence,
40 coefficient samples pass a 1% relative-change criterion (with a 0.01 scale
floor), and axial drag is nonnegative. A successful numerical solve is still
unvalidated: boundary-layer inflation, y+ assessment, mesh/domain independence,
second-order sensitivity, unsteady effects, and independent experimental
comparison remain necessary. No automatic activation follows a CFD job.

Each job retains `input.json`, its vehicle fingerprint and settings,
`vehicle.brep`, `fluid.msh`, OpenFOAM dictionaries/mesh/fields, coefficient history,
stage logs, `result.json` or `failure.json`, and a recursive SHA256 artifact
manifest for completed jobs. `view.foam` supports external postprocessing in
ParaView. Raw unconverged output is retained with `dataset: null`; it is not
presented as a valid flight provider. Geometry/mesh failures also keep logs.

### Verification

Rust tests cover interpolation/reference translation, incomplete/duplicate grids,
explicit domain policy, vehicle binding, local atmospheric telemetry identities,
cancelled versus duration-limited runs, full project/run precision round-trips,
CSV units/columns, schema migration, retained results through input undo, fresh
CFD export, and rejection of modified study geometry before tool startup.
Python tests exercise OpenFOAM environment setup without inherited positional
arguments, exact argument quoting, failed setup, and force/moment axis mapping.
Workspace verification currently passes **267 Rust tests, 19 ignored**, plus
**3 Python driver tests**. Strict Clippy, formatting, documentation, builds, and
dependency-boundary checks pass.

The native macOS workflow creates two reference runs, saves schema 3, and checks
comparison/export and reopen behavior. A clearly synthetic dataset also exercises
preview, normalized comparison, activation, explicit fallback, and restoration
of the built-in provider in the final native build. These remain smoke checks, not a general
cross-platform accessibility or performance qualification.

Actual external-tool smoke studies used the Aether One exterior (lugs excluded),
Mach 0.2, zero incidence, 101325 Pa, 288.15 K, and 4 mm near-surface size.
The revised mesh passed all requested OpenFOAM topology/geometry checks.
A 300-iteration solve and a fresh **1200-iteration / 144,978-cell** solve
both finished without meeting convergence criteria. The latter coefficient
relative-change metric was **0.4528**, above the 0.01 gate.
It retained raw results, fields/logs, and a 43-file artifact manifest; no
activation dataset was emitted. This demonstrates execution and rejection of
unconverged data, not CFD prediction accuracy or successful design verification.
The earlier rejected mesh exposed small-cell quality defects; the adapter also
required a fix so OpenFOAM environment setup receives no solver arguments.
Both failures informed the final checks rather than being treated as success.

## Next increments

1. Resolve installed propulsion mass/CG/inertia and aerodynamic conventions with
   independent verification cases before broader accuracy claims.
2. Add multiple editable named cases, run removal/archive, criteria, and sweeps.
3. Add CFD boundary-layer meshing, mesh/domain studies, and an independently
   validated operating-envelope dataset before using it for design verification.

Numerical follow-ups remain high priority: installed motor location and loaded
mass/CG/inertia, geometry-specific mass properties, aerodynamic moment reference,
rate normalization for damping, explicit model validity, and run termination
status. The new recovery regressions address one identified defect; they do not
establish the broader professional validation gates.
