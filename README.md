# Aether

A professional, tiered rocket design and 6-DOF flight simulator. Native Rust
core with an egui UI and a wgpu 3D viewport. Targets model rockets
through amateur/sounding vehicles, with physics fidelity that scales from
subsonic Barrowman to supersonic compressible aerodynamics.

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

Saves write a schema-2 project containing the vehicle and one named flight case,
including its resolved motor curve, launch/wind/recovery settings, model set,
and numerics. Files replace atomically. Legacy schema-1 designs still open;
select a motor for their imported default case. The portable
[`examples/aether-one-project.rocket`](examples/aether-one-project.rocket)
includes the reference vehicle and a complete offline case.

Choose **Simulate** for the bundled A8-3, B6, and C6 sample motor curves. These
work offline and are embedded in the app. See the [UI review](docs/ui-review.md)
for the changes and remaining UX work.

See the planning documents for the full architecture and roadmap:

- `docs/adr/` — Architecture Decision Records (0001–0009)
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
```

## License

MPL-2.0. See [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md).

Motor data is fetched on demand from [ThrustCurve.org](https://www.thrustcurve.org/)
via their public API and is governed by each file's per-file license. See
`docs/adr/0006-thrustcurve-api-fetch-on-demand.md`.
