# Phase 2 — Basic Flight Simulator — Plan & Task List

> **Current direction:** The [engineering plan](rocket-simulator-plan.md) and
> [model to flight execution plan](rocket-simulator-model-to-flight-plan.md)
> supersede this document's priority order, estimates and release sequence.
> Retain these tasks as technical history; phase completion does not establish
> the current validation or model-to-flight gates.

**Status:** Historical phase task inventory; current sequencing is M0–M5
**Estimated duration:** 3 weeks (one focused engineer)
**Goal:** A user can run a flight simulation of their rocket design and see the trajectory (altitude, velocity, thrust) plotted in real time, with apogee/landing readouts.

---

## Phase 2 goal & gate

Per the plan (§15), Phase 2's gate is:

> *Predicted apogee within 5% of OpenRocket for 5 reference LPR rockets.*

Concretely, by the end of Phase 2, a user can:

1. Select a motor for their rocket design (from the cached ThrustCurve.org library).
2. Configure the simulation environment (launch angle, pad altitude, wind).
3. Run the simulation and watch the trajectory plot in real time.
4. See apogee, max velocity, max acceleration, landing time, and landing position.
5. Compare the predicted apogee against OpenRocket for reference rockets.

This phase delivers the "simulate" half of the "design → simulate → analyze" loop. It does **not** include 6-DOF attitude dynamics (Phase 3), full-fidelity aerodynamics (Phase 3), Monte Carlo (Phase 5), or recovery events (Phase 3).

## Scope decision: 3-DOF, not 1-DOF

The plan mentions "1–3 DOF." We implement **3-DOF translational dynamics** (x, y, z position + velocity), not 1-DOF (vertical-only). Reasons:

- 1-DOF can't predict landing position (drift), which is a key user question.
- 3-DOF is barely more code than 1-DOF (just `v_x, v_y` integration + wind vector).
- Phase 3's 6-DOF will build on this translational core.
- The 5% apogee gate is achievable with 3-DOF + constant Cd.

We do **not** implement attitude (quaternions, angular velocity) — the rocket flies "perfectly straight" along its body axis, which is a valid approximation for fin-stabilized rockets at small angle of attack.

## Prerequisites (from Phase 1)

All met:

- ✅ Component data model (`rocket-core`)
- ✅ Design-time aero (gives us `C_Nα` and `x_cp` for stability, but Phase 2 needs **drag Cd** — see Task 2.2)
- ✅ Motor parser + ThrustCurve API (`rocket-motors`)
- ✅ Design editor UI (`rocket-ui`)
- ✅ Estes Alpha fixture builds correctly

---

## Tasks

### Task 2.1 — Atmosphere model (1 day)

**Goal:** Implement the ISA (International Standard Atmosphere) model for T0.

**Deliverables:**

- [ ] `rocket-physics::atmosphere::isa` module.
- [ ] `IsaAtmosphere::at(altitude: Length) -> AtmosphereState` returning:
  - Temperature (K)
  - Pressure (Pa)
  - Density (kg/m³)
  - Speed of sound (m/s)
- [ ] Sea-level constants: T₀ = 288.15 K, P₀ = 101325 Pa, lapse rate L = 0.0065 K/m (troposphere, valid to 11 km).
- [ ] Hydrostatic pressure: `P(h) = P₀ · (1 − L·h/T₀)^(g·M/(R·L))`
- [ ] Density: `ρ(h) = P(h) / (R_specific · T(h))`
- [ ] Speed of sound: `a(h) = √(γ · R · T(h))`
- [ ] Unit tests: sea-level values match ISA standard (T=288.15K, P=101325Pa, ρ=1.225 kg/m³, a=340.3 m/s); values at 1000m, 5000m match published tables.

**Acceptance criteria:**

1. Sea-level density = 1.225 kg/m³ ± 0.001.
2. Pressure at 1000m = 89875 Pa ± 10.
3. Speed of sound at 5000m = 320.5 m/s ± 0.5.

### Task 2.2 — Drag coefficient estimation (1 day)

**Goal:** A simple, constant Cd estimate for the flight sim.

Phase 1's `rocket-aero::design` gives `C_Nα` and `x_cp` (stability) but not drag. Phase 3 will implement the full drag buildup (skin friction + base + form). For Phase 2, we need a reasonable Cd to make the apogee prediction match OpenRocket within 5%.

**Deliverables:**

- [ ] `rocket-aero::drag::estimate_cd(rocket: &Rocket, mach: f64) -> f64` — a simple drag estimate.
- [ ] For now: a constant Cd per rocket, estimated from the total wetted area and a typical skin-friction coefficient, plus a base drag term.
- [ ] Cd ≈ 0.4–0.6 for a typical model rocket (matching OpenRocket's defaults for the Estes Alpha).
- [ ] A user override: if the rocket has an `overrides.cd` field, use it instead.
- [ ] Unit tests: Estes Alpha Cd in 0.3–0.7 range.

**Acceptance criteria:**

1. Estes Alpha Cd ≈ 0.45 ± 0.15.
2. Cd is constant across Mach (Phase 2 doesn't model compressibility).

### Task 2.3 — Motor mass flow & thrust curve interpolation (1 day)

**Goal:** Compute thrust and mass-vs-time from a motor's thrust curve.

**Deliverables:**

- [ ] `rocket-motors::Motor::thrust_at(time: Duration) -> Force` — linear interpolation of the thrust curve.
- [ ] `rocket-motors::Motor::mass_at(time: Duration, initial_mass: Mass) -> Mass` — mass decreases as propellant burns.
- [ ] Mass flow rate: `ṁ = T(t) / (Isp · g₀)`, integrated over time.
- [ ] `rocket-motors::Motor::propellant_mass_burned(time: Duration) -> Mass`.
- [ ] Unit tests: thrust at known times matches the curve; mass at burnout = dry mass.

**Acceptance criteria:**

1. Thrust interpolation matches data points exactly.
2. Mass at t=0 = total motor mass; mass at t=burnout = total − propellant.

### Task 2.4 — The flight simulator (3 days)

**Goal:** The core 3-DOF translational simulator with RK4 integration.

**Deliverables:**

- [ ] `rocket-sim::SimConfig`: launch angle, pad altitude, wind vector, motor selection, time step.
- [ ] `rocket-sim::SimState`: position (x, y, z), velocity (vx, vy, vz), time, mass.
- [ ] `rocket-sim::Simulator::new(rocket, motor, config) -> Self`.
- [ ] `rocket-sim::Simulator::run() -> SimResult` — runs the full flight to ground impact.
- [ ] Forces per step:
  - Thrust: along the rocket's body axis (launch angle), from motor thrust curve.
  - Gravity: `−m·g` (constant g for T0).
  - Drag: `−½ρv²·Cd·A_ref` opposite to velocity.
  - Wind: adds to the relative velocity for drag computation.
- [ ] RK4 fixed-step integration at 1 ms (0.001 s) for accuracy.
- [ ] Event detection: apogee (vz crosses zero), ground impact (z crosses 0).
- [ ] Telemetry: record (time, position, velocity, mass, thrust, drag, accel) at 10 ms intervals.
- [ ] `SimResult`: max altitude, max velocity, max acceleration, apogee time, landing time, landing position, full telemetry.
- [ ] Unit tests: vacuum vertical shot matches closed-form; energy conservation in coast.

**Acceptance criteria:**

1. Vacuum vertical shot: `z(t) = v₀t − ½gt²` matches to 0.1%.
2. With drag, apogee < vacuum apogee (drag reduces altitude).
3. Landing position is correct (z returns to 0).

### Task 2.5 — Simulate panel + plots (3 days)

**Goal:** UI for running simulations and viewing results.

**Deliverables:**

- [ ] `rocket-ui::simulate` panel:
  - Motor selector (dropdown from cached motors).
  - Launch angle slider (0–15° from vertical).
  - Pad altitude field.
  - Wind speed/direction fields.
  - "Run Simulation" button.
- [ ] `rocket-ui::plots` panel:
  - Altitude vs time plot.
  - Velocity vs time plot.
  - Thrust vs time plot.
  - Apogee / max velocity / landing readouts.
- [ ] Live telemetry streaming: the sim runs in a background thread, plots update as data arrives.
- [ ] `egui_plot` (or manual canvas drawing) for the plots.
- [ ] Integration with the existing design editor: the current rocket design is the sim input.

**Acceptance criteria:**

1. User selects a motor, clicks Run, sees the trajectory plot.
2. Apogee and landing readouts are displayed.
3. The sim completes in <1 second for a typical model rocket flight.

### Task 2.6 — Validation & gate review (2 days)

**Goal:** Validate against OpenRocket and verify the 5% apogee gate.

**Deliverables:**

- [ ] Integration tests in `aether-tests`: run the sim on the Estes Alpha with an Estes A8-3, B6-4, C6-5 motor and compare apogee to published values.
- [ ] Cross-check against OpenRocket's predicted apogee for the same configurations.
- [ ] If apogee is off by >5%, tune the Cd estimate (Task 2.2) until it matches.
- [ ] Phase-2 gate review: walk through the gate criteria.

**Acceptance criteria (Phase-2 gate):**

1. Predicted apogee within 5% of OpenRocket for the Estes Alpha on A8-3, B6-4, C6-5.
2. The sim runs in <1 second.
3. The UI shows the trajectory plot and apogee readout.

---

## Risks & mitigations

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| Cd estimate is too far off, missing the 5% gate. | High | Medium | Task 2.6 includes Cd tuning; if constant Cd can't hit 5%, add a simple Mach-dependent Cd table. |
| RK4 at 1 ms is too slow for interactive use. | Medium | Low | A 5-second flight at 1 ms = 5000 steps; each step is a few force evaluations. Should be <100 ms total. |
| Motor mass flow calculation is wrong (Isp inversion). | Medium | Medium | Task 2.3 has explicit unit tests for mass at t=0 and t=burnout. |
| egui_plot API has changed in 0.37. | Low | Medium | Fall back to manual canvas drawing if needed. |

## What's explicitly out of scope for Phase 2

- 6-DOF attitude dynamics (quaternions, angular velocity) — Phase 3.
- Compressibility / Mach-dependent Cd — Phase 3/4.
- Recovery events (parachute deployment) — Phase 3.
- Staging — Phase 4.
- Wind shear (use constant wind for now) — Phase 3.
- Monte Carlo — Phase 5.

---

## Phase 2 exit

Phase 2 is complete when:

1. ✅ All six tasks pass their acceptance criteria.
2. ✅ The Phase-2 gate is met (apogee within 5% of OpenRocket for 3 motors).
3. ✅ CI is green.

**Estimated total: 11 working days (~3 weeks).**

---

*Phase 2 plan version 1.0 — ready for execution after Phase 1.*
