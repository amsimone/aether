# ADR-0009: Rhai scripting for v1.x hooks; no native FFI plugins

**Status:** Accepted
**Date:** 2026-10-18

**Extension:** [ADR-0011](0011-shared-model-and-execution-targets.md) and the [model to flight plan](../../rocket-simulator-model-to-flight-plan.md) update this decision's scope. Rhai remains optional orchestration tooling. Portable controller semantics belong to the restricted typed graph and generated C path. Generated controller source and separately hosted C ABI/FMI adapters are distinct from arbitrary in-process native plugins; the plugin restriction remains.

## Context

Aether's built-in event system (plan §7.6) covers the common cases: time-based ignition, apogee deployment, altitude-based deployment, burnout detection, staging. But power users will inevitably want custom logic that the built-in events can't anticipate:

- "Ignite stage 2 when acceleration drops below 5 m/s² *and* altitude exceeds 1 km."
- "Deploy a small drogue at Mach 0.3 if the apogee was above 2 km, otherwise deploy at apogee."
- A TVC controller with custom PID gains or a custom guidance law.
- An optimizer objective function like "minimize the cost per meter of apogee, where cost = motor price + airframe material cost."

The question is how to expose this extensibility. The forces:

1. **Hot path vs cold path.** The flight loop (the integrator's per-step force computation) runs ~2,000–10,000 times per sim and must stay in Rust for the <5 ms/sim budget (ADR-0001, MC/optimization doc §5.6). Scripting in the hot path is unacceptable. But event triggers and optimizer objectives are called a handful of times per sim, not per step — scripting there has negligible overhead.
2. **Safety.** Native plugins (dynamically-loaded shared libraries with FFI) execute arbitrary code in the host process. A malicious or buggy plugin can crash the app, corrupt data, or worse. Sandboxing is essential for a tool that users may extend with community-shared scripts.
3. **Ease of embedding.** The scripting runtime must embed cleanly in Rust, without a heavy external dependency (like a Python interpreter) or a garbage collector that complicates the parallel Monte Carlo host.
4. **Reach.** A widely-known scripting language (Lua, Python) lowers the barrier for users to write custom logic. An obscure language raises it.
5. **API stability.** Whatever we expose must be versioned so that scripts written for Aether 1.0 continue to work in 1.5 and 2.0, with a clear deprecation path when we change the API.

## Decision

Aether will expose extensibility through three layers, in order of priority:

1. **Rust library API + CLI (v1.0).** The physics core is a Rust library (ADR-0001) with a clean public API, and the `rocket-cli` binary provides headless batch access. This covers batch workflows, CI integration, and is the substrate for everything else. A user who can write Rust can do anything.
2. **Rhai scripting for two narrow hooks (v1.x).** We embed [Rhai](https://rhai.rs/) — a pure-Rust, sandboxed, no-GC scripting language — and expose it at exactly two extension points:
   - **Custom event triggers:** a Rhai function `should_fire(state, events_fired) -> bool` registered against a named event slot, evaluated instead of the built-in trigger predicates.
   - **Optimizer objective functions:** a Rhai function `score(result) -> float` used as the CMA-ES objective (MC/optimization doc §14.1), so users can optimize for anything.
   The Rhai API surface is tiny and versioned (`aether::script::v1`) so we can evolve it without breaking users.
3. **No native plugins (FFI/WASM) for the foreseeable future.** We will not support dynamically-loaded native plugins. If a use case emerges that Rhai can't cover (e.g., a user needs to call a CFD solver from within an event trigger), we will revisit WASM sandboxing as a v2+ feature, not native FFI.

## Consequences

**Positive:**

- **The hot path stays in Rust.** Rhai is never called per integration step — only at event evaluations (a few times per sim) and at objective evaluations (once per CMA-ES candidate). The <5 ms/sim budget is preserved.
- **Rhai is pure Rust, sandboxed by default, and has no GC.** It embeds as a Cargo dependency with no external runtime, no security surface beyond Rhai's own (well-audited) sandbox, and no garbage-collection pauses that would complicate the `rayon` parallel Monte Carlo host.
- **Rhai is small and fast enough for our use cases.** A typical event-trigger function evaluates in microseconds; called a few times per sim, this is invisible. An objective function called once per CMA-ES candidate is even less of a concern.
- **The API is versioned and tiny.** Exposing only `should_fire` and `score` (plus a small `state`/`result` view object) gives users the two hooks they actually need without a sprawling API surface that we'd have to maintain forever.
- **The Rust API + CLI path is available from v1.0.** Power users who can write Rust have full access immediately; the Rhai layer is a convenience for those who can't or don't want to compile.
- **No native plugins means no security nightmare.** We don't have to worry about a malicious `.dll`/`.so`/`.dylib` compromising users' machines, no code-signing for plugins, no ABI stability concerns.

**Negative:**

- **Rhai is less known than Lua or Python.** Users who already know Lua (common in game scripting) or Python (common in scientific computing) have to learn a new language. We mitigate with good documentation and a library of example scripts.
- **Rhai is slower than Lua (`mlua` with LuaJIT) and much slower than Rust.** For our two hooks this doesn't matter (microseconds per call), but if a future use case emerges that needs high-frequency scripting (e.g., a per-step controller), Rhai won't be fast enough. We accept this — such use cases should be in Rust, not in scripts.
- **The versioned API (`aether::script::v1`) is a maintenance commitment.** Once we ship v1, we must support it (with deprecation warnings) through v2 and beyond. This constrains how the `state` and `result` view objects can evolve. We mitigate by keeping the API minimal and additive (new fields are fine; removing or renaming fields is not).
- **Some users will want Python.** Python is the lingua franca of scientific computing, and an engineer who wants to script Aether from a Jupyter notebook will be disappointed by Rhai. We accept this for v1; a Python binding (via PyO3) is a possible v2 feature, layered on top of the Rust API, not replacing Rhai.

**Neutral:**

- Rhai's syntax is Rust-like, which is natural for the project's contributor base but slightly unfamiliar to Python/Lua natives.

## Alternatives considered

### Lua (via `mlua` with LuaJIT)

The traditional embedded-scripting choice, used by game engines, Neovim, and many others. Rejected because:

- LuaJIT (the fast Lua) is a C dependency with its own build complexities, and LuaJIT's maintenance has slowed.
- `mlua` adds a substantial dependency and exposes a GC'd runtime inside our otherwise GC-free process. While the GC pauses are small, they complicate reasoning about the parallel Monte Carlo host's performance.
- Lua's syntax and semantics (1-indexed arrays, global-by-default variables) are footguns for users coming from Rust/Python.
- Lua's sandboxing is not as tight as Rhai's by default; we'd need to be careful to lock down `os`, `io`, etc.

Lua is a defensible choice and we revisit if Rhai proves limiting, but Rhai's pure-Rust, sandboxed-by-default nature is a better fit for v1.

### Python (via PyO3)

Maximum reach — every scientist knows Python. Rejected for v1 because:

- Embedding CPython is heavy (the interpreter is ~30 MB) and pulls in a runtime with its own threading model.
- The GIL complicates the parallel Monte Carlo host: Rhai scripts can run on any `rayon` worker without locking; Python scripts would need GIL acquisition, serializing them.
- Python's security surface for sandboxing is notoriously bad — `eval` of untrusted Python is a security disaster, and proper sandboxing (e.g., RestrictedPython) is leaky.
- A Python binding makes more sense as a *separate library* (`aether-py`, exposing the Rust API via PyO3) for users who want to script Aether from Python, rather than embedding Python inside Aether. This is a possible v2 feature.

### Native plugins (FFI / dynamically-loaded shared libraries)

Maximum flexibility — users can extend Aether with any language that can produce a shared library. Rejected for the foreseeable future because:

- **Security.** A native plugin runs arbitrary machine code in the host process. A malicious plugin (or a compromised supply chain for a plugin) can do anything — steal files, crash the app, persist malware. For a tool that users may extend with community-shared plugins, this is unacceptable.
- **Stability.** A buggy plugin can corrupt memory, cause crashes that are hard to attribute to the plugin, and break the app for users who don't realize a plugin is at fault.
- **ABI stability.** Rust has no stable ABI. Exposing a Rust API via FFI requires `#[repr(C)]` wrappers and a C ABI, which constrains the API design and adds maintenance burden. Any change to the structs passed across the boundary can break plugins.
- **Build complexity.** Plugins must be compiled against a specific version of the Aether headers, distributed separately, and loaded at runtime. The UX is poor compared to a script file.

If a use case emerges that truly needs native-code performance or external-library access (e.g., calling a CFD solver from an event trigger), the right path is WASM sandboxing (via `wasmtime`), which provides native-code performance with a security boundary. This is a v2+ research item, not a v1 feature.

### WebAssembly (WASM) sandboxing

The most flexible safe option: users write plugins in any language that compiles to WASM (Rust, C, C++, AssemblyScript, …), and we run them in `wasmtime` with a capability-based security model. Rejected for v1 because:

- The integration is substantial: defining a host ABI, managing WASM module compilation and caching, exposing Aether state to the guest, handling memory across the boundary.
- The UX is heavier than a script file: the user has to compile their plugin to WASM before loading it.
- For our two hooks (event triggers and objective functions), Rhai is sufficient. WASM's advantage (any language, native performance) doesn't pay off until the use cases are more demanding.

WASM is the right answer if we ever expand scripting to the hot path (e.g., user-supplied force models) or to plugins that need external libraries. It's a v2+ option, layered on top of the same `aether::script` API.

### No scripting at all (Rust API only)

Purist approach: the Rust API + CLI is the only extension point, and users who want custom logic must write Rust. Rejected because:

- It excludes users who can't or won't compile Rust — a large fraction of the rocketry community.
- The two hooks (event triggers, objective functions) are common enough that a scriptable path is a real usability win.
- A scriptable tool is more shareable — a user can post a Rhai snippet to a forum, where others can paste it in. A Rust extension requires a build toolchain.

The Rust API remains the power-user path; Rhai is the accessibility path.

## References

- Plan: `rocket-simulator-plan.md` §18 and the model-to-flight extension above.
- Rhai: https://rhai.rs/
- `mlua` (Lua bindings for Rust): https://crates.io/crates/mlua
- PyO3 (Python bindings for Rust): https://pyo3.rs/
- `wasmtime` (WASM runtime): https://wasmtime.rs/
- [Model-to-flight execution contracts](../../rocket-simulator-model-to-flight-plan.md) (generated controllers and host orchestration).
