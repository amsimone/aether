# ADR-0011: Shared model and execution targets

**Status:** Accepted for architecture and delivery direction; backend selection remains gated by the M1 spike

**Date:** 2026-10-18

## Context

Aether already has a Rust vehicle model, a 6-DOF host simulator, saved flight
cases/runs, UI/CLI execution, and an offline CFD adapter. Professional control
development also needs consistent equations and interfaces through controller
generation, software-in-the-loop (SIL), hardware-in-the-loop (HIL), and flight
reconciliation. Re-entering parameters or translating units, frames, clocks and
state semantics between those stages creates an additional source of defects.

A parameter file alone cannot generate equivalent executable models. The system
needs an explicit plant/controller distinction, a portable operation subset,
sampled execution semantics, observation models, and evidence tying generated
artifacts to their source. Existing mass/inertia and aerodynamic findings must
be resolved before new analysis functions support engineering accuracy claims.

## Decision

Adopt the [model to flight execution plan](../../rocket-simulator-model-to-flight-plan.md)
and its M0–M5 gates as the current delivery sequence. Retain the Rust workspace,
native editor, local persistence, shared GUI/CLI execution and evidence inventory.
Evaluate Julia or another existing generation tool through a bounded spike;
do not maintain two independently authored production physics models.

A versioned source package contains vehicle/plant definitions, a distinct
controller graph, sensor/actuator definitions, experiment/scheduling settings,
and observation/replay mappings. Resolve it into a restricted typed IR and
schedule with source diagnostics. Generate only supported target artifacts:

- CPU plant/controller evaluation and smooth local linearizations;
- standalone controller C with explicit state/reset/step semantics;
- an eligible GPU ensemble kernel with a paired CPU reference;
- a host plant/HIL interface connected to the actual board controller;
- observation mappings and reconciliation reports for immutable measured logs.

Version primitives, units, frames, rates, numeric profiles, derivative/domain
support and target capabilities. Opaque Rust/external models cannot implicitly
participate in differentiation or GPU generation. Unsupported combinations must
produce diagnostics rather than silently changed equations or backend behavior.

Keep model/physics evaluation independent of UI, transport, wall clock and
filesystem. Host schedulers/adapters supply clocks, packets and artifacts.
Controller code does not contain a simulated rocket plant; the initial HIL
plant executes on the host. CFD stays an offline coefficient/evidence producer.

Each generated artifact has a receipt linking model/graph/schema, compiler,
options, target precision, ABI/protocol and content hashes. Tests establish
controller semantic parity, physical model verification, CPU/GPU agreement,
and measured HIL timing separately. Calibration creates a proposed model
revision and preserves preflight predictions, raw logs and held-out evaluation.

Begin with one subsonic rigid rocket, one sampled controller, one recorded-log
adapter and one board. Add a pilot-selected C ABI/Trick or FMI external adapter
after the own-host workflow works. Vendor compatibility requires a versioned
integration test, not a file-extension or ABI claim.

## Consequences

The existing editor and run artifacts remain useful while the core grows toward
controller development and post-flight review. CPU simulation, generated C,
ensembles and replay can share signal/state semantics and source identity.
Small-launcher and university pilots get a concrete workflow to evaluate.

The new boundary adds compiler, scheduler, numeric, hardware and data-alignment
verification work. Generic model-language design, all-device support and hard
real-time guarantees are outside the first increment. GPU support is conditional
on a named subset, correctness checks and useful measured acceleration.

Extract model/control/analysis/codegen/HIL/replay crates only after modules and
examples establish their dependencies. A Rust C ABI may require a narrowly
reviewed exception to the unsafe-code policy; portable C generation does not
require loosening that policy across the workspace.

## Relationship to existing decisions

This extends ADR-0001's layering and changes the master plan's previous
hobby-first tier sequence. ADR-0003's offline integrator choice does not define
controller ticks or HIL timing. ADR-0008's local-file approach remains, extended
through explicit migrations and evidence bundles.

ADR-0009's restriction on arbitrary in-process native plugins remains. Generated
controller source, a separately hosted C-callable plant, and optional isolated
analysis/build tools are distinct export/integration boundaries. Rhai remains
an optional orchestration mechanism; it does not define portable controller
execution semantics. No runtime plugin loader is authorized by this ADR.

## Alternatives considered

Continue expanding rocket physics tiers and defer control/HIL/replay. This
retains the earlier roadmap but delays the professional workflow that now sets
product priorities; needed physical corrections remain in M0 instead.

Rewrite the application in Julia immediately. Julia has relevant analysis and
GPU infrastructure, but a rewrite would abandon working integration before the
shared-model and deployment questions are tested. Evaluate artifact generation
and interoperability first.

Keep handwritten CPU, GPU, controller and notebook implementations synchronized
through parameter files alone. This leaves equations, update order and numeric
semantics outside the common definition and increases ongoing verification cost.

Build a general Simulink/Modelica replacement and every vendor adapter first.
This creates a broad language/toolchain program before one workflow establishes
the required semantics. Start with a restricted graph and one integration.

## References

- [Engineering plan](../../rocket-simulator-plan.md)
- [Execution contracts, source research and delivery gates](../../rocket-simulator-model-to-flight-plan.md)
- [Implemented behavior and validation evidence](../development-status.md)
