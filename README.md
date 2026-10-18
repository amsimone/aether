# Aether

A professional, tiered rocket design and 6-DOF flight simulator. Native Rust
core with an egui UI and a wgpu/three-d 3D viewport. Targets model rockets
through amateur/sounding vehicles, with physics fidelity that scales from
subsonic Barrowman to supersonic compressible aerodynamics.

## Status

**Phase 0 — scaffolding and de-risking.** Not yet usable.

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
