# Aether

Aether is being built as an open engineering workbench connecting vehicle and
controller models to simulation, generated controller code, hardware-in-the-loop
tests, and flight-data reconciliation. The production workspace is Rust with
an egui UI and a wgpu 3D viewport.

The [engineering plan](rocket-simulator-plan.md) and
[model to flight execution plan](rocket-simulator-model-to-flight-plan.md)
define that direction: declarative models, automatic linearization, CPU/GPU
Monte Carlo, controller C generation, HIL, and telemetry replay. These new
capabilities are planned; the status below describes the current application.

## Status

The native design editor supports component editing, a selectable 3D model,
design stability estimates, and flight simulation. Aerodynamics and simulation
remain under development; see the [development status](docs/development-status.md)
for implemented increments and validation evidence.

## Try the editor

```sh
cargo run -p rocket-ui
```

The app starts with **Aether One**, a 250 mm reference rocket with a nose,
body, three fins, launch lug, and recovery mass. The same design is available
as [`examples/aether-one.rocket`](examples/aether-one.rocket). Use **Load reference
design** in the parts panel or File menu to restore it. The reference is a software
test model, not a certified flight design.

Select parts in the list or click the model to edit dimensions. Selected parts
highlight in cyan. Enable **Isolate part** to inspect internal components. Drag
to orbit, scroll over the canvas to zoom, Shift-drag to pan, and double-click
or choose **Fit view** to frame the model.

Component property edits, additions, duplication, deletion, and case changes
support **Edit → Undo/Redo** and Cmd/Ctrl+Z. Invalid dimensions and references
are rejected. New/Open/reference/close actions offer Save, Discard, or Cancel
when the vehicle or case has changed.

The toolbar **Save** button writes a schema-3 project containing the vehicle,
one named flight case, and retained run records,
including its resolved motor curve, launch/wind/recovery settings, model set,
and numerics. Files replace atomically. Schema-2 projects migrate with an empty
run list. Legacy schema-1 designs still open;
select a motor for their imported default case. The portable
[`examples/aether-one-project.rocket`](examples/aether-one-project.rocket)
includes the reference vehicle and a complete offline case.

Choose **Flight case** for the bundled A8-3, B6, and C6 sample motor curves. These
work offline and are embedded in the app. See the [UI review](docs/ui-review.md)
for the changes and remaining UX work.

**Run case → Results** runs in the background with cancellation. Results retain
their exact inputs, source/solver versions, termination status, events, and full
telemetry. Compare two runs, select plot channels, zoom/pan/inspect samples, and
export CSV or a portable run JSON. Saving and reopening preserves the runs.

**Aerodynamics** imports explicit coefficient datasets for inspection, comparison,
and deliberate selection in the flight case. Its CFD panel exports or runs a
local Gmsh/OpenFOAM study with retained geometry, mesh, solver files, and logs.
The first backend is exploratory steady subsonic RANS; convergence is not an
accuracy certificate. See [setup, contracts, and limits](docs/development-status.md#analysis-and-cfd-increment).

See the planning documents for the full architecture and roadmap:

- `docs/adr/` — Architecture Decision Records (0001–0011)
- Plan, technical specs, and Phase-0 spike: see the project documentation.

## Building

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The CLI binary:

```sh
cargo run -p rocket-cli -- --version
cargo run -p rocket-cli -- status
cargo run -p rocket-cli -- simulate examples/aether-one-project.rocket --output /tmp/run.json --csv /tmp/telemetry.csv
cargo run -p rocket-cli -- cfd-prepare examples/aether-one-project.rocket /tmp/aether-cfd-study
# After installing the external tools and selecting their paths:
cargo run -p rocket-cli -- cfd-run /tmp/aether-cfd-study --python /path/to/venv/bin/python --foam-bashrc /path/to/OpenFOAM/etc/bashrc
```

## License

MPL-2.0. See [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md).

Motor data is fetched on demand from [ThrustCurve.org](https://www.thrustcurve.org/)
via their public API and is governed by each file's per-file license. See
`docs/adr/0006-thrustcurve-api-fetch-on-demand.md`.
