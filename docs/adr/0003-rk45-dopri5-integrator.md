# ADR-0003: Dormand-Prince RK45 as the primary integrator

**Status:** Accepted
**Date:** 2026-10-18

**Extension:** [ADR-0011](0011-shared-model-and-execution-targets.md) and the [model to flight plan](../../rocket-simulator-model-to-flight-plan.md) update this decision's scope. DOPRI5 remains a planned offline integration path; the production loop currently uses RK4. A sampled controller/HIL schedule is a separate contract with explicit ticks and sample/hold ordering. GPU and real-time numerical profiles require convergence checks against the CPU reference.

## Context

The flight loop integrates a 12-dimensional state (position, velocity, unit quaternion, angular velocity) through events: motor burn, coast, staging, parachute deployment, ground impact. The integrator choice determines:

- **Accuracy** — how closely the numerical solution matches the true trajectory. Rocket flights span timescales from 0.01 s (thrust transient) to 300 s (chute descent), a 30,000:1 dynamic range. A fixed-step integrator either wastes time in the slow phases or loses accuracy in the fast ones.
- **Event detection** — apogee (`v_z = 0`), ground impact (`z = 0`), deployment altitudes (`z = h_deploy`), staging times (`t = t_stage`) are zero-crossings of scalar functions of state. The integrator must locate these precisely, not just at step boundaries, because a parachute deployed at 247 m vs 250 m changes the descent noticeably.
- **Performance** — the Monte Carlo target (10,000 runs in 60 s) implies a per-sim budget under 5 ms. A wasteful integrator makes this unreachable.
- **Conservation properties** — post-burnout, with no thrust and no aero (vacuum check), total energy and angular momentum must be conserved to high precision. This is both a physics requirement and the primary correctness diagnostic for the integrator.

The forces in tension:

1. Adaptive step size is required for accuracy across the dynamic range, but adaptive integrators cost more per step and complicate event detection.
2. Symplectic integrators (e.g., Verlet, leapfrog) conserve energy *by construction* on long Hamiltonian simulations, but they don't handle dissipative forces (aero drag) or variable mass cleanly, and they're lower-order.
3. High-order methods (8th, 10th) give better accuracy per step but have more stages per step, and the tolerance is rarely tight enough to justify them.
4. Stiff integrators (BDF, implicit Euler) are needed for systems with fast transients (e.g., a TVC controller with 100 Hz dynamics), but the rocket body itself is not stiff — applying a stiff solver everywhere wastes effort.

## Decision

The primary integrator is **Dormand-Prince RK45 (DOPRI5)** — the 7-stage, 5th-order embedded Runge-Kutta method with a 4th-order error estimate — with **event detection via dense output and Brent's method**, and **adaptive step size** via the standard controller.

Tolerances: `atol = 1e-9` (absolute, in SI units: meters, m/s, rad, rad/s), `rtol = 1e-7`.

Step-size guards:
- During motor burn: `Δt ≤ 0.01 s` (thrust curves change fast; denser points in the ENG file).
- During coast: `Δt ≤ 0.5 s`.
- During chute descent: `Δt ≤ 0.05 s`.
- Hard floor: `Δt ≥ 1e-6 s` to avoid pathological shrinkage.

A **fixed-step RK4 at 1 kHz** is used *only* for the real-time 3D playback display, replaying a precomputed trajectory. It is never used for the logged results.

For T2 phases with stiff controller dynamics (future TVC), a local switch to **implicit Euler / BDF4** is planned but out of scope for v1.

## Consequences

**Positive:**

- DOPRI5 is the standard choice for non-stiff ODEs in scientific computing, with decades of validation and well-understood behavior. Implementations are widely available; our hand-rolled version will be small (~200 lines) and well-tested.
- The embedded 4th-order error estimate gives adaptive step control for free — no extra function evaluations beyond the 7 stages.
- DOPRI5 provides **dense output** (a 4th-order interpolant) at no extra cost, which we use for both event detection and smooth 3D playback at arbitrary timestamps.
- Event detection via Brent's method on the dense output is precise to a tolerance tighter than the step size, so parachute deployments and staging events land where they should.
- The 5th-order accuracy is more than sufficient for our tolerances — at `rtol=1e-7`, the per-step local error is ~`1e-12` on a 60 s flight, giving accumulated global error well below 1 mm in position.
- Energy and angular momentum conservation (the §17 checks in the aero-dynamics doc) will be satisfied to `rtol=1e-7` post-burnout, which is the primary correctness diagnostic.

**Negative:**

- DOPRI5 is **not symplectic** — on pure Hamiltonian systems (vacuum, no dissipation) it leaks energy at a rate proportional to `rtol`. For our use case this is fine: real flights have aero dissipation dominating any integrator leak, and the conservation checks are run at `rtol=1e-7` where the leak is negligible over a 300 s flight. But for an (out-of-scope) orbital propagation use case, we would need a symplectic method.
- Adaptive step size complicates reproducibility of *trajectories at specific times* — the integrator chooses its own time points. We mitigate by interpolating to a fixed output grid via the dense output, so logged results are on a regular time grid regardless of the integrator's internal steps.
- 7 stages per step × 12 state dims = 84 function evaluations per step, each requiring an aero lookup. The aero cache (aero-dynamics doc §7) is what makes this affordable — without it, DOPRI5 would be too slow for the Monte Carlo budget.
- Implementing event detection correctly (bracketing the zero, handling simultaneous events, handling events that trigger configuration changes mid-step) is fiddly. We will write extensive tests for the event system specifically.

**Neutral:**

- The fixed-step RK4 for playback is a separate code path. It must be kept in sync with the DOPRI5 implementation's force computations, or the playback will visually diverge from the logged trajectory. We mitigate by sharing the force-assembly code (the same `compute_forces(state, env)` function) between both integrators; only the time-stepping differs.

## Alternatives considered

### Fixed-step RK4 everywhere

Simpler to implement, no adaptive-step machinery, no dense output for events. Rejected because:

- To match DOPRI5's accuracy at `rtol=1e-7`, a fixed-step RK4 would need `Δt ≈ 1e-3 s` everywhere, giving 60,000 steps for a 60 s flight vs DOPRI5's ~2,000. The per-sim cost would blow the Monte Carlo budget by 30×.
- Event detection at fixed step boundaries is imprecise — a parachute deployed at the nearest 1 ms boundary is fine, but at a 10 ms boundary the deployment altitude error is ~1 m, which compounds into descent-rate error.

### Symplectic integrator (Verlet / leapfrog)

Energy-conserving by construction on Hamiltonian systems. Rejected because:

- Rocket flight is **not Hamiltonian**: there's thrust (time-varying, non-conservative), aero drag (dissipative), and variable mass. Symplectic methods don't handle these cleanly — the symplectic property is lost the moment you add non-conservative forces, at which point you're using a symplectic method for no benefit.
- Symplectic methods are typically 2nd-order; reaching 5th-order accuracy requires composition (Yoshida), which multiplies the per-step cost.
- The conservation we care about is "don't leak energy over a 300 s flight," which DOPRI5 at `rtol=1e-7` satisfies. The symplectic advantage (conservation over millions of orbits) is irrelevant for our timescales.

### Implicit/stiff integrator (BDF4, Radau)

Required if the system has fast transients (e.g., a 100 Hz TVC controller with 1 ms dynamics). Rejected for v1 because:

- The rocket body itself is not stiff — the fastest dynamics are pitch oscillations at ~5–10 Hz, which DOPRI5 handles with `Δt ≈ 0.01 s`.
- Implicit methods require solving a nonlinear system at each step (Newton iteration with a Jacobian), which is expensive and requires Jacobian computation (autodiff or finite differences) for our force assembly.
- We will revisit if/when TVC controllers are added (T2 phase); the architecture allows a local switch to BDF in specific phases without rewriting the whole pipeline.

### High-order methods (DOP853, 8th-order)

Higher accuracy per step, fewer steps for the same tolerance. Rejected because:

- At `rtol=1e-7`, DOP853's advantage over DOPRI5 is marginal — both reach the tolerance in a reasonable step count.
- DOP853 has 12 stages per step vs DOPRI5's 7, so each step is more expensive. The crossover where DOP853 wins is around `rtol=1e-10`, tighter than we need.
- DOPRI5's broader implementation history and validation literature make it the safer choice.

### Off-the-shelf ODE solver crate (`nalgebra-ode`, `ode-solvers`)

Considered. These provide DOPRI5 and similar methods out of the box. Rejected for the primary path because:

- Our event detection requirements (configuration-changing events mid-step, like staging and parachute deployment) are not well-served by generic solvers, which typically support only simple zero-crossing detection.
- We need tight integration with the aero cache (forcing a cache lookup per stage, not per step) and the force-assembly code, which is easier with a hand-rolled integrator.
- The hand-rolled DOPRI5 is ~200 lines and well-tested; the dependency cost of an external crate isn't justified.

We may still use `ode-solvers` as a reference implementation for cross-validation in tests.

## References

- Aero/dynamics spec: `rocket-simulator-aero-dynamics.md` §16 (integrator), §17 (conservation checks).
- Plan: `rocket-simulator-plan.md` §7.8.
- Hairer, Nørsett, Wanner, *Solving Ordinary Differential Equations I* (the DOPRI5 reference).
- Dormand & Prince, "A family of embedded Runge-Kutta formulae" (1980).
