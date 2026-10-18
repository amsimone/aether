# ADR-0005: MPL-2.0 license for all workspace crates

**Status:** Accepted
**Date:** 2026-10-18

## Context

Aether is a tool we hope will be used by the rocketry community — hobbyists, clubs, educators, and engineers. The license choice affects:

- **Adoption** — a permissive license (MIT, Apache-2.0) lowers the barrier for anyone considering using or extending the project. A copyleft license (GPL) raises it.
- **Contributions back** — a permissive license allows a competitor to fork the whole tool, rebrand it, ship a closed version, and contribute nothing back. A copyleft license forces modifications to be shared.
- **Ecosystem reuse** — we want a flight-controller firmware to be able to link `rocket-physics` for hardware-in-the-loop simulation. If `rocket-physics` is GPL, the firmware must also be GPL, which blocks most commercial and many hobbyist firmware projects.
- **Code reuse from other projects** — OpenRocket is GPL-3.0. If we are GPL, we *could* (with care) reuse code from it. If we are not GPL, we cannot. Either way, we will reimplement from primary literature (the SP-7224 report, Hoerner, etc.), not from OpenRocket's Java.
- **Data compatibility** — OpenRocket's motor database repo is GPL. We are not bundling motors (ADR-0006), so this doesn't constrain the license, but it's relevant context.

The rocketry simulation ecosystem today is dominated by GPL tools (OpenRocket) and proprietary tools (RASAero, RockSim). A new entrant has the chance to set a different precedent.

The forces in tension:

1. We want maximum adoption and reuse, *especially* of the physics core in embedded and educational contexts.
2. We want to protect the project from embrace-extend-rebrand — a competitor taking our work, closing it, and out-marketing us.
3. We want to be compatible with the broader open-source ecosystem (GPL projects especially, since OpenRocket is GPL).
4. We want a license that is easy for contributors to understand and for lawyers to clear.

## Decision

All crates in the Aether workspace will be licensed under the **Mozilla Public License 2.0 (MPL-2.0)**.

Concretely:

- Every crate's `Cargo.toml` declares `license = "MPL-2.0"`.
- Every source file carries an MPL-2.0 header (the standard "This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. …" notice).
- The repository root contains a `LICENSE` file with the MPL-2.0 text.
- The `NOTICE.md` file attributes any MPL-2.0 §3.3-noticed third-party code we depend on.

**Consequence for OpenRocket code:** we **cannot copy code from OpenRocket** (GPL-3.0) into the Aether tree. All algorithms (Barrowman, extended Barrowman, compressibility corrections) are reimplemented from primary literature — the 1967 SP-7224 report, Hoerner's *Fluid-Dynamic Drag*, the Vukelich and RASAero publications. This was already the plan; MPL-2.0 makes it a hard constraint.

**Consequence for OpenRocket data:** the OpenRocket motor-database repo is GPL-3.0. We do not bundle it (ADR-0006); we fetch motor data via the ThrustCurve.org API. So the GPL on the motor-database repo does not affect us.

## Consequences

**Positive:**

- **File-level copyleft.** Anyone who modifies Aether's source files must publish those modifications under MPL-2.0. This protects against embrace-extend-rebrand: a competitor can fork, but their improvements to *our code* come back to the community.
- **Library-friendly.** A closed-source application (a flight-controller firmware, a commercial CAD tool, an educational app) can link `rocket-physics` without its own source becoming MPL. This is the key property that GPL lacks and that matters most for ecosystem adoption.
- **GPL-compatible.** MPL-2.0 is compatible with GPL-3.0+ (per MPL-2.0 §3.3 and the GPL compatibility exception), so a GPL project can incorporate MPL code. This keeps the door open to collaboration with OpenRocket or other GPL tools.
- **Easy to explain.** "Edit our files → share your edits; link our crates → do what you want" is a one-sentence summary that contributors and legal reviewers can understand.
- **Well-understood.** MPL-2.0 is used by Firefox, Rust itself (the standard library is Apache/MIT, but many Rust ecosystem crates are MPL), and thousands of other projects. It has cleared legal review at most major companies.

**Negative:**

- **Not permissive enough for some.** Developers who want MIT/Apache-level freedom to fork-and-close will be deterred. We accept this — the protection of file-level copyleft is worth the loss of those contributors.
- **File-level scope is sometimes confusing.** MPL-2.0's copyleft triggers at the file level, not the project level. A contributor who moves code between files (e.g., refactoring a function from `aero.rs` to `aero/fin.rs`) needs to be aware that the moved code remains MPL. This is a minor friction in contribution review.
- **No code from OpenRocket.** Reimplementing Barrowman et al. from primary sources is more work than porting OpenRocket's Java. This was already the plan for correctness reasons (we want to validate against the literature, not against OpenRocket's implementation), but MPL makes it a legal requirement too.
- **No code from GPL-3.0-only crates** (as opposed to GPL-3.0-or-later). If a useful Rust crate is `GPL-3.0-only` (not `or-later`), it is not MPL-2.0-compatible and we cannot use it. In practice, most GPL crates are `or-later`, but we must check each dependency's license in `cargo-deny`.

**Neutral:**

- MPL-2.0 has a patent grant (§3.3) similar to Apache-2.0's, providing defensive patent protection for contributors and users.

## Alternatives considered

### GPL-3.0 (OpenRocket's choice)

Maximally protective: any project that incorporates Aether code must itself be GPL-3.0+ and publish its source. Rejected because:

- It blocks the "link `rocket-physics` into a flight-controller firmware" use case, which we explicitly want to enable. Most firmware is closed-source.
- It blocks use in commercial CAD tools or educational software, limiting the ecosystem we can build around the physics core.
- The protection it provides against embrace-extend-rebrand is marginal in practice — a competitor determined to fork-and-close can rewrite the code (which they'd have to do anyway to translate Java to Rust if we were starting from OpenRocket).

The argument for GPL is "force everything open." The argument against is "block the use cases we care most about." For a library (physics core) more than an application, the latter wins.

### MIT or Apache-2.0

Maximally permissive: anyone can use, modify, distribute, including in closed-source products, with no obligation to share modifications. Rejected because:

- It provides no protection against embrace-extend-rebrand. A competitor could fork the entire Aether UI, rebrand it, ship a closed version, and contribute nothing back — competing against the original with the original's code.
- For a flagship tool (not just a library), this is a real risk. MIT/Apache is the right choice for infrastructure libraries (like `serde` or `nalgebra`), where adoption is the only goal and forks are not a competitive threat. For an application, it cedes too much.

Apache-2.0 was considered specifically for its patent grant, but MPL-2.0 has an equivalent grant and adds the file-level copyleft we want.

### LGPL-3.0

A middle ground: the library can be linked by closed-source applications (like MPL), but modifications to the library itself must be LGPL (like GPL). Rejected because:

- LGPL's linking exception is legally murky for statically-linked languages like Rust (where the linker combines everything into one binary). MPL-2.0's file-level scope is clearer and more defensible.
- LGPL-3.0 is GPL-3.0-flavored, carrying the same patent and anti-tivoization provisions that complicate adoption.

### Dual licensing (MPL-2.0 + commercial)

Ship the code under MPL-2.0, with a separate commercial license available for users who can't or won't comply with MPL (e.g., they want to embed it in a closed product without file-level disclosure). Rejected for v1 because:

- The administrative overhead (license inquiries, contract negotiation) is not justified for a small project.
- The MPL-2.0 file-level scope is already permissive enough that most commercial users won't need a separate license — they can link the crates and keep their own code closed.

Revisit if a commercial user emerges who needs terms beyond MPL-2.0 (e.g., they want to modify the physics core and keep modifications closed).

### Custom license

Rejected outright. A custom license creates legal review burden for every user and contributor, blocks adoption, and provides no benefit over a well-known existing license.

## References

- Plan: `rocket-simulator-plan.md` §18 Q3.
- MPL-2.0 FAQ: https://www.mozilla.org/en-US/MPL/2.0/FAQ/
- MPL-2.0 FAQ on GPL compatibility: https://www.mozilla.org/en-US/MPL/2.0/FAQ/#gpl-compatibility
- "Choose an open source license": https://choosealicense.com/licenses/mpl-2.0/
- OpenRocket license (GPL-3.0): https://github.com/openrocket/openrocket/blob/unstable/LICENSE
