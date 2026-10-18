# Aether engineering plan

**Project:** Open vehicle modeling, control development, and flight reconciliation

**Stack:** Rust workspace, egui, and raw wgpu; Julia evaluated as optional analysis/build tooling

**Initial scope:** One rigid rocket plant and sampled controller through simulation, generated C, HIL, and replay

**Status:** Active development; model-to-flight direction adopted 18 October 2026

The [model to flight execution plan](rocket-simulator-model-to-flight-plan.md)
defines the current implementation sequence and acceptance gates. The
[development status](docs/development-status.md) distinguishes implemented
capabilities from proposals. Earlier numbered phase plans are historical task
inventories; their completion does not establish the new gates or physical
accuracy claims.

## 1. Vision and goals

Build an open engineering workbench that keeps vehicle, plant, controller,
interfaces, experiments, and measured-data mappings consistent through the
model → simulation → controller code → HIL → post-flight reconciliation loop.
The product promise is that an engineer can reproduce a study and its generated
artifacts, test the same controller on hardware, and explain model changes from
measured evidence without manually translating parameters and signal meanings.

The first audience is small-launcher GNC and university experimental vehicle
teams. CubeSat/ADCS teams are an adjacent audience for the shared control,
interface, HIL, and replay workflow; full spacecraft dynamics and mission
operations require a separate extension. Validate demand with pilot teams rather
than assuming immediate adoption. OpenRocket remains a useful usability and
selected physics reference; hobby feature parity does not set the roadmap.

### Design principles

- **One versioned source definition:** generate target artifacts from declared
  plant/controller semantics, units, frames, timing, and parameter bindings.
- **Distinct physical and software models:** the host plant, embedded controller,
  sensors/actuators, and experiment are separate sections of a shared package.
- **Evidence with every increment:** preserve exact inputs, build identity,
  outcomes, uncertainty, and validation scope. Code-generation parity is not
  physical validation, and wall-clock pacing is not a hard real-time guarantee.
- **Library first:** keep physics/model contracts independent of UI and I/O;
  GUI, CLI, SIL, HIL, ensembles, and replay consume the same contracts.
- **Reproducible within a declared target:** persist seeds and sampled inputs;
  measure numeric differences across compilers/devices instead of promising
  universal bitwise identity.
- **Small coherent increments:** preserve the working editor and run workflow;
  expand model or backend support only with focused verification cases.

## 2. Supported scope and expansion

The first new reference is a single-stage, rigid, subsonic research rocket with
an installed thrust-curve motor, explicit mass/CG/inertia, two-axis TVC actuator
model, sampled attitude/rate controller, and a specified sensor/estimator chain.
Aether One remains the simple passive editing fixture; the controlled reference
needs its own independent answers and operating-envelope specification.

Initial outputs are a nonlinear CPU simulation, automatic local linearization,
generated controller C and compiled SIL, CPU/GPU uncertainty studies, one board
HIL harness, and observation-aware measured-data replay/reconciliation. Each
has a separate support and evidence gate; see M0–M5 in §15.

The earlier T0/T1/T2 altitude/Mach ladder becomes an expansion backlog rather
than the release sequence. Publish explicit supported model combinations and
validity envelopes. Transonic/supersonic aero, staging, rotating/round Earth,
and broader spacecraft environments follow a demonstrated user need and an
independent validation panel. A selectable fidelity label is not evidence that
its numerical or physical models are integrated and credible.

## 3. Scope boundaries

Include a restricted typed model/controller graph, automatic differentiation
for supported smooth dynamics, generated C for the supported controller subset,
a measured GPU ensemble backend, one HIL board/protocol, and one recorded-data
adapter before generalizing. Keep Julia optional until the backend spike resolves
its role. Rust remains the production workspace and reference runtime.

Defer a general Simulink replacement or import engine, an arbitrary acausal
language, all embedded targets/vendor adapters, certified flight-code claims,
full launcher/orbital physics, liquid engine internals, cloud job services,
real-time collaboration, and manufacturing CAD. Complete one reproducible
workflow before expanding those surfaces.

The current external CFD adapter remains an offline producer of aerodynamic
evidence/data. Its meshing and validation backlog does not become a dependency
of controller code generation, HIL timing, or GPU kernels. A converged CFD point
also does not establish a validated flight envelope.

## 4. Architecture and target artifacts

A package contains vehicle/plant, controller, sensor/actuator, experiment, and
observation definitions plus source evidence. Validation resolves units, frames,
IDs, model capabilities and rates into a restricted typed operation graph and
execution schedule. Runtime state is explicit and separate from this immutable
definition. Compilers emit target artifacts with source mappings and a build
receipt. A JSON parameter file alone is not a multi-target model compiler.

| Target | Artifact and execution boundary |
|---|---|
| Engineering simulation | Host CPU plant/controller evaluation and observations; automatic local A/B/C/D or discrete-map analysis |
| GPU Monte Carlo | Supported graph subset and persisted batch inputs; CPU reference, declared precision/event policy, paired verification |
| Controller code | Standalone C99 state/reset/step interfaces from the controller graph; board drivers and RTOS remain separate |
| HIL | Same host plant and signal/schedule contract connected to the actual compiled controller on one board |
| Flight replay | Immutable recorded telemetry mapped through the same observations/controller interfaces; residuals and explicit model revisions |

The [execution plan](rocket-simulator-model-to-flight-plan.md#3-shared-model-and-compilation-boundary)
contains the runtime data-flow diagram and target contracts. Existing Rust
physics must provide derivative and backend support contracts before it can
participate in AD/GPU paths. Unsupported operations receive explicit diagnostics.
Avoid independently rewriting the equations in a shader, notebook, and firmware.

## 5. Technology choices

| Concern | Current decision and next gate |
|---|---|
| Production language | Retain Rust and the existing Cargo workspace. |
| Symbolic/AD and generation | Timebox a Rust graph/AD/emitter versus Julia ModelingToolkit/Symbolics route; assess reuse of existing generators. Select after a shared small-model spike. |
| GPU execution | CPU ensembles first; evaluate the restricted GPU backend during the spike, then require paired parity and measured end-to-end acceleration. Rendering GPU use does not establish compute support. |
| UI and rendering | Keep egui and raw wgpu provisionally; extend reusable engineering views against concrete workflows. |
| Controller ABI | Portable generated C99, explicit state and reset, fixed dimensions and sample periods; separate host/board harness. |
| Persistence | Preserve current JSON project schema 3 and run JSON/CSV. Add versioned model, build, observation, and evidence artifacts through explicit migrations. |
| Interoperability | Own SIL/HIL harness first; then one pilot-selected C ABI/Trick or FMI adapter, with exact versions and behavior tested. |
| Verification | Existing Rust checks plus derivative fixtures, compiled-C vectors/SIL, CPU/GPU comparisons, hardware timing, and held-out measured-data evaluation. |

Julia offers relevant model transformation and GPU building blocks; using them
still requires Aether's timing, sensor, state, artifact, and target verification
contracts. The production selection follows the bounded comparison in the
[execution plan](rocket-simulator-model-to-flight-plan.md#4-rust-and-julia-decision).

## 6. Workspace and application boundaries

The current workspace includes `rocket-core`, `rocket-physics`, `rocket-aero`,
`rocket-cfd`, `rocket-sim`, `rocket-motors`, `rocket-io`, `rocket-render`,
`rocket-ui`, `rocket-cli`, `rocket-monte-carlo`, `rocket-optimizer`, and
`aether-tests`. The Monte Carlo and optimizer crates are scaffolding, rather
than implemented study engines.

Keep the current data/physics dependency direction. Add model/IR, control,
analysis, codegen, HIL, and replay as tested modules first; extract crates when
ownership and dependencies justify it. No UI, renderer, network, or hardware I/O
may enter the pure model/physics evaluation path. Scheduling and transports live
in host services/adapters, and the UI invokes those services.

A shared application boundary should grow from the current UI-independent
editing document when CLI/study commands require it. It owns revisions, cases,
job lifecycle, artifact identity, and save/import behavior. `rocket-sim` remains
the host orchestrator over a shared plant/observation evaluator. Offline CFD
and code-generation jobs do not execute inside controller ticks. Generated C
exports and optional isolated Julia build tools do not imply arbitrary native
plugin loading in the desktop process.

---

## 7. Physics Core — Detailed Models

These model families remain a technical backlog, subject to the declared
operating envelope and M0 verification. They are not a capability inventory;
production integration and known defects are tracked in the
[development status](docs/development-status.md). The shared-model, derivative,
controller, and execution contracts in the new execution plan take precedence
over older tier assignments and implementation sketches below.

### 7.1 Coordinate frames & transformations

- **ECEF** — Earth-Centered Earth-Fixed (T2).
- **ECI** — Earth-Centered Inertial (T2, for high-altitude/orbital).
- **Local launch frame** — right-handed, x downrange, y cross-range, z up. Explicit ENU/NED transforms require declared site axes; NED uses down-positive z.
- **Body frame** — origin at instantaneous CG; +x out the nose; axes aligned with principal inertia (assumed axially symmetric → roll is a free axis).
- **Wind frame** — used to compute angle of attack α and sideslip β, transform body aero forces.
- **Stability (aerodynamic) frame.**

Attitude represented as a **unit quaternion** `q = (w, x, y, z)` plus body angular velocity `ω`. Conversion matrices cached per step. Euler angles are output-only (for the UI), never the state.

*Validation:* round-trip `quat → DCM → quat`, orthogonality, handedness tests; compare quaternion-integrated attitude vs analytical spinning-top case.

### 7.2 Atmosphere & environment

| Tier | Model | Inputs | Output |
|---|---|---|---|
| T0 | ISA simple: T = T0 − L·h to 11 km, p/hydrostatic, ρ = p/(RT) | altitude | T, p, ρ, a (speed of sound), μ (Sutherland) |
| T1 | **US Standard Atmosphere 1976** (piecewise lapse, troposphere/stratosphere/mesosphere to ~86 km) | altitude | T, p, ρ, a, μ, kinematic viscosity |
| T2 | **NRLMSISE-00** (empirical; includes solar F10.7, geomagnetic Ap, day-of-year, latitude/longitude, local solar time) + **HWM14** horizontal wind model | h, lat/lon, UTC, space-weather | T, ρ, composition, wind vector |

Wind (T0/T1): surface layer power-law `u(h) = u_ref · (h/h_ref)^α` and log-law for stability-corrected profiles; gust model optional. Winds aloft (T1+): interpolate a sounding (user-supplied) or HWM14 (T2).

*Validation:* reproduce US-1976 tables to 6 sig figs; NRLMSISE-00 against the reference C implementation on a fixed input vector.

### 7.3 Aerodynamics

> **Current coefficient contract:** see the [aerodynamic dataset contract](docs/development-status.md#aerodynamic-dataset-contract). Imported data is bound to a vehicle and declared references/validity; static coefficients supply no dynamic derivatives.

The aero module returns coefficients `C_A, C_N, C_Y, C_l, C_m, C_n` (axial, normal, side, roll, pitch, yaw) and the **center of pressure** as functions of (Mach, α, β, roll rate, configuration). This is the biggest fidelity lever.

#### 7.3.1 Subsonic — Extended Barrowman (T0, T1)

Component-by-component superposition (each part contributes a normal-force slope `C_Nα` and a CP location `x_p` measured from the nose tip):

- **Nose cones** — conical, ogive (tangent), elliptical, parabolic (power series), von Kármán, LV-Haack, secant ogive, custom. Closed-form `C_Nα` and `x_p` for each.
- **Cylindrical body** — `C_Nα = 0` per Barrowman (no contribution to static stability), but **body lift at α > 0** added via the *extended* method (Allison–Mandell / RockSim) for non-linear regime.
- **Transitions / boattails** — conical frustrum contribution, sign-correct for expansions.
- **Fins** — trapezoidal, elliptical, custom planform; `C_Nα` from Barrowman with **fin–body interference** (two methods: Barrowman's classic and the more accurate RockSim/Adams); sweep correction; **three-fin and four-fin** sets; **multiple fin sets**.
- **Launch lug** — drag only.
- **Base drag** — empirical (e.g., Krasan / Hoerner): `C_D_base = K · (d_base/d_ref)²`.
- **Skin friction** — laminar (Blasius) and turbulent (Prandtl–Schlichting) `C_f`, with **Reynolds-based transition** and a **body-length** integration; compressibility correction (Karman–Tsien) at T1.
- **Pressure/form drag** of each component via empirical correlations.

Composite `C_Nα_total = Σ C_Nα_i`, `x_cp = Σ(C_Nα_i · x_i) / C_Nα_total`.

#### 7.3.2 Compressibility (T1)

- Subsonic compressibility: **Prandtl–Glauert** → transonic: **von Kármán/Murman-style** empirical drag rise (e.g., Lock's 4th-power law or the empirical transonic model used by RASAero) → **wave drag** at supersonic via component methods.
- **Supersonic CP behavior**: verify Mach- and geometry-dependent CP changes against declared references; evaluate the Vukelich/ST or *Hoerner secant-tangent* methods; fin `C_Nα` follows the **supersonic 2-D + edge correction** (Busemann / linear supersonic theory `C_Nα = 4/(M²−1)` corrected for fin aspect ratio and sweep).
- **Nose wave drag**: Newtonian / tangent-cone / modified-Newtonian per shape.
- Drag divergence near M=1 is the single least-accurate area of analytical aero; we will (a) expose a Cd-vs-Mach override table for users with CFD/wind-tunnel data, and (b) document the model's uncertainty band.

#### 7.3.3 High-α / nonlinear (T1+, optional T0)

- Replace linear `C_N(α) = C_Nα·α` with piecewise nonlinear: linear to a critical α, then ramp to a stall plateau; fin stall uses **Polhamus leading-edge suction analogy** or empirical.
- Body lift at α via crossflow drag (Jorgensen) for high-α regimes (e.g., weathercocking on the pad, dynamic post-burnout).

#### 7.3.4 Dynamic derivatives

- Pitch damping `C_mq`, `C_mα̇` — empirical.
- Roll damping `C_lp` — fin-based empirical.
- Magnus / roll–pitch coupling — T2 only.

*Validation:*
1. **Barrowman reference cases** — audit the report identifier, exact equations and source tables before accepting a fixture. The existing SP-7224 attribution needs correction.
2. **RASAero** comparison at M=0.1, 0.8, 1.2, 2.0 for a panel of 6 canonical rockets (e.g., Goony, DX3, Marsa, etc. with published data).
3. Drag: compare to known Cd vs Mach curves for the Estes Alpha (well-characterized).
4. CP location: never off by more than 0.5 calibers from RASAero at M=0.1 for the validation set.

### 7.4 Rigid-body dynamics (6-DOF)

> **Next dynamics contract:** see [automatic linearization](rocket-simulator-model-to-flight-plan.md#5-automatic-linearization) for explicit plant inputs/observations, quaternion perturbations, modes and derivative validity.

State vector (13 components):
```
x     : position in launch frame  (3)
v     : velocity in launch frame  (3)
q     : attitude quaternion (body→launch) (4)
ω     : body angular velocity      (3)
m     : total mass derived from installed propulsion schedule (not an independent state)
```
(13 stored kinematic scalars; 12 local perturbation coordinates because unit attitude has three independent dimensions. Additional actuator/estimator states are explicit.)

Equations:
- Translation: `m · v̇ = F_thrust + F_aero + F_gravity` (in launch/inertial frame; aero computed in wind frame then rotated).
- Rotation (Euler's equations in body frame, with variable inertia):
  ```
  I(t)·ω̇ + ω × (I(t)·ω) = M_aero + M_thrust + M_jet_damping
  ```
  `I(t)` and mass-flow effects need a consistent installed-propulsion derivation and independent verification before this variable-mass equation is accepted for the supported regime.
- Attitude: `q̇ = ½ · q ⊗ ω_quat`.
- Gravity: T0 constant; T1 `g(h) = g0·(Re/(Re+h))²`; T2 + J2 (oblateness, ~0.1% effect, matters at high altitude) and Coriolis/centrifugal in the rotating launch frame.
- **Jet damping** and **CG offset → thrust moment** are model extensions to derive and verify explicitly.

Forces in detail:
- **Thrust** along body +x, optionally with a small thrust-vector angle (TVC-ready) and thrust-line misalignment (random in Monte Carlo).
- **Aero** = `½ρV²·A_ref·[C_A x̂_body + C_N n̂ + C_Y ŷ]` plus moments about CG: `M = ½ρV²·A_ref·L_ref·[C_l + C_m + C_n] + dynamic derivatives`.
- **Wind**: the rocket sees `V_rel = V_body − V_wind`; α, β derived from `V_rel` direction in body frame.

*Validation:*
- Spin decay of a freely spinning axisymmetric body matches analytical solution.
- Ballistic flight of a fin-stabilized dart: zero lateral drift in still air (symmetry check — catches sign bugs immediately).
- Vertical-shot apogee vs 1-D closed-form for a constant-Cd, constant-m thrust curve (agreement to 0.1%).

### 7.5 Propulsion & mass properties

- **Thrust curve**: time–thrust table from ENG/RASP (NAR standard). Linear interpolation between points; area = total impulse; propellant mass `m_prop = I_total / (Isp·g0)` with `Isp = I_total / (m_prop·g0)` when propellant mass is supplied; keep effective exhaust velocity distinct from Isp in seconds.
- **Mass flow**: `ṁ_prop = −T(t) / (Isp·g0)`; burnt mass integrated.
- **CG / inertia vs burn**: propellant modeled as a uniform fill of the motor casing volume (or a user grain geometry later); CG and `I_axial, I_transverse` must follow the declared casing/propellant geometry and installation. Verify these approximations against independent mass integrals and measured overrides.
- **Clusters**: N motors, each with its own curve and ignition time; sum thrusts and mass flows; offset thrust lines contribute to moments.
- **Staging**: see §7.7.
- **Hybrid/liquid** (future): same external contract (thrust(t), ṁ(t), CG(t)).

### 7.6 Recovery & event system

A generic **event-action** system (so it generalizes to staging, airstart, etc.):

| Event (trigger) | Action |
|---|---|
| `time == t0` | ignite motor |
| `acceleration < threshold` (burnout) | (optional) mark coast start |
| falling zero-crossing of vertical velocity after ascent (apogee) | deploy drogue |
| `altitude ≤ h_main` | deploy main |
| falling crossing of local ground altitude after liftoff | ground impact → end sim |
| `time == t_stage` | separate stage / ignite upper |

Parachute descent integrates relative-air drag `½ρV²·C_d·A`; compare stable descent with the local terminal-speed balance `m·g = ½ρV²·C_d·A`. Opening transients need a separate model. Streamers use declared drag-area inputs.

### 7.7 Staging

- **Separation event**: instant transfer of state (x, v, q, ω) to the upper stage; the spent lower stage becomes its own body that can be tracked (simple ballistic, no recovery) or discarded.
- **Sustainer ignition**: immediate / delayed / airstart-at-velocity / airstart-at-altitude.
- **Couplers & drag of the stack** while joined: aero computed for the *whole stack* until separation; then each stage recomputes its own coefficients.
- Multi-stage just composes the same machinery.

### 7.8 Integrators and execution timing

The production loop currently uses fixed-step RK4 and ISA. DOPRI5 and advanced
atmosphere modules exist separately; their production integration requires
converged-reference comparisons. Numerical methods must use the shared dynamics
and event contracts, with tolerances scaled by state units.

Offline adaptive integration and fixed-rate controller/HIL ticks serve different
purposes. Advance exactly to declared sensor/controller/event boundaries with
defined sample/hold ordering. Benchmark the supported fixed-step HIL/GPU profile
against converged CPU results. Playback scheduling does not choose the physics
integrator, and a renderer frame is not a controller clock. Detect/reject
unsupported stiffness or timing rather than silently switching models.

---

### 7.9 Validation & V&V (cross-cutting)

- **Conservation checks** in CI: in vacuum without external forces/torques or mass change, energy and angular momentum conserved within a convergence-based tolerance (catches force-sign and frame bugs).
- **Analytical cases**: vacuum vertical shot, vacuum spinning body, flat-fire projectile (vacuum + constant Cd).
- **Cross-tool**: automated diff vs RASAero and OpenRocket on a nightly validation set.
- **Flight-data regression**: a small library of real flights with GPS/altimeter data; nightly compare predicted vs flown apogee and descent.

---

## 8. Declarative vehicle and system model

Keep the existing serializable vehicle/component tree, stable IDs, units,
materials and mounting references as the vehicle source. Add measured mass/CG/
inertia definitions with explicit ownership and precedence, rather than hiding
physical overrides in UI state. The runtime schema in `rocket-core` remains
authoritative; the shared-model compiler must not create a parallel vehicle
representation that users maintain separately.

Extend the package with model/provider identities, typed controller operations
and state, sensors/actuators, signal dictionary, scheduling, scenarios and
observation mappings. Units and frames are checked at connections; fixed-size
numeric arrays at execution boundaries retain their dimensional metadata in
the model and generated interface description. Parameters are editable data;
equations and sampled semantics resolve through versioned graph/primitives.

See the [shared model contract](rocket-simulator-model-to-flight-plan.md#3-shared-model-and-compilation-boundary)
for required package sections and the compilation boundary. Schema migration,
source mapping and artifact identity are part of this model, not later export
features.

---

## 9. Motor Library & Thrust Curves

- **Format**: NAR RASP `.eng` and RockSim `.rse` parsers; both are de-facto standards. Also accept simple CSV (`t_s, thrust_N`).
- **Catalog**: keep the existing on-demand ThrustCurve API/cache and embedded offline sample curves. Retain each curve's source/terms and exact data in reproducible cases; expand redistribution only with explicit provenance.
- **Thrust-curve interpolation**: PCHIP (monotonic) to avoid overshoot between data points; area-preserving.
- **Metadata**: designation, class, diameter, length, total/peak/average thrust, burn time, propellant mass, total mass, CG shift (if available), delays (for single-use motors with built-in delays), certification.
- **Custom motors**: a form to enter `t, F` points or paste a curve; the system computes impulse, Isp, mass flow.

---

## 10. Monte Carlo and optimization

Implement repeatable CPU ensembles with persisted parameter samples, correlated
uncertainty, per-run seeds, failure retention, and criteria. Then implement and
benchmark one eligible GPU plant/controller subset using the same samples,
precision, timestep, and event policy. Publish complete timing including build,
transfer, and output cost against a multicore CPU baseline and a converged
physical reference. See [CPU and GPU Monte Carlo](rocket-simulator-model-to-flight-plan.md#7-cpu-and-gpu-monte-carlo).

Automatic linearization precedes general optimization. Add parameter estimation
for replay as a reviewed model revision with identifiability and held-out checks.
Vehicle/controller optimization follows credible objectives, constraints, and
uncertainty; it is not on the first model-to-flight release's critical path.
The earlier unconditional 10,000 runs in 60 seconds promise is replaced by a
published benchmark contract and useful measured acceleration gate.

## 11. Engineering user workflows

Preserve the current Design, Flight case, Results, and Aerodynamics workspaces,
visible Save action, reversible edits, retained comparisons, and data exports.
The GUI and CLI must expose the same input validity, model/build identity,
termination status, and diagnostics.

Introduce additional views only as their services become usable: Model and
interfaces (units/frames/rates and validation); Control and analysis (controller
state, generated artifacts, local matrices and operating point); Test bench
(SIL/HIL clocks, packets, latency and deadline misses); Replay (measured,
estimated and simulated channels, alignment, residuals and proposed revisions).
Results link to the exact model, controller binary and build receipt used.

Use small reusable forms, signal tables, linked engineering plots, and visible
job status before attempting docking or a block-diagram editor. A schema/CLI
workflow must be reproducible without the desktop. Continue native input,
accessibility, scaling, large-data and responsiveness checks as views grow.

---

## 12. 3D Rendering (`rocket-render`)

> **3D library decision:** see [`docs/adr/0010-raw-wgpu-for-3d-rendering.md`](./docs/adr/0010-raw-wgpu-for-3d-rendering.md). rend3, scena, and three-d were all rejected; we use raw wgpu directly.

- Raw wgpu (sharing egui's wgpu instance via `egui_wgpu::CallbackTrait`); custom `RocketMesh` built from the component tree (each part → vertices + indices → GPU buffers). Mesh is rebuilt only when geometry changes (dirty flag), not every frame.
- **Procedural geometry**: nose cones from lathe of the profile function; fin sets instanced; body tubes as cylinders with optional thickness for cutaway.
- **Camera**: orbit (design), follow (playback), free-fly.
- **Picking**: ray-cast against the scene to select a component in the 3D view → syncs the tree & properties (huge UX win).
- **Flight FX**: particle trail colored by Mach, velocity vectors, AO/Mach cone hint at supersonic, deployment events animated.
- **Export**: STL/STEP of the geometry for manufacturing (via `opencascade-rs` or a STEP exporter later — out of v1 scope, but the geometry pipeline is structured for it).

---

## 13. Persistence and evidence artifacts

Current schema-3 `.rocket` JSON contains the vehicle, one editable flight case,
and retained runs. Schema-1 designs and schema-2 projects migrate explicitly.
Run JSON preserves complete inputs, versions, outcomes/events and telemetry;
CSV exports numerical channels. Keep these usable throughout the new work.

Extend through versioned schemas to model/controller/experiment definitions,
multiple named cases, observation mappings, raw measured data, build receipts,
linearizations, uncertainty samples, hardware traces and reconciliation reports.
Use immutable, content-identified artifacts and relative manifest references;
verify hashes and dependencies on import. Separate human-editable model source
from derived/generated files and runtime state. Introduce a directory/bundle
format and large-array storage only when needed, with explicit migrations and
read compatibility; do not silently reinterpret existing run records.

Each generated artifact records source model/graph hash, schema/compiler and
target versions, numerical profile, ABI/protocol, options and artifact hash.
A review bundle links preflight prediction, SIL, HIL and measured-data reports.
Calibration creates a new revision; original measured logs and prior baselines
remain immutable. Large studies retain summaries plus selected traces rather
than embedding unlimited full logs in every project snapshot.

The initial plant derives mass from its propulsion schedule; actuator, estimator,
controller and event states are explicit. Initial/reset state and sample ordering
are persisted so replay and regenerated C can reproduce a test.

---

## 14. Project Structure & Conventions

- **Workspace** as in §6; one concern per crate.
- **`#![deny(unsafe_code)]`** at workspace root — no unsafe unless a reviewed ADR justifies it.
- **Units**: `rocket-units` newtypes; SI internally; UI shows user units.
- **Error handling**: `thiserror` for library errors, `color-eyre` in the binary for diagnostics.
- **Logging**: `tracing` + `tracy` for profiling the flight loop.
- **ADRs**: every non-trivial decision recorded in `docs/adr/` (e.g., "why RK45 not a symplectic integrator", "why egui not iced", "why PCHIP for thrust curves").
- **Benchmarks**: `criterion` benches for the flight loop, aero coefficient computation, and mesh build; tracked in CI against a baseline.

---

## 15. Delivery roadmap

The current priority order replaces the earlier hobby-to-amateur phase ladder.
Advance by evidence gates rather than one-engineer calendar promises. Detailed
work packages are in the [model to flight execution plan](rocket-simulator-model-to-flight-plan.md#10-delivery-gates-and-first-work-packages).

| Gate | Deliverable | Exit evidence |
|---|---|---|
| M0 | Physical model repair and pure plant/observation/timing contracts | Independent installed mass/CG/inertia, frame, force/moment and rate tests; declared operating envelope; existing-file compatibility |
| M1 | Shared declarative package and restricted graph; Rust/Julia backend spike | One small model evaluates, differentiates, generates stateful C and exercises a GPU path from shared definitions; recorded architecture choice and gaps |
| M2 | Linearization, compiled controller SIL, CPU ensembles and initial replay | Named A/B/C/D or discrete maps pass perturbation checks; two-compiler controller parity; reproducible samples/failures; synthetic and measured-data mappings |
| M3 | GPU Monte Carlo backend for the supported subset | Paired CPU/GPU correctness and convergence evidence; published end-to-end benchmark versus multicore CPU |
| M4 | One HIL board and transport | Same generated binary identity, loopback and real-board evidence, clock/latency/fault/reset tests and measured deadline behavior |
| M5 | Full reconciliation pilot and one external tool adapter | Held-out residuals and explicit revisions; second engineer regenerates/repeats the study, including the selected external host |

M1 is a timeboxed prototype; M2 is the first developer preview. Replay ingestion
and CPU studies begin before hardware integration. GPU support remains a
concrete deliverable with its own measured go/no-go gate. Broader dynamics,
CFD refinement and orbital/ADCS expansion follow a demonstrated requirement;
physical verification remains concurrent with every gate.

## 16. Risks and mitigations

| Risk | Consequence | Response |
|---|---|---|
| Shared parameters hide separately authored equations | Targets drift despite matching filenames | Restricted typed IR/evaluator, source mappings, versioned primitives, derivative and backend parity tests |
| AD crosses a non-smooth model/event boundary | Misleading control linearization | Mode/domain-specific derivatives and explicit rejection; quaternion tangent coordinates; nonlinear perturbation tests |
| GPU kernel uses weaker physics or omits failed samples | Attractive speed with misleading results | Named supported subset, paired inputs/outcomes, converged reference and full timing contract |
| Generated C changes precision, reset, state or timing behavior | SIL and board diverge | Explicit state/update semantics, two-compilers, target-precision tests, binary/build receipts and board measurements |
| Desktop pacing is treated as hard real time | HIL timing evidence is unreliable | Measured deadlines, fault policy, bounded timed path and separate real-time-host qualification |
| Calibration hides sensor/time/model errors | A fit appears accurate without predicting new data | Observation models, explicit clock alignment, raw-log retention, identifiability and held-out evaluation |
| Rust and Julia become independent model sources | Small team maintains duplicate physics | Shared graph/artifacts; bounded backend spike; optional Julia role selected only on evidence |
| Every vendor or vehicle regime becomes first-release scope | Delivery stalls | One rocket/controller, one board, one log adapter and one external pilot adapter; defer general imports/orbital stack |
| Unverified existing mass/aero behavior is inherited | New tools amplify wrong results | M0 repairs with independent fixtures before engineering claims; maintain an evidence inventory |

## 17. Verification and validation strategy

Keep existing Rust tests, strict Clippy, formatting, build/docs, driver contract
and dependency checks. Extend them around independent answers and cross-target
behavior, rather than relying on coverage percentages or cross-tool agreement
alone.

- Model schemas: units/frames/rates, graph validity, migration, hashes and source
  diagnostics, including unsupported target operations.
- Physics and analysis: analytical mass/forces/torques, conservation and solver
  convergence; AD versus independent derivatives; local perturbation response,
  quaternion tangent basis, modes, resets and sampled-data behavior.
- Controller generation: compiled-C vectors, reset/limit/fault/long-run cases,
  two compilers, target precision and nonlinear SIL parity.
- Ensembles: persisted inputs, seed/worker-order invariance, correlated sampling,
  failure retention, CPU/GPU per-case and aggregate comparisons, full benchmarks.
- HIL: loopback plus real-board tests with binary identity, latency/jitter/drift,
  packet faults, resets and deadline evidence on the named host/board.
- Replay: known clock/unit/frame/calibration fixtures, gaps/resets, proper sensor
  observations, independent measured data and held-out reconciliation.

Verification of the implementation, physical validation for a declared use,
code-generation equivalence and target timing qualification are separate
claims. Every release report identifies which cases support each claim.

---

## 18. Architecture decisions and their scope

[ADR-0011](docs/adr/0011-shared-model-and-execution-targets.md) extends the layered
Rust architecture with shared model semantics and target/evidence contracts.
Existing ADRs remain decisions within their stated scopes; earlier assumptions
about hobby-first rollout, fixed performance promises, or deferred HIL no longer
set delivery priorities.

| Decision | Current disposition |
|---|---|
| ADR-0001 layering | Retain Rust workspace and pure boundaries; extend through model/control/analysis and host adapters. |
| ADR-0002 UI and ADR-0010 rendering | Keep egui and raw wgpu provisionally, measured against real workflows. Rendering compute and simulation compute remain separate. |
| ADR-0003 numerical integration | DOPRI5 remains a planned offline path; production currently uses RK4. Controller/HIL rates need their own scheduling contract. |
| ADR-0004 distribution | Native desktop distribution remains; generated C and optional build/analysis tools are additional artifacts. |
| ADR-0005 licensing | Existing workspace license remains; generated artifacts need explicit provenance and dependency notices as part of generation tooling. |
| ADR-0006 motors | Preserve per-file provenance and existing on-demand catalog/offline fixtures. |
| ADR-0007 progressive disclosure | Retain progressive UI disclosure; expose supported model combinations instead of assuming a tier establishes accuracy. |
| ADR-0008 persistence | Keep local files, safe saving and explicit migrations; extend to immutable model/build/telemetry evidence bundles. |
| ADR-0009 scripting | Scripts remain optional orchestration hooks. Restricted controller code generation and C-callable exported artifacts are distinct from loading arbitrary native plugins. |

The Rust/Julia tooling choice, precise IR subset, first board, GPU backend and
external adapter are resolved by M1 measurements and pilot constraints, not by
introducing a second production implementation ahead of evidence.

---

## 19. Immediate next steps

1. Define the controlled reference and evidence manifest; repair installed motor
   mass/CG/inertia and verify aerodynamic force/moment/rate conventions.
2. Extract pure plant/observation and sampled controller interfaces; specify
   units, frames, state/reset behavior and deterministic scheduling.
3. Run the ten-engineering-day small-model backend spike for shared definitions,
   automatic linearization, controller C generation and GPU feasibility. Record
   Rust/Julia tradeoffs and choose the implementation route.
4. Deliver M2 as a usable headless/desktop developer preview, preserving existing
   projects and adding CPU ensembles and initial telemetry replay alongside SIL.
5. Select pilot data, one HIL board/protocol and one external host; plan M3–M5
   from measured capability, integration effort and validation evidence.

*Document version 0.3 — shared model, controller generation, GPU ensembles, HIL,
and flight reconciliation adopted as the product direction.*
