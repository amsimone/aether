# Rocket Simulator — Engineering Plan

**Project:** (working name) **Aether** — a professional, tiered rocket design & 6-DOF flight simulator
**Stack:** Rust workspace (physics core) + egui (UI) + wgpu/three-d (3D), native desktop
**Scope target:** Model/low-power → high-power → amateur/experimental (to Karman line), tiered
**Status:** Planning

---

## 1. Vision & Goals

Build a rocket simulator that surpasses OpenRocket in two axes:

1. **Physics fidelity** — a layered engine that is correct for subsonic model rockets *and* scales to supersonic/amateur flight with compressible aerodynamics, stratified atmospheres, staging, and 6-DOF attitude dynamics with quaternions (no Euler singularities).
2. **Interface quality** — a modern, dark, professional native UI with a real-time 3D design viewport, interactive trajectory plots, flight playback, and exportable reports — not a 2005-era Swing look.

### Design principles
- **Correctness first, polish second, speed third** — but never ignore speed; Monte Carlo and optimization depend on it.
- **Layered physics** — the same rocket file simulates with increasing fidelity tiers; results are comparable across tiers.
- **Deterministic & reproducible** — every sim is a pure function of (design, environment, motor, seed). No hidden state.
- **Library-first** — the physics core is a library with no UI dependencies. A headless CLI and a Rust API exist alongside the GUI.
- **Verifiable** — every aero/dynamics module has unit tests and validation cases against published data and other tools (RASAero, OpenRocket, flight data).

---

## 2. Scope Tiers

The "tiered" choice means the physics core exposes multiple fidelity levels. The user picks a tier per simulation; higher tiers add models, never replace correct ones.

| Tier | Name | Altitude | Mach | Atmosphere | Gravity | Earth | Dynamics |
|------|------|----------|------|------------|---------|-------|----------|
| T0 | **Model / LPR–MPR** | ≤ 5 km | ≤ 0.6 | ISA (sea-level + lapse rate) | Constant g, flat | Flat | 6-DOF (linear aero) |
| T1 | **HPR** | ≤ 30 km | ≤ 3.0 | US Standard Atmosphere 1976 | g(h), flat + Coriolis optional | Flat | 6-DOF, compressible aero |
| T2 | **Amateur / sounding** | ≤ 100 km | ≤ 5+ | NRLMSISE-00 + wind model | g(h), J2 | Round Earth, rotating | 6-DOF, full compressible, staging, TVC-ready |

T0 must be excellent before T1; T1 before T2. Each tier is a milestone gate, not a feature flag sitting unused.

---

## 3. Non-goals (for v1.0)

- CFD / Navier–Stokes solving (we use engineering correlation methods; CFD coefficients can be *imported*).
- Liquid-bipropellant engine internals (motor is a thrust curve + mass properties).
- Real-time hardware-in-the-loop / flight computer emulation (future plugin).
- Multiplayer / cloud sync.
- CAD-quality manufacturing drawings (export STEP/STL is in scope; full GD&T is not).

---

## 4. High-level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                       rocket-ui (egui)                        │
│  Design editor │ Sim control │ Plots │ 3D playback │ Reports │
└────────────┬───────────────────────────────────┬────────────┘
             │  (pure data)                      │ (wgpu surface)
┌────────────▼───────────────┐      ┌────────────▼────────────┐
│      rocket-render          │      │      rocket-physics      │
│  procedural geometry,       │      │  atmosphere, aero,       │
│  scene, camera, picking     │      │  dynamics, propulsion    │
└────────────┬───────────────┘      └────────────┬────────────┘
             │                                    │
┌────────────▼────────────────────────────────────▼────────────┐
│                     rocket-sim (orchestration)                 │
│   integrators, event system, flight loop, telemetry stream    │
└────────────┬───────────────────────────────────┬────────────┘
             │                                    │
┌────────────▼───────────────┐      ┌────────────▼────────────┐
│      rocket-core            │      │   rocket-monte-carlo     │
│  data model, components,    │      │   + rocket-optimizer     │
│  materials, units, frames   │      │   sampling, stats, CMA-ES│
└────────────┬───────────────┘      └───────────────────────────┘
             │
┌────────────▼───────────────┐      ┌───────────────────────────┐
│      rocket-io              │      │      rocket-motors         │
│  save/load, ENG/RASP parse, │      │  thrust-curve lib,         │
│  CSV/KML/PDF export         │      │  catalog, search           │
└─────────────────────────────┘      └───────────────────────────┘
                       │
                  rocket-cli  (headless batch)
```

Key property: **`rocket-physics` and `rocket-core` depend on nothing UI-related.** This enables a CLI, a REPL, WASM builds, and fast property tests.

---

## 5. Technology Stack & Rationale

| Concern | Choice | Why |
|---|---|---|
| Language | Rust | Performance for Monte Carlo + optimizer; strong type system for units/state; zero-cost abstractions for hot loops. |
| UI | **egui** + `egui_dock` + `egui_plot` | Immediate mode → fast iteration on a design tool; docking panels; trivial to embed a wgpu viewport; custom theming gives a modern dark look. *(Alternative: iced — cleaner defaults, but slower to build a dense tool UI. Revisit at Phase 1 review.)* |
| 3D rendering | **wgpu** + `three-d` crate | Cross-platform (Metal/Vulkan/DX12); procedural rocket geometry from parametric parts; instancing for particle trails; depth peeling for transparent parts. |
| Math | `nalgebra` (linear), `ndarray` (arrays/stats), `apollo`-style quaternion ops | `nalgebra` has first-class quaternions and rotations; SIMD-friendly. |
| Interpolation / ODE | `argmin` (optimization), hand-rolled RK45 with event detection, `ndarray-stats` | We need event detection (apogee, ground, deployment) which off-the-shelf solvers handle awkwardly; a custom RK45 is small and well-tested. |
| Persistence | `serde` + JSON for designs; ENG/RASP for motors; Parquet (`polars`) for large sim logs | Human-readable designs; binary Parquet for telemetry of 10k-run Monte Carlos. |
| Parallelism | `rayon` for Monte Carlo batches | Embarrassingly parallel across runs. |
| CLI | `clap` | Standard. |
| Build/CI | `cargo` workspace, `cargo-nextest`, `cargo-deny`, `cargo-mutants`, `typos`, GitHub Actions | Rigorous CI from day one. |

---

## 6. Crate / Workspace Breakdown

```
rocket-sim/
├── Cargo.toml                      # workspace
├── crates/
│   ├── rocket-core/                # data model, no deps on physics/UI
│   ├── rocket-units/               # dimensional types: Length, Velocity, Pressure… (or submodule of core)
│   ├── rocket-physics/             # atmosphere, aero, dynamics, propulsion models
│   ├── rocket-sim/                 # flight loop, integrators, events, telemetry
│   ├── rocket-aero/                # (optional split) aero coefficient prediction — kept separate for clarity & testing
│   ├── rocket-motors/              # ENG/RASP parser, catalog, thrust-curve interpolation
│   ├── rocket-monte-carlo/         # distributions, LHS sampler, stats
│   ├── rocket-optimizer/           # CMA-ES + Nelder-Mead wrappers, objective builders
│   ├── rocket-render/              # wgpu/three-d scene, geometry from components, camera, picking
│   ├── rocket-ui/                  # egui app: panels, views, theming, docking
│   ├── rocket-io/                  # save/load, CSV/KML/STL/PDF export
│   └── rocket-cli/                 # headless batch runner
├── assets/                         # icons, fonts, default theme
├── data/
│   ├── motors/                     # bundled ENG files
│   └── atmosphere/                 # NRLMSISE-00 tables, wind model data
├── benches/                        # criterion benchmarks
├── tests/                          # integration + validation suite
└── docs/                           # this plan, ADRs, validation reports
```

---

## 7. Physics Core — Detailed Models

This is the heart of "not amateurish." Each subsection lists the model, its tier, and validation source.

### 7.1 Coordinate frames & transformations

- **ECEF** — Earth-Centered Earth-Fixed (T2).
- **ECI** — Earth-Centered Inertial (T2, for high-altitude/orbital).
- **NED / ENU launch frame** — local tangent, x = downrange, z = up. (T0, T1.)
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

> **Full technical spec:** see [`rocket-simulator-aero-dynamics.md`](./rocket-simulator-aero-dynamics.md) Part I — frames, coefficient contract, per-component formulas, compressibility, validation cases, and the Rust API.

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
- **Supersonic CP shift**: CP moves *forward* with Mach; use the Vukelich/ST or the *Hoerner secant-tangent* methods; fin `C_Nα` follows the **supersonic 2-D + edge correction** (Busemann / linear supersonic theory `C_Nα = 4/(M²−1)` corrected for fin aspect ratio and sweep).
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
1. **Barrowman SP-7224** reference cases (the original report worked example) — exact-match to its tables.
2. **RASAero** comparison at M=0.1, 0.8, 1.2, 2.0 for a panel of 6 canonical rockets (e.g., Goony, DX3, Marsa, etc. with published data).
3. Drag: compare to known Cd vs Mach curves for the Estes Alpha (well-characterized).
4. CP location: never off by more than 0.5 calibers from RASAero at M=0.1 for the validation set.

### 7.4 Rigid-body dynamics (6-DOF)

> **Full technical spec:** see [`rocket-simulator-aero-dynamics.md`](./rocket-simulator-aero-dynamics.md) Part II — state vector, frame transformations, quaternion kinematics, Euler equations with variable inertia, force/moment assembly, integrator & event detection, and conservation checks.

State vector (13 components):
```
x     : position in launch frame  (3)
v     : velocity in launch frame  (3)
q     : attitude quaternion (body←launch) (4)
ω     : body angular velocity      (3)
m     : total mass                 (1)   -- or kept as derived from t + motor
```
(treated as 13 with mass, or 12 with mass recomputed from burn schedule — design decision in §13.)

Equations:
- Translation: `ṁ v = F_thrust + F_aero + F_gravity` (in launch/inertial frame; aero computed in wind frame then rotated).
- Rotation (Euler's equations in body frame, with variable inertia):
  ```
  I(t)·ω̇ + ω × (I(t)·ω) = M_aero + M_thrust + M_jet_damping
  ```
  `I(t)` recomputed each step from current propellant mass distribution.
- Attitude: `q̇ = ½ · q ⊗ ω_quat`.
- Gravity: T0 constant; T1 `g(h) = g0·(Re/(Re+h))²`; T2 + J2 (oblateness, ~0.1% effect, matters at high altitude) and Coriolis/centrifugal in the rotating launch frame.
- **Jet damping** and **CG offset → thrust moment** included (these matter and OpenRocket undersells them).

Forces in detail:
- **Thrust** along body +x, optionally with a small thrust-vector angle (TVC-ready) and thrust-line misalignment (random in Monte Carlo).
- **Aero** = `½ρV²·A_ref·[C_A x̂_body + C_N n̂ + C_Y ŷ]` plus moments about CG: `M = ½ρV²·A_ref·L_ref·[C_l + C_m + C_n] + dynamic derivatives`.
- **Wind**: the rocket sees `V_rel = V_body − V_wind`; α, β derived from `V_rel` direction in body frame.

*Validation:*
- Spin decay of a freely spinning axisymmetric body matches analytical solution.
- Ballistic flight of a fin-stabilized dart: zero lateral drift in still air (symmetry check — catches sign bugs immediately).
- Vertical-shot apogee vs 1-D closed-form for a constant-Cd, constant-m thrust curve (agreement to 0.1%).

### 7.5 Propulsion & mass properties

- **Thrust curve**: time–thrust table from ENG/RASP (NAR standard). Linear interpolation between points; area = total impulse; propellant mass `m_prop = I_total / (Isp·g0)` with `Isp = I_total / m_prop` (or supplied per motor).
- **Mass flow**: `ṁ_prop = −T(t) / (Isp·g0)`; burnt mass integrated.
- **CG / inertia vs burn**: propellant modeled as a uniform fill of the motor casing volume (or a user grain geometry later); CG and `I_axial, I_transverse` recomputed by integrating over remaining propellant. This is what OpenRocket does *approximately*; we do it *exactly* for the casing + propellant assembly.
- **Clusters**: N motors, each with its own curve and ignition time; sum thrusts and mass flows; offset thrust lines contribute to moments.
- **Staging**: see §7.7.
- **Hybrid/liquid** (future): same external contract (thrust(t), ṁ(t), CG(t)).

### 7.6 Recovery & event system

A generic **event-action** system (so it generalizes to staging, airstart, etc.):

| Event (trigger) | Action |
|---|---|
| `time == t0` | ignite motor |
| `acceleration < threshold` (burnout) | (optional) mark coast start |
| `vertical velocity ≤ 0` (apogee) | deploy drogue |
| `altitude ≤ h_main` | deploy main |
| `altitude ≤ 0` | ground impact → end sim |
| `time == t_stage` | separate stage / ignite upper |

Parachute descent: `m·g = ½ρV²·C_d·A`; terminal velocity solved directly for stable phase; transient modeled for the opening shock (optional impulse). Streamers via lower Cd.

### 7.7 Staging

- **Separation event**: instant transfer of state (x, v, q, ω) to the upper stage; the spent lower stage becomes its own body that can be tracked (simple ballistic, no recovery) or discarded.
- **Sustainer ignition**: immediate / delayed / airstart-at-velocity / airstart-at-altitude.
- **Couplers & drag of the stack** while joined: aero computed for the *whole stack* until separation; then each stage recomputes its own coefficients.
- Multi-stage just composes the same machinery.

### 7.8 Integrators & numerical methods

- **Primary: RK45 (Dormand–Prince)** adaptive, with event detection via dense output + root finding (Brent) on event functions. Tolerances: `atol = 1e-9 m`, `rtol = 1e-7`.
- **Fallback / fast-path: fixed-step RK4** at 1 kHz for real-time playback rendering (cheaper, and visually identical).
- **Stiffness**: if a sim has a TVC controller with fast dynamics, switch to implicit Euler / BDF4 in that phase only. (T2, future.)
- **Step-size guards**: clamp Δt ≤ 0.01 s during burn (thrust changes fast), allow larger in coast.
- Event detection: apogee (`ż=0`), deployment altitudes (`z=h`), ground (`z=0`), separation times.

### 7.9 Validation & V&V (cross-cutting)

- **Conservation checks** in CI: post-burnout, total energy + angular momentum monotonic within integrator tolerance (catches force-sign and frame bugs).
- **Analytical cases**: vacuum vertical shot, vacuum spinning body, flat-fire projectile (vacuum + constant Cd).
- **Cross-tool**: automated diff vs RASAero and OpenRocket on a nightly validation set.
- **Flight-data regression**: a small library of real flights with GPS/altimeter data; nightly compare predicted vs flown apogee and descent.

---

## 8. Design Data Model

Everything is a strongly-typed, `serde`-serializable tree. Sketch (final field list in an ADR):

```rust
pub struct Rocket {
    pub name: String,
    pub version: u32,                      // format version for migrations
    pub root: Stage,
    pub materials: MaterialLibrary,
    pub overrides: AeroOverrides,         // user-supplied Cd/Mach tables, CP shifts
    pub default_motor: Option<MotorRef>,
}

pub struct Stage {
    pub components: Vec<Component>,       // ordered nose → tail
    pub motor_mount: Option<MotorMount>,
    pub recovery: Vec<RecoveryDevice>,
    pub events: Vec<Event>,               // staging, airstart, deploy…
    pub upper_stage: Option<Box<Stage>>,  // sustainer
}

pub enum Component {
    NoseCone(NoseCone),
    BodyTube(BodyTube),
    Transition(Transition),
    Coupler(Coupler),
    FinSet(FinSet),
    LaunchLug(LaunchLug),
    Pod(Pod),                             // side-mounted clusters, camera pods…
    MassObject(MassObject),               // batteries, altimeters, ballast
}

pub struct FinSet {
    pub count: u8,                        // 3, 4, 6…
    pub planform: FinPlanform,            // Trapezoid{…}, Elliptical{…}, Custom(Polygon)
    pub airfoil: Airfoil,                 // Square, Rounded, Airfoil(thickness, …)
    pub cross_section: CrossSection,
    pub material: MaterialRef,
    pub thickness: Length,
    pub mount: ComponentId,               // which body tube
    pub position: Position,               // axial + radial
}
```

- All dimensional quantities are newtyped (`Length`, `Mass`, `Velocity`, `Force`, `Pressure`…) with compile-time unit tracking (SI internally, display in user's chosen units — imperial/metric). No `f64` floats in the data model.
- Component identity via stable IDs (so partial edits don't break references).
- Material library shared; materials referenced by ID.

---

## 9. Motor Library & Thrust Curves

- **Format**: NAR RASP `.eng` and RockSim `.rse` parsers; both are de-facto standards. Also accept simple CSV (`t_s, thrust_N`).
- **Catalog**: bundle a curated subset of ThrustCurve.org (license-respected); allow user to drop files into `data/motors/` and auto-index. Index stored as a small SQLite or JSON file for fast filtering by class, diameter, total impulse, manufacturer.
- **Thrust-curve interpolation**: PCHIP (monotonic) to avoid overshoot between data points; area-preserving.
- **Metadata**: designation, class, diameter, length, total/peak/average thrust, burn time, propellant mass, total mass, CG shift (if available), delays (for single-use motors with built-in delays), certification.
- **Custom motors**: a form to enter `t, F` points or paste a curve; the system computes impulse, Isp, mass flow.

---

## 10. Monte Carlo & Optimization

> **Full technical spec:** see [`rocket-simulator-mc-optimization.md`](./rocket-simulator-mc-optimization.md) — sampling methods (LHS/Sobol), uncertainty model, parallel execution engine, statistical aggregation, CMA-ES with restarts, constraint handling, mixed-integer strategy, and validation cases.

### 10.1 Monte Carlo

- **Distributions** attachable to: motor total impulse (±%), burn time, launch rod angle & heading, wind speed/direction (and shear), surface roughness, fin alignment (cant & toe-in), CG location, dry mass, recovery deployment altitude & time.
- **Samplers**: Latin Hypercube (default, converges faster than pure random), Sobol (for >6 params), pure MC (for comparison).
- **Engine**: `rayon` parallel, each run is a full sim; results aggregated into per-channel statistics.
- **Outputs**: histograms of apogee, max Q, max Mach, drift, landing ellipse (with covariance → ellipse on a map), stability-margin-over-time envelope (percentiles).
- **Target**: 10,000 runs of a typical HPR flight in <60 s on a modern laptop (this drives the performance budget → single-sim target <5 ms for a 60 s flight, achievable with adaptive stepping + good aero caching).

### 10.2 Optimization

- **Objectives** (composable, weighted): maximize apogee, minimize drift, hit target altitude, achieve minimum static margin in [1,2] calibers, minimize peak deceleration.
- **Variables**: fin root/tip chord, span, sweep, count; nose length & shape; body diameter/length; ballast mass & position; motor selection (discrete).
- **Methods**: **CMA-ES** (continuous vars, robust, gradient-free) via `argmin`; discrete choices (motor pick, fin count) via outer enumeration or mixed-integer wrapper.
- **Constraints**: static margin range, fin thickness ≥ structural min, CG ahead of CP at all Mach, max dynamic pressure.
- Surrogate model (kriging/Gaussian process) once CMA-ES evaluations exceed ~200, to accelerate.

---

## 11. UI/UX Architecture

### 11.1 Layout (egui_dock workspace)

```
┌──────────────────────────────────────────────────────────────────┐
│  Menu bar  │  Rocket ▸ Stage ▸ …  │  Run ▸  Tier ▸  Export ▸       │
├───────────┬──────────────────────────────────┬───────────────────┤
│ Component │            3D Viewport            │   Properties      │
│  tree     │   (design or flight playback)    │  (selected part)  │
│           │                                   │                   │
│  - Nose   │      [rotate / section / grid]   │  Material ▾        │
│  - Body   │                                   │  Length   250 mm  │
│  - Fins ▸ │                                   │  …                │
│  - Motor  ├───────────────────────────────────┤                   │
│  - Recov. │  Plots: altitude, v, Mach, α,    │  Mass/CG/CP readout│
│           │  stability margin, thrust, G-load │  Static margin bar │
├───────────┴───────────────────────────────────┴───────────────────┤
│  Status: mass 1.24 kg │ CG 0.62 m │ CP 0.71 m │ SM +1.4 cal │ ⚠ 0 │
└──────────────────────────────────────────────────────────────────┘
```

- **Dockable, persistable layout.** Remember the user's arrangement across sessions.
- **Three modes**: *Design* (edit parts, see instant mass/CG/CP), *Simulate* (configure environment & run), *Analyze* (results, plots, playback, Monte Carlo dispersion). Mode switch in the header; layout swaps per mode.

### 11.2 Design editor

- **Component tree** (left) — drag to reorder, context menu to add/duplicate/delete.
- **3D viewport** — real-time procedural mesh; sectioned/cutaway toggle; show CG (●), CP (◆), and the stability-margin ruler; live update as you drag sliders in Properties.
- **Properties panel** — typed inputs with units toggle; sliders with numeric field; live validation (red when constraint violated).
- **Smart defaults** — adding a fin set auto-orients to the body tube you clicked; adding a motor mount offers compatible motors.

### 11.3 Simulate panel

- Environment: launch site (lat/lon/elevation or preset), launch angle/heading, pad height, tier selector with an "estimated runtime" hint.
- Wind: surface speed/dir + profile, or load a sounding.
- Motor selection + ignition events.
- Run button → progress bar; cancelable; results stream into a telemetry channel that the plots and 3D playback consume live.

### 11.4 Analyze / playback

- **Time slider** scrubs the 3D flight (rocket attitude + trail + Mach coloring on the trail).
- **Linked plots**: hovering a plot highlights the 3D rocket at that t; clicking a plot region zooms all.
- **Monte Carlo view**: 3D dispersion of trajectories + landing ellipse on a simple map; percentile bands on plots.
- **Comparison**: overlay multiple runs/sims for A/B.

### 11.5 Visual identity

- Dark theme by default (deep charcoal `#1a1d23`, panels `#232830`, accent electric-blue `#4aa8ff`, warning amber `#f0a020`, danger red `#e5484d`).
- Single typeface with tabular figures for all numeric fields (so columns of numbers align).
- Custom-drawn stability margin gauge, thrust-curve mini-chart, and CG/CP side ruler — these small bespoke widgets are what separate a "designed" tool from a generic form.
- Subtle motion: 120 ms easing on panel transitions; no gratuitous animation.

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

## 13. Persistence, Formats & I/O

| Artifact | Format | Notes |
|---|---|---|
| Rocket design | `.rocket` (JSON, gzipped) | Versioned schema, `serde` migrations, diff-friendly. Import OpenRocket `.ork` is a stretch goal. |
| Sim results (single) | JSON + Parquet telemetry table | Embedded in `.simrun` zip. |
| Sim results (Monte Carlo) | Parquet (one row per run per timestep, or summary only) | Lazy-loaded for plots. |
| Motors | `.eng`, `.rse`, CSV | Standard parsers. |
| Export | CSV (telemetry), KML (trajectory on Earth), PDF (flight report), STL (geometry) | Reports via a templating engine (`typst` for PDF — clean, modern, scriptable). |

**State-vs-time decision (§7.4)**: mass is *derived* from `t` + the motor schedule rather than carried in the integrator state. This keeps energy/momentum conservation checks clean (mass changes are an explicit thrust term) and means a sim is fully reproducible from `(Rocket, Environment, MotorSchedule, seed)` — no hidden integrated mass drift.

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

## 15. Milestones & Roadmap

Each milestone is a **demoable**, tested gate. Estimates assume one focused engineer.

### Phase 0 — Foundation  *(~2 wks)*
- Workspace, CI (fmt, clippy, nextest, deny), ADR template.
- `rocket-core`: data model, units, materials, basic component types.
- `rocket-io`: save/load `.rocket`; motor ENG parser.
- *Gate:* can save/load a trivial rocket and parse an `.eng` file with tests.

### Phase 1 — Design editor  *(~4 wks)*
- `rocket-ui` skeleton (egui + egui_dock), theming.
- `rocket-render`: procedural meshes for nose/body/fin/transition; orbit camera; picking.
- Component tree + properties panel; live mass/CG/CP readouts (geometric CP via simple Barrowman for cylinders+nose+fin only — enough for design-time feedback).
- *Gate:* user builds a complete model rocket visually and sees live CG/CP.

### Phase 2 — Basic flight sim  *(~3 wks)*
- `rocket-sim`: RK4 fixed-step, event system, ISA atmosphere, constant-Cd aero, motor mass flow, vertical & angled 1–3 DOF trajectory.
- Simulate panel; live plots (altitude, velocity, thrust); apogee/landing readouts.
- *Gate:* predicted apogee within 5% of OpenRocket for 5 reference LPR rockets.

### Phase 3 — Full 6-DOF + extended aero  *(~7 wks)*
- Quaternion attitude; Euler equations with variable `I(t)`; full extended-Barrowman aero; US-1976 atmosphere; wind gradient; recovery (drogue/main).
- 3D flight playback; linked plots; stability-margin-vs-time.
- *Gate:* match RASAero CP within 0.5 cal and Cd within 10% at M=0.1, 0.8 across the validation panel; reproduce a real HPR flight's apogee within 7%.

### Phase 4 — HPR tier (compressible + staging)  *(~5 wks)*
- Prandtl-Glauert + transonic drag rise + supersonic wave drag & CP shift; clustering; staging & airstart; round-Earth gravity `g(h)`.
- Supersonic Mach coloring in playback; multi-stage UI.
- *Gate:* Cd-vs-Mach curve within the published band for a characterized supersonic rocket; staged flight apogee within 10% of flight data.

### Phase 5 — Monte Carlo + optimization  *(~4 wks)*
- `rocket-monte-carlo`: LHS + Sobol, `rayon` parallel, dispersion & percentile outputs.
- `rocket-optimizer`: CMA-ES via `argmin`; objective/constraint builder UI.
- Landing-ellipse map; optimization convergence plots.
- *Gate:* 10k-run MC of a 60 s HPR flight < 60 s wall time; optimizer recovers a known-optimal fin geometry on a synthetic problem.

### Phase 6 — Amateur tier & polish  *(ongoing)*
- NRLMSISE-00 + HWM14; J2 gravity; round rotating Earth; (optional) TVC controller hook.
- Typst-based PDF flight report; KML export; STL export.
- UI polish pass: bespoke widgets, keyboard shortcuts, undo/redo, command palette.
- *Gate:* reproduce a published sounding-rocket trajectory to agreed tolerance.

### Phase 7 — V&V hardening  *(~3 wks, concurrent with 3–6)*
- Nightly cross-tool diff suite; flight-data regression library; mutation testing on physics core; fuzz the parser and the file loader.
- Public validation report in `docs/`.

---

## 16. Risks & Mitigations

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| Transonic/supersonic aero is analytically weak; Cd at M≈1 can be off 20%+. | High | High | (1) Expose user override tables. (2) Document uncertainty bands in UI. (3) Plan an optional CFD-coefficient import path. |
| NRLMSISE-00 port is large and easy to get subtly wrong. | Medium | Medium | Port the reference C impl behind a build script and diff outputs on a fixed input matrix; keep it as a test oracle. |
| egui ↔ wgpu viewport integration friction. | Medium | Medium | Validate the integration in Phase 0 spike *before* committing; fall back to iced if blocked. |
| Single-sim <5 ms target may be missed by naive aero code. | High | Medium | Cache aero coefficients vs (Mach, α) on a grid; interpolate; profile with `tracy` from Phase 2. |
| Scope creep (TVC, liquids, HITL). | High | High | Strict non-goals (§3); each addition requires an ADR. |
| Validation data is sparse for amateur tier. | Medium | High | Partner / use published NAR/Tripoli flight logs; mark T2 as "research-grade, not certified." |
| Building a CAD-quality geometry pipeline is a project in itself. | Medium | Medium | Keep v1 geometry render-only; STEP export deferred to a dedicated later phase. |

---

## 17. Testing Strategy

- **Unit tests** per module; physics targets ≥90% line coverage with `cargo-llvm-cov` in CI.
- **Property tests** (`proptest`): quaternion round-trips, frame invariance (result unchanged under launch-heading rotation), mass conservation through burn.
- **Analytical regression**: the closed-form cases in §7.9 as golden tests.
- **Mutation testing** (`cargo-mutants`) on `rocket-aero` and `rocket-sim` — the two highest-risk crates.
- **Fuzzing** (`cargo-fuzz`) on `.eng`/`.rse`/`.rocket` parsers.
- **Benchmarks** (`criterion`) gated in CI to detect performance regressions >10%.

---

## 18. Open Questions — Resolved

The seven questions from v0.1 have been researched and decided. Each becomes an ADR. Summary below; full ADRs to be filed in `docs/adr/`.

### Q1 — egui vs iced → **egui** (ADR-0002)

**Findings (verified, 2026 state):**
- egui **0.34** switched `eframe`'s default backend to **`egui-wgpu`** — wgpu is now first-class, not a bolt-on. The single biggest risk from v0.1 (viewport integration friction) is largely gone.
- A 2026 survey of Rust GUI libraries credits egui as the **first wgpu-rendered library with good accessibility support**.
- Adoption: egui ~600k all-time crates.io downloads vs iced ~180k; both healthy, egui's community larger.
- Consensus across multiple 2025–26 comparisons: egui excels for **tools and rapid iteration** (immediate mode, no state-management ceremony); iced (Elm/retained) is stronger for **structured end-user apps with heavy theming** (COSMIC desktop ships on it, though partly forked).
- Iced's cleaner architecture comes at a cost for our use case: embedding a live wgpu 3D viewport inside iced's retained widget tree and getting per-frame redraw of both is doable but more ceremony; egui's immediate mode redraws everything every frame by default, which is exactly what a real-time flight playback + scrubbing plots + 3D scene wants.

**Decision: egui (+ `egui_dock`, `egui_plot`, `egui-wgpu`).**

**Rationale specific to us:** A rocket simulator is a *tool* (design → sim → analyze loop), not a consumer app. It has a 3D viewport that must redraw every frame, plots that must scrub in sync, and a dense properties panel — all of which immediate mode handles with less code and zero state-sync bugs. The "less polished defaults" criticism of egui is real but mitigable: we ship a custom theme (§11.5) and bespoke widgets (stability gauge, CG/CP ruler, thrust-curve mini-chart), which is where the polish actually comes from anyway.

**Condition:** the Phase-0 spike still builds a 10-line docking + wgpu-viewport demo *first* to confirm `egui-wgpu` + `three-d` coexist on a single wgpu instance. If they fight over the surface, fall back to rendering the 3D scene to an offscreen texture and displaying it as an egui image — a known-good pattern.

---

### Q2 — Distribution → **Native per-OS installers + auto-update; no app store for v1** (ADR-0004)

**Analysis:**
- The app ships a wgpu binary with bundled assets and a motor cache; size will be ~30–60 MB, well within "just download it" territory.
- A single static binary (like Rust produces) is great for power users but bad for non-technical rocketry hobbyists, who expect a `.dmg` / `.msi` / `.deb`.
- App stores (Mac App Store, Microsoft Store) take 15–30%, impose sandboxing that complicates the motor-cache-on-disk model, and add a 1–5 day review latency that hurts our iterative release cadence.

**Decision:**
- **macOS**: notarized `.dmg` (built via `cargo-bundle` + `xcrun notarytool`).
- **Windows**: signed `.msi` via `cargo-wix`.
- **Linux**: `.deb` + `.rpm` + a static binary tarball; publish a Flatpak to Flathub as the "store" path (no revenue share, no sandbox surprise).
- **Updates**: `self_update` crate polling a release JSON on GitHub Releases; user-controlled (check + prompt), never silent. Download deltas via zsync-style patching later.
- **Signing certs**: Apple Developer ID (~$99/yr) and a Windows code-signing cert (~$200–400/yr) are real budget items — list them as project costs.

**Deferred:** App store submission (revisit if a mobile companion is ever built).

---

### Q3 — License → **MPL-2.0** (ADR-0005)

**Analysis:**
- **GPL-3.0** (OpenRocket's choice) maximally protects the project but **blocks reuse** of the physics core in commercial tools, flight controllers, and embedded avionics — exactly the ecosystem we want to seed. It also means anyone embedding `rocket-physics` must open-source their whole app.
- **MIT/Apache-2.0** maximally enables adoption but allows a competitor to fork the whole GUI, rebrand, and ship a closed version with no contribution back.
- **MPL-2.0** is the middle path: file-level copyleft. Anyone can use the crates as libraries in a closed app (like MIT/Apache), but any *modifications to our source files* must be published back. This protects the project from embrace-extend-rebrand while still letting a flight-controller firmware link `rocket-physics` without going open-source.
- MPL-2.0 is also **GPL-compatible** and **viral at file scope only**, which is intuitive to explain to contributors ("edit our files → share your edits; link our crates → do what you want").

**Decision: MPL-2.0** for all crates in the workspace.

**Consequence:** we **cannot copy code from OpenRocket** (GPL-3.0) into our tree — clean-room reimplementation only. This was already the plan (we reimplement Barrowman from the original SP-7224 report and the literature, not from OpenRocket's Java). We *can* use OpenRocket's motor database repo (GPL) only if we treat it as data fetched via the ThrustCurve API (see Q4), not as bundled source.

---

### Q4 — Motor database licensing → **Fetch-on-first-run via the ThrustCurve.org API; never bundle** (ADR-0006)

**Findings (verified from ThrustCurve.org's own pages):**
- Each motor file on ThrustCurve.org carries its **own per-file license**: *Public Domain*, *Free Usage* (GPL/CC/Apache), *Other (restricted)*, or *Unknown*. There is **no blanket license** that lets us redistribute the whole catalog.
- The site copyright notice explicitly reserves all rights for the *site/UI*; the *data files* are governed by their per-file tags, many of which are "Unknown" — i.e., legally unusable for redistribution.
- John Coker (site maintainer) provides a **sanctioned JSON/XML API** (`/api/v1/search.json`, `/api/v1/download.json`) explicitly intended for simulators to fetch data programmatically. OpenRocket's own motor-database repo notes: *"If you wish to use this data for other projects, please use the ThrustCurve.org API directly."*
- NRLMSISE-00 C source, by contrast, is **public domain** — no such constraint on the atmosphere model (relevant to Q4-adjacent: we can port/bind it freely).

**Decision:**
- Ship the app with **no motor files bundled**. On first run (or on demand), the motor browser queries the ThrustCurve API, and selected motors are downloaded as `.eng`/`.rse` into a user-writable cache (`~/.cache/aether/motors/` or `%LOCALAPPDATA%`).
- The cache stores the per-file license tag alongside each file; "Unknown" or "restricted" files are still usable *locally* (the user fetched them) but the app never re-distributes them.
- A small **curated starter set** of ~20 classic motors (Estes A8, B6, C6; Aerotech F/G/H samples) will be hand-verified as Public Domain or explicitly permission-gotten from John Coker, and *those* can ship in-repo under their per-file terms. This avoids the "empty on first launch" problem for offline users.
- Custom motors (user-entered thrust curves) and imported files always work offline.

**Engineering impact:** `rocket-motors` needs an HTTP client (`reqwest`, `tokio`) and an API client module — modest scope, adds a Phase-2 task. A throttling/retry layer is polite to a free service.

---

### Q5 — Tier gating in UI → **Progressive disclosure, tier auto-suggested** (ADR-0007)

**Analysis:**
- Showing T0/T1/T2 controls always violates "not amateurish" in the opposite direction: it overwhelms a Cub Scout with NRLMSISE-00 solar-flux inputs.
- Hiding T2 behind a settings toggle is better but creates a "where's the feature?" discovery problem.
- Auto-detection is the elegant middle: the simulator inspects the rocket + environment (altitude ceiling from motor + drag estimate, peak Mach from impulse/mass) and **suggests a tier**, with the user able to override via a small "Advanced" disclosure.

**Decision:**
- Default tier is **auto**: estimate apogee and Mach cheaply (1-DOF, constant Cd); pick T0 if apogee < 5 km and Mach < 0.6, T1 if < 30 km and < Mach 3, else T2.
- The tier selector is a 4-way segmented control in the Simulate panel: **Auto · T0 · T1 · T2**. Hovering each shows a one-line description and the *reason* Auto picked what it did ("Auto: T1 — estimated apogee 8.2 km, peak Mach 0.94").
- Tier-relevant inputs (solar F10.7, geomagnetic Ap, latitude/longitude for Coriolis) live behind an "Advanced environment" expander that only un-grayes when T2 (or Auto→T2) is active.
- A one-time first-run toast explains the tier system in two sentences with a "learn more" link to the docs.

**Engineering impact:** a ~30-line `tier::suggest(rocket, env) -> Tier` function in `rocket-sim`; trivial. Pays for itself in UX.

---

### Q6 — Collaboration → **v1 is local-only; design the file format for future sync, build no sync now** (ADR-0008)

**Analysis:**
- A sync/collaboration layer (accounts, auth, CRDT or server-authoritative merge, conflict resolution) is a 3–6 month project unto itself and would dominate v1 scope.
- But rocketry clubs *do* share designs, and "email the `.rocket` file" works fine today. Real-time multi-user editing is a "nice to have," not a "must have," for our audience.
- The risk is *format lock-in*: if we build a file format that can't later support collaboration, we pay a migration cost.

**Decision:**
- v1: **local files only**. Designs are `.rocket` files (JSON, §13); users share them however they like (email, Git, Dropbox — OpenRocket users already do exactly this with `.ork`).
- **Design the format to be sync-friendly from day one**: stable component IDs (already planned), no positional references (use IDs), operations expressible as patches (we'll keep an internal `Edit` enum even if the UI only applies them directly now). This costs ~nothing now and saves a future migration.
- A "Compare designs" view (diff two `.rocket` files) is a cheap v1.x feature that clubs will love and that doubles as the foundation for any future merge logic.
- **Deferred to v2.x if demand appears**: a Git-based collaboration flow (designs as a repo, pull-request-style review) is the lightest-weight real-collab option and fits the format we're building. A hosted sync server is the heaviest; only if a clear paying audience emerges.

---

### Q7 — Plugin / scripting → **Rust API + CLI now; Rhai scripting in v1.x; no native-plugin FFI** (ADR-0009)

**Analysis:**
- A custom event/controller layer (e.g., "ignite stage 2 when acceleration > X *and* altitude > Y", or a TVC controller) is the #1 thing power users will want that the built-in event system (§7.6) can't anticipate.
- Options:
  - **Lua** — familiar, fast (`mlua`), but adds a runtime and a GC.
  - **Rhai** — pure Rust, no GC, sandboxed by default, smaller; slightly slower and less known. Excellent fit for *configuration/scripting* (not hot loops).
  - **Python (PyO3)** — maximum reach, but embedding Python is heavy and the GIL is a pain for a parallel Monte Carlo host.
  - **Native plugins (FFI/WASM)** — most flexible, most complexity, biggest security surface.
- Hot-path code (the flight loop) must stay in Rust for the <5 ms/sim budget; scripting is for *event handlers* and *objective functions* (called a handful of times per sim, not per step), where Rhai's overhead is invisible.

**Decision:**
- **Now (v1.0):** expose the physics core as a **Rust library API** and a **CLI** (`aether run design.rocket --env preset.json --tier auto --runs 1`). This covers batch workflows, CI, and is the substrate for everything else.
- **v1.x:** add **Rhai** scripting for two narrow, well-defined hooks:
  1. **Custom events** — a Rhai function `should_fire(state, events_fired) -> bool` registered against a named event slot, used in place of the built-in triggers.
  2. **Optimizer objectives** — a Rhai function `score(result) -> float` for the CMA-ES objective, so users can optimize for anything (e.g., "minimize cost per meter of apogee").
- **Never (for foreseeable scope):** native dynamic-library plugins. The security and stability surface isn't worth it; Rhai's sandbox plus a future WASM sandbox (if truly needed) covers the real use cases.
- The Rhai API surface is kept tiny and versioned (`aether::script::v1`) so we can evolve it without breaking users.

---

## 19. Immediate Next Steps

> **Phase-0 execution plan:** see [`rocket-simulator-phase0-spike.md`](./rocket-simulator-phase0-spike.md) — five tasks with acceptance criteria, go/no-go gates, and a SPIKE_REPORT template.

1. ✅ Open questions resolved — file ADR-0002 through ADR-0009 in `docs/adr/` at Phase-0 start.
2. Review & approve this plan; ADR-0001 (overall architecture) is the umbrella.
3. Phase-0 spike (5–7 days): the five tasks in the spike plan. Two de-risk the architecture (egui/wgpu viewport + ThrustCurve API), three build the foundation (repo + CI, `.eng` parser, validation fixtures).
4. Lock the validation panel: pick 5–6 reference rockets with published RASAero/flight data and stage their `.eng` files and dimensions into `tests/fixtures/`.
5. Curate the ~20-motor PD-licensed starter set (Q4) — email John Coker for explicit blessing if any are ambiguous.
6. Begin Phase 0 in earnest.

---

*Document version 0.2 — open questions resolved; ready for Phase-0 spike.*
