# Phase 3 — Full 6-DOF + Extended Aero — Plan & Task List

**Status:** Ready for execution
**Estimated duration:** 7 weeks (one focused engineer)
**Goal:** Replace the point-mass Phase 2 sim with a full 6-DOF rigid-body sim with quaternion attitude, Euler-equation rotational dynamics, extended-Barrowman aero (subsonic + compressibility), US-1976 atmosphere, wind gradient, and recovery events. Validate against RASAero and a real HPR flight.

---

## Phase 3 goal & gate

Per the plan (§15), Phase 3's gate is:

> *Match RASAero CP within 0.5 cal and Cd within 10% at M=0.1, 0.8 across the validation panel; reproduce a real HPR flight's apogee within 7%.*

Concretely, by the end of Phase 3, a user can:

1. Run a full 6-DOF simulation that models attitude (pitch, yaw, roll) as well as translation.
2. See the rocket's 3D attitude in the playback view (it pitches over, weathervanes in wind, oscillates at apogee).
3. Use the extended-Barrowman aero model with compressibility corrections (Prandtl-Glauert up to M≈0.8, transonic drag rise).
4. Use the US-1976 atmosphere up to the stratopause (~50 km).
5. Simulate recovery: drogue chute at apogee, main chute at a configurable altitude.
6. Validate the aero coefficients against RASAero for the validation panel.

## Scope decisions

### Quaternion attitude, not Euler angles

Per the spec §10.1, we use a 4-component unit quaternion `q ∈ S³` for attitude, not a 3-parameter Euler representation. Quaternions have no singularities (gimbal lock) and integrate cleanly. The state vector is 13-dimensional: `[x(3), v(3), q(4), ω(3)]`.

### Diagonal inertia tensor

For an axially symmetric rocket (the vast majority of designs), the inertia tensor is `diag(I_axial, I_t, I_t)` in body principal axes. This decouples roll from pitch/yaw and avoids a 3×3 linear solve. Phase 4 can add full 3×3 inertia for asymmetric configs (pods, clustered motors).

### Prandtl-Glauert + transonic, not full supersonic

Phase 3 covers **subsonic + transonic** (M ≤ ~1.2) aero. Full supersonic wave drag (linearized / Newtonian / shock-expansion) is Phase 4. The Prandtl-Glauert correction `β = √(1−M²)` handles M ≤ 0.7 cleanly; above that we use an empirical transonic drag-rise model. This is sufficient for the gate's M=0.1 and M=0.8 validation points.

### US-1976 atmosphere (troposphere + stratosphere)

Phase 2's ISA model is valid to 11 km. Phase 3 extends this to 50 km (the stratopause) using the US-1976 standard atmosphere, which has piecewise-constant lapse rates in each layer. T2 (Karman line, 100 km) requires NRLMSISE-00 and is Phase 4.

### Recovery events

Implement a simple recovery event system: deploy drogue at apogee, deploy main at a user-configurable altitude (default 300 m AGL). Each chute adds a `Cd·A` drag term. This enables realistic descent-rate prediction — a major user question.

## Prerequisites (from Phase 2)

All met:

- ✅ ISA atmosphere model (`rocket-physics::atmosphere`)
- ✅ Drag coefficient estimate (`rocket-aero::drag`)
- ✅ Motor thrust/mass flow (`rocket-motors::rasp::Motor`)
- ✅ 3-DOF point-mass sim (`rocket-sim`)
- ✅ Simulate UI panel with plots

---

## Tasks

### Task 3.1 — Frames & quaternions (2 days)

**Goal:** A `rocket-physics::frames` module with the quaternion type, rotation matrices, and frame transformations. No physics yet — just the math, heavily tested.

**Deliverables:**

- [ ] `Quaternion` type (w, x, y, z) with `from_axis_angle`, `from_euler`, `to_rotation_matrix`, `normalize`, `conjugate`, `multiply`.
- [ ] `Vec3` type (or reuse `nalgebra::Vector3`) with cross/dot/rotate.
- [ ] `Rotation3` (3×3 matrix) with `from_quaternion`, `transpose`, `multiply_vec`.
- [ ] Frame transformations: launch ↔ body, body ↔ wind.
- [ ] Angle-of-attack / sideslip computation from velocity and attitude.
- [ ] Tests: round-trip `q ↔ R`, quaternion multiplication identity, steady-spin closed-form `q(t) = q(0) ⊗ exp(½ωt)`.

**Acceptance criteria:**

1. `|q| = 1` after `from_axis_angle` and after `multiply` + `normalize`.
2. `R(q)·R(q)⁻¹ = I`.
3. Steady-spin test: integrate `q̇ = ½·q⊗ω` for 1 s with constant `ω`, compare to closed-form to 1e-9.

### Task 3.2 — US-1976 atmosphere (1 day)

**Goal:** Extend the atmosphere model to 50 km with the US-1976 piecewise-lapse-rate layers.

**Deliverables:**

- [ ] `rocket-physics::atmosphere::us1976` module.
- [ ] Layers: troposphere (0–11 km, L=−6.5 K/km), stratosphere lower (11–20 km, isothermal), stratosphere mid (20–32 km, L=+1.0 K/km), stratosphere upper (32–47 km, L=+2.8 K/km), stratopause (47–50 km, isothermal).
- [ ] `Us1976::at(altitude)` returning `AtmosphereState` (same struct as ISA).
- [ ] Backward compatibility: `IsaAtmosphere` becomes a thin wrapper around `Us1976` for `h ≤ 11 km`.
- [ ] Tests: values at 11, 20, 32, 47, 50 km match published US-1976 tables.

**Acceptance criteria:**

1. Troposphere values match Phase 2 ISA (regression).
2. Density at 20 km = 0.0889 kg/m³ ± 0.001.
3. Density at 50 km = 0.00103 kg/m³ ± 0.0001.

### Task 3.3 — Extended Barrowman aero (3 days)

**Goal:** Full subsonic aero buildup with all components and dynamic derivatives.

**Deliverables:**

- [ ] `rocket-aero::buildup` module computing `AeroCoefficients { c_a, c_n, c_m, c_y, c_l, c_n_yaw, c_m_pitch_rate, c_roll_rate, ... }` at a given `(α, β, Mach, Re)`.
- [ ] Nose cone: `C_Nα = 2`, CP per shape (from Phase 1 design estimate, now as a function of α).
- [ ] Body tubes: slender-body `C_Nα` (nonzero in extended Barrowman, unlike the classic).
- [ ] Fin set: full Barrowman formula with fin-body interference, sweep, aspect ratio.
- [ ] Transitions: conical-frustrum contribution (sign-correct for boattails).
- [ ] Compressibility: Prandtl-Glauert `β = √(1−M²)` correction to `C_Nα` and CP for M ≤ 0.7. Empirical transonic rise for 0.7 < M < 1.2.
- [ ] Dynamic derivatives: pitch damping `C_m_q + C_m_α̇`, roll damping `C_l_p`.
- [ ] Aero cache: `AeroCache` keyed by `(Mach, α)` with linear interpolation, evicted LRU.
- [ ] Tests: SP-7224 validation cases (rows 1–3), CP monotonicity, Cd-vs-Mach shape.

**Acceptance criteria:**

1. CP at M=0.1 matches Phase 1 design estimate (regression).
2. CP shifts aft with Mach (Prandtl-Glauert moves CP forward; fins dominate).
3. `C_Nα` increases by factor `1/β` from M=0 to M=0.7.
4. Cd rises sharply between M=0.8 and M=1.0 (transonic drag rise).

### Task 3.4 — 6-DOF dynamics (4 days)

**Goal:** The full 6-DOF flight simulator with quaternion attitude and Euler-equation rotation.

**Deliverables:**

- [ ] `rocket-sim::state6dof` module with `SimState6Dof { position, velocity, quaternion, angular_velocity, time, mass }`.
- [ ] Force & moment assembly per spec §14: thrust (along body axis), aero (in wind frame, rotated to body), gravity (in launch frame).
- [ ] Euler equations `I·ω̇ + ω × (I·ω) = M` for diagonal inertia.
- [ ] Quaternion kinematics `q̇ = ½·q⊗ω`.
- [ ] Mass properties vs time: `m(t)`, `x_cg(t)`, `I(t)` from the motor schedule (derived, not integrated — spec §15.3).
- [ ] RK4 integration of the 13-dim state (Phase 3.7 will switch to DOPRI5).
- [ ] Conservation checks: quaternion norm, angular momentum, energy (in CI).
- [ ] Tests: vacuum vertical shot, vacuum spinning body, ballistic-dart symmetry.

**Acceptance criteria:**

1. Vacuum vertical shot: `z(t)` matches closed-form to 1e-7 m.
2. Steady-spin: `|ω|` constant to 1e-9, `q(t)` matches closed-form.
3. Ballistic dart (symmetric, zero AoA): zero lateral drift at impact.
4. Angular momentum conserved in vacuum coast to rtol=1e-6.

### Task 3.5 — Recovery events (2 days)

**Goal:** Parachute deployment with two-stage recovery (drogue + main).

**Deliverables:**

- [ ] `rocket-core::stage::RecoveryEvent { trigger, altitude_agl, chute: ChuteSpec }`.
- [ ] `ChuteSpec { cd, diameter, name }` — a parachute with a drag area.
- [ ] Event detection in the sim: deploy drogue at apogee (`vz` crosses zero), deploy main at `h_agl < h_main`.
- [ ] Post-deployment dynamics: add `½·ρ·v²·Cd·A_chute` drag opposing velocity, with a 0.5 s inflation transient.
- [ ] UI: recovery config panel (main chute altitude, chute sizes).
- [ ] Tests: descent rate under chute matches `v = √(2mg/(ρ·Cd·A))`, drogue-then-main sequence.

**Acceptance criteria:**

1. Drogue deploys within 0.1 s of apogee.
2. Main deploys within 0.5 s of crossing `h_main`.
3. Descent rate under main ≈ 5 m/s for a typical LPR (matches real chutes).

### Task 3.6 — 6-DOF UI: attitude playback + recovery config (2 days)

**Goal:** UI updates to show 6-DOF results and configure recovery.

**Deliverables:**

- [ ] 3D playback: scrub a time slider, see the rocket's attitude at each point (rotate the 3D model by `q(t)`).
- [ ] Recovery config panel: chute sizes, deployment altitude.
- [ ] Updated plots: add `α` (AoA) vs time, roll rate vs time.
- [ ] Stability-margin-vs-time plot (CP and CG both move during flight).
- [ ] Tests: smoke test that the playback slider renders.

**Acceptance criteria:**

1. User can scrub the 3D playback and see the rocket pitch over.
2. Recovery config saves to `.rocket` file.
3. AoA plot shows the post-burnout oscillation.

### Task 3.7 — DOPRI5 adaptive integrator (2 days)

**Goal:** Replace fixed-step RK4 with adaptive Dormand-Prince RK45 for the 6-DOF sim.

**Deliverables:**

- [ ] `rocket-sim::integrator::dopri5` module.
- [ ] 7-stage DOPRI5 with embedded 4th-order error estimate.
- [ ] Adaptive step controller: `Δt_new = Δt · clamp(0.9·(tol/err)^(1/5), 0.2, 5.0)`.
- [ ] Dense output (4th-order interpolant) for event detection.
- [ ] Step-size guards from spec §16.3 (≤0.01 s during burn, ≤0.5 s coast, ≤0.05 s chute).
- [ ] Benchmarks: DOPRI5 vs RK4 on a 10-s flight — DOPRI5 should be ~3× faster for the same accuracy.
- [ ] Tests: same conservation checks as RK4 but to tighter tolerances.

**Acceptance criteria:**

1. Same vacuum vertical shot, tighter tolerance (1e-9 m).
2. DOPRI5 uses ≤500 steps for a 10-s LPR flight (vs 10000 for RK4 at 1 ms).
3. Conservation checks pass at rtol=1e-9.

### Task 3.8 — Validation & gate review (3 days)

**Goal:** Validate against RASAero and a real HPR flight; verify the Phase 3 gate.

**Deliverables:**

- [ ] RASAero CP comparison: build the validation panel rockets, compute CP at M=0.1 and M=0.8, compare to RASAero output.
- [ ] Cd-vs-Mach curve for the Estes Alpha: compare to OpenRocket/RASAero at M=0.1, 0.3, 0.5, 0.8.
- [ ] HPR flight reproduction: find a published HPR flight log (apogee, max velocity, descent rate) and match the sim to it within 7%.
- [ ] Phase-3 gate review.

**Acceptance criteria (Phase-3 gate):**

1. CP within 0.5 cal of RASAero at M=0.1 and M=0.8 for 3+ validation rockets.
2. Cd within 10% of RASAero at M=0.1 and M=0.8.
3. HPR flight apogee within 7%.
4. 6-DOF attitude oscillation frequency within 5% of the analytical damped-pendulum model.
5. Recovery descent rate within 10% of `√(2mg/(ρ·Cd·A))`.

---

## Risks & mitigations

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| Quaternion sign conventions are wrong. | High | High | The ballistic-dart symmetry test (spec §17.3) catches this immediately. Write it early. |
| Prandtl-Glauert CP shift is wrong direction. | Medium | Medium | Compare to RASAero at M=0.5 for the validation panel; fix the sign if CP moves the wrong way. |
| DOPRI5 is slower than RK4 in practice. | Medium | Low | Fall back to RK4 at 1 ms if DOPRI5 doesn't show a 3× speedup. The accuracy gain is the main reason for the switch. |
| HPR flight data is hard to find. | Medium | Medium | Use a ThrustCurve.org flight log or a published NAR research report. If none available, use OpenRocket as the reference instead. |
| Recovery inflation transient is unphysical. | Low | Medium | Use a simple exponential inflation `Cd(t) = Cd·(1−exp(−t/τ))` with τ=0.3 s; it's a reasonable approximation. |

## What's explicitly out of scope for Phase 3

- Full supersonic wave drag (M > 1.2) — Phase 4.
- Staging and clustering — Phase 4.
- J2 oblateness / round-Earth — Phase 4 (T2).
- NRLMSISE-00 atmosphere — Phase 4 (T2).
- Wind shear (use constant wind + gradient, not full shear model) — Phase 4.
- Monte Carlo — Phase 5.
- CMA-ES optimization — Phase 5.

---

## Phase 3 exit

Phase 3 is complete when:

1. ✅ All eight tasks pass their acceptance criteria.
2. ✅ The Phase-3 gate is met (RASAero CP/Cd match, HPR apogee within 7%).
3. ✅ CI is green.

**Estimated total: 21 working days (~7 weeks).**

---

*Phase 3 plan version 1.0 — ready for execution after Phase 2.*
