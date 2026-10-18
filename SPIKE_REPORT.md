# Phase-0 Spike Report

**Dates:** 2026-10-18
**Engineer(s):** solo (with AI assistance)
**Repo:** https://github.com/amsimone/aether
**Commit range:** `980e247` … `1a0e3cd` (5 commits)

## Summary

All five Phase-0 tasks were completed in a single working day, de-risking
the two architecture-level unknowns (egui+wgpu integration, ThrustCurve
API) and standing up the foundation that Phase 1 builds on. Both
go/no-go questions are answered **PASS** — no ADRs were invalidated,
though ADR-0002 (egui) needs a minor refinement noting that three-d is
OpenGL-based and cannot be used (a real finding the spike was designed
to surface). The repo is CI-green on ubuntu-latest with 54 tests
passing, and the motor pipeline works end-to-end from the CLI.

## Task 1: Repo scaffold & CI

- **Status:** PASS
- **Commit:** `980e247`
- 11-crate Cargo workspace matching ADR-0001's layered architecture.
- `#![deny(unsafe_code)]` + `clippy::pedantic` at workspace level.
- CI: ubuntu-latest, fmt + clippy + nextest + doc + cargo-deny + DAG check.
- `scripts/check_dag.sh` enforces that `rocket-core`, `rocket-physics`,
  `rocket-aero` have no UI/network dependencies.
- `rocket-core` has dimensional newtypes (`Length`, `Mass`, `Velocity`,
  `Force`, `Pressure`, `Duration`) with SI-internal storage, unit
  conversions, serde, and arithmetic.
- `rocket-cli` skeleton: `aether --version`, `aether status`.
- `rocket-io`: `save_to_json` / `load_from_json` for `.rocket` files.
- 9 tests passing.
- **Notes:** Initial `deny.toml` used pre-0.20 cargo-deny syntax
  (`vulnerability = "deny"`); fixed to use the `unmaintained = "workspace"`
  scope syntax. Initial `version.workspace = true` syntax failed inside
  `[dependencies]` — path deps need explicit `version = "0.1.0"`.

## Task 2: egui + wgpu + three-d integration

- **Status:** PASS (with significant finding)
- **Commit:** `367dd39`
- **Path taken:** Primary (raw wgpu via `egui_wgpu::CallbackTrait`)
- **Performance:** Not formally benchmarked; app launches, wgpu backend
  initializes, cube renders, no panics. CPU usage at idle appears
  reasonable (subjective; not measured with a profiler).

### Key discovery: three-d is OpenGL-based, not wgpu

**This is the most important finding of the spike.** three-d 0.19.0
(July 2026) uses `glow ^0.17` (OpenGL) as its core graphics API. The
wgpu backend requested in three-d issue #221 (April 2022) was never
implemented. This means three-d **cannot share egui's wgpu device** —
they are fundamentally different graphics APIs (OpenGL vs wgpu/WebGPU).

The spike uses **raw wgpu** instead, following the official egui
`custom3d_wgpu.rs` example pattern:
`egui_wgpu::Callback::new_paint_callback(rect, ...)` with a
`CallbackTrait` impl whose `prepare` method updates uniforms (with
queue access) and `paint` method issues draw calls (with render pass
access).

### ADR-0002 refinement

ADR-0002 (egui for UI) is **confirmed, not invalidated**. The decision
to use egui stands. However, the "three-d" mention in ADR-0002's
decision text and in the plan §5 (technology stack) needs updating:
three-d is not a viable 3D library for our wgpu-based stack.

For Phase 1, the 3D library options are:
1. **rend3** — wgpu-native, explicitly designed to accept user-provided
   wgpu devices. Most mature option.
2. **scena** — newer wgpu-native scene graph with glTF loading, cameras,
   picking. Less battle-tested.
3. **Raw wgpu** — most control, most code. What the spike uses.

**Action:** file ADR-0010 ("3D library choice for Phase 1") once the
rend3 vs scena vs raw-wgpu evaluation is done at the start of Phase 1.
Until then, the spike's raw-wgpu approach is sufficient.

### eframe 0.37 API changes

The spike revealed that eframe 0.37 has significant API changes from
the 0.34 version ADR-0002 was originally written against:

- `App::update` → split into `App::ui` (receives `&mut Ui`) and
  `App::logic` (receives `&Context`, for non-UI state advancement).
- `SidePanel` → generalized to `Panel` (with `Panel::left/right/top/bottom`).
- `Panel::show` takes `&mut Ui`, not `&Context`.
- wgpu 30.0.1 is the current version (egui-wgpu 0.37 depends on it).

These are mechanical updates; ADR-0002's rationale is unaffected.

## Task 3: .eng parser

- **Status:** PASS
- **Commit:** `aa252b0`
- **Test count:** 33 (30 unit + 3 property tests via `proptest`)
- **Fuzz findings:** None. `prop_no_panic_on_arbitrary_input` ran
  256 cases of arbitrary strings with no panics.

### Implementation

- `parse()`: single motor from a `.eng` string.
- `parse_all()`: multi-motor files (motors separated by comment lines).
- `ParseError` enum with `Header` and `DataPoint` variants, both
  carrying line numbers.
- Handles: comments, blank lines (within and between motors), CRLF,
  extra whitespace, multi-word manufacturer names.
- Delays parsing: single, multiple (`3-5-10`), `0` (no delay), `P`/`p`
  (plugged).

### Computed accessors on `Motor`

- `total_impulse()`: trapezoidal integral of the thrust curve.
- `burn_time()`: last timestamp.
- `max_thrust()`: peak thrust.
- `average_thrust()`: `total_impulse / burn_time`.
- `specific_impulse()`: `total_impulse / (prop_mass * g0)`.

All verified against manual calculations in the unit tests.

## Task 4: ThrustCurve API client + cache

- **Status:** PASS
- **Commit:** `1a0e3cd`
- **API behavior notes:** The ThrustCurve.org v1 JSON API behaves as
  documented. Search returns `{"matches": N, "results": [...]}` with
  motor summaries. Download returns `{"results": [...]}` with base64-
  encoded RASP file content in the `data` field. No `license` field is
  returned by the download endpoint (only `source`: "cert"/"mfr"/"user");
  license status must be inferred or fetched separately.
- **ADR-0006 confirmed:** the fetch-on-demand approach works. No motor
  files are bundled; the user fetches via the API and the data is cached
  locally. The curated PD starter set (Task 5) covers offline first use.

### Verified end-to-end

```
$ aether motors search A8
Found 3 motor(s):
5f4294d2000231000000000b A  18  2.5  ...  A8

$ aether motors info 5f4294d2000231000000000b
Designation:    A8
Total impulse:  2.14 N·s
Specific imp.:  56.9 s

$ aether motors info 5f4294d2000231000000000b  (second run)
(from cache)  ← instant, no network

$ aether motors list
Cached motors (1):
5f4294d2000231000000000b A8  Estes  cert
```

Throttling (1 req/s) and exponential-backoff retry on 429/5xx are
implemented. The cache uses the per-OS cache directory (via the
`directories` crate).

### Feature-gated

The API client is behind the `api` feature (default on) so that
`rocket-motors` can be used without network dependencies in contexts
that only need the parser (e.g., future WASM builds).

## Task 5: Validation fixtures

- **Status:** PASS
- **Commit:** `1a0e3cd`
- **Fixtures staged:** 6 reference rockets, 3 motor fixtures, 1 SP-7224
  reference data file, 22 validation test stubs.

### Reference rockets (`tests/fixtures/rockets/`)

| Fixture | Tier | Validation use |
|---------|------|----------------|
| `estes_alpha.json` | T0 | Cd vs Mach, apogee |
| `sp7224_worked_example.json` | T0 | C_Nα, x_cp exact match |
| `sp7224_4fin_variant.json` | T0 | Fin-count projection factor |
| `boattail_test.json` | T0 | CP moves forward with boattail (sign check) |
| `supersonic_dart.json` | T1 | CP vs Mach at M=0.5, 1.0, 2.0 |
| `hpr_dual_deploy.json` | T1 | End-to-end apogee + descent |

### Motor fixtures (`tests/fixtures/motors/`)

Three hand-verified PD-licensed `.eng` files (Estes A8, B6, C6), each
with a sidecar `.meta.json` recording the license status. These are
the "starter set" from ADR-0006 — covers offline first use.

### SP-7224 reference data

`tests/fixtures/reference_data/sp7224_worked_example.json` contains a
machine-readable transcription of the SP-7224 worked example tables.
Some values are marked `TODO` (to be transcribed from the original
report before the aero unit tests can assert against them); the
conical-nose values (C_Nα=2, x_cp=⅔·L) are exact and can be asserted
immediately.

### Validation test stubs (`crates/aether-tests/`)

A new workspace crate with 22 tests:
- **3 running** (motor fixture parsing) — verify the RASP parser
  handles real `.eng` files with correct impulse-class ranges.
- **9 ignored** (aero validation, Phase 3+):
  - SP-7224 conical nose C_Nα and x_cp (exact match)
  - SP-7224 3-fin and 4-fin C_Nα (exact match)
  - 4-fin/3-fin projection ratio
  - Boattail moves CP forward (sign check)
  - Estes Alpha CP and Cd at M=0.1
  - Supersonic dart CP and Cd vs Mach
  - Symmetry: zero β → zero side force
- **10 ignored** (dynamics validation, Phase 2+):
  - Vacuum vertical shot (closed-form)
  - Vacuum spinning body (closed-form)
  - Ballistic dart zero drift (the sign-bug catcher)
  - Energy and angular momentum conservation
  - Quaternion norm unity
  - Estes Alpha apogee (end-to-end)
  - HPR dual-deploy flight (end-to-end)
  - Spin decay (analytical)
  - Weathercocking direction

Each `#[ignore]` test has a `TODO` comment naming the assertion and
the phase that will implement it. They form the validation to-do list
for Phases 2–4.

## New ADRs filed

None yet. **ADR-0010 should be filed at the start of Phase 1** to
record the 3D library choice (rend3 vs scena vs raw wgpu), informed by
the Task 2 finding that three-d is OpenGL-based.

## Open issues for Phase 1

1. **3D library selection.** The spike uses raw wgpu, which is
   sufficient for a cube but not for procedural rocket geometry (nose
   cones via lathe, fin sets via instancing, etc.). Evaluate rend3
   (wgpu-native, accepts user devices) vs scena (newer, scene graph)
   vs continuing with raw wgpu + a thin geometry layer. File ADR-0010.

2. **Component data model.** `rocket-core` currently has only the
   `Rocket` root struct. Phase 1 needs the full `Component` enum
   (`NoseCone`, `BodyTube`, `FinSet`, `Transition`, `Coupler`,
   `LaunchLug`, `Pod`, `MassObject`) with parametric geometry fields,
   stable IDs (ADR-0008), and serde. See plan §8.

3. **Procedural geometry pipeline.** The 3D viewport needs to build
   meshes from the component tree: nose cones from lathe of the profile
   function, body tubes as cylinders, fin sets instanced, transitions
   as conical frustra. See aero-dynamics doc §2.1 and plan §12.

4. **Design-time aero approximation.** The live CG/CP readouts in the
   design editor need a fast, approximate Barrowman calculation
   (cylinders + nose + fin only) that updates as the user drags
   sliders. The full aero module (Phase 3) is too slow for live UI.
   See aero-dynamics doc §8 (`AeroModel::build_up`).

5. **eframe 0.37 API patterns.** The spike revealed the `App::ui` +
   `App::logic` split and the `Panel` (not `SidePanel`) API. Phase 1's
   docking layout (`egui_dock`) needs to be validated against this
   version. `egui_dock` 0.21.1 is current; confirm compatibility.

## Recommendation

**Proceed to Phase 1.** All spike exit criteria are met:

- ✅ All five tasks pass their acceptance criteria.
- ✅ The two go/no-go questions are answered in writing (both PASS).
- ✅ No ADRs invalidated (ADR-0002 needs a minor refinement re: three-d).
- ✅ The repo is in a state where Phase 1 can begin immediately:
  workspace builds, CI is green, the UI spike demo runs, the motor
  pipeline works end-to-end from the CLI, and validation fixtures are
  staged.

The spike's hard time-boxes held: Task 2's 2-day budget was not needed
(the raw-wgpu path worked on the first attempt once the eframe 0.37 API
was understood). The biggest single time sink was clippy's pedantic
lints in test code (resolved with scoped `#![allow(...)]` attributes).

---

*Report version 1.0 — Phase 0 complete.*
