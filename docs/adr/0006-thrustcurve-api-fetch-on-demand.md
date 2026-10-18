# ADR-0006: ThrustCurve API fetch-on-demand, no bundled motors

**Status:** Accepted
**Date:** 2026-10-18

## Context

A rocket simulator needs a library of motor thrust curves. The de-facto source is **ThrustCurve.org**, maintained by John Coker since 1998, which hosts over a thousand `.eng` (RASP) and `.rse` (RockSim) files for certified model and high-power rocket motors. OpenRocket bundles a snapshot of this database with each release.

The question is whether Aether should:

1. Bundle a snapshot of the ThrustCurve.org database with each release (as OpenRocket does), or
2. Fetch motor data on demand from the ThrustCurve.org API.

The forces:

- **Legal.** Each motor file on ThrustCurve.org carries its own per-file license tag: *Public Domain*, *Free Usage* (GPL/CC/Apache), *Other (restricted)*, or *Unknown*. There is **no blanket license** granting redistribution of the catalog. Many files are tagged *Unknown* — meaning the data was sourced from elsewhere and the license is unclear, making redistribution legally unsafe.
- **Freshness.** New motors are certified regularly; a bundled snapshot goes stale between releases. OpenRocket users routinely encounter "my motor isn't in the database" because the snapshot is months old.
- **Author intent.** John Coker (ThrustCurve.org maintainer) provides a sanctioned JSON/XML API (`/api/v1/search.json`, `/api/v1/download.json`) explicitly intended for simulators to fetch data programmatically. The OpenRocket motor-database repo notes: *"If you wish to use this data for other projects, please use the ThrustCurve.org API directly."*
- **Offline use.** Rocketry clubs often fly at remote sites with no internet. A purely fetch-on-demand model breaks for these users — they need motors available offline.
- **Performance.** A bundled database is instant. API fetches add latency and a dependency on ThrustCurve.org's availability.

## Decision

Aether will **ship with no motor files bundled** and will fetch motor data on demand from the ThrustCurve.org API. Specifically:

1. **No bundled catalog.** The release contains no `.eng`/`.rse` files (with the exception of the curated starter set in point 3).
2. **API client.** `rocket-motors` includes a ThrustCurve API client (`reqwest` + `tokio`) that:
   - Searches the catalog by class, diameter, manufacturer, total impulse, designation.
   - Downloads selected `.eng`/`.rse` files into a user-writable cache (`~/.cache/aether/motors/` on Linux/macOS, `%LOCALAPPDATA%\Aether\motors` on Windows).
   - Stores the per-file license tag alongside each cached file.
   - Throttles requests (max 1 req/s) and retries with exponential backoff, to be polite to a free service.
3. **Curated PD starter set.** A small set (~20) of classic motors (Estes A8/B6/C6, Aerotech F/G/H samples) hand-verified as Public Domain will ship in-repo under their per-file terms, so the app is useful on first launch with no internet. For any ambiguous-licence motor, we will seek explicit written permission from John Coker before including it.
4. **Custom motors and imports.** Users can always import their own `.eng`/`.rse`/CSV files (drag-and-drop or file picker), and these work fully offline. Custom motors entered via the in-app form are saved as user files in the same cache.
5. **License-aware UI.** The motor browser displays each motor's license tag. Files tagged *Unknown* or *restricted* are usable locally (the user fetched them) but the app never re-distributes them.

## Consequences

**Positive:**

- **Legally clean.** We redistribute only the hand-verified PD starter set. Everything else is fetched by the user, for the user, and stays in the user's cache. No ambiguity about per-file licenses.
- **Always fresh.** New motors appear as soon as ThrustCurve.org has them, without waiting for an Aether release.
- **Respects author intent.** We use the API the way John Coker asks third-party projects to.
- **Smaller binary.** No multi-megabyte motor database in the installer.
- **Future-proof.** If ThrustCurve.org adds metadata (e.g., propellant CG shift, certification status), we get it via the API without schema changes.

**Negative:**

- **First-run requires connectivity** (for any motor outside the starter set). We mitigate with the starter set and clear messaging ("For more motors, click 'Browse ThrustCurve.org'").
- **API dependency.** If ThrustCurve.org is down, motor browsing fails. The cache means already-downloaded motors still work; only new downloads are affected. We will document this clearly.
- **Engineering cost.** `rocket-motors` needs an HTTP client, an API client module, throttling/retry logic, and a cache manager. This is a Phase-2 task (per the plan) adding maybe a week of work.
- **Politeness burden.** A free service deserves respect. We must implement throttling, caching, and avoid hammering the API. A bug that requests the same motor 10,000 times (e.g., in a Monte Carlo loop) would be abusive — we must ensure cached motors are read from disk, not re-fetched.
- **Starter set maintenance.** The ~20-motor PD starter set must be hand-verified. If any motor's license tag on ThrustCurve.org changes, we need to re-verify. Low-frequency work, but real.

**Neutral:**

- The cache is per-user, not shared. If two users on the same machine both use Aether, they have separate caches. This is fine for our use case.

## Alternatives considered

### Bundle the full catalog (OpenRocket's model)

Rejected. The per-file license tags make this legally unsafe for many files (the *Unknown* tag alone covers a significant fraction of the catalog). OpenRocket can do this because their GPL-3.0 license and their standing in the community give them a different relationship with ThrustCurve.org; we cannot assume the same. John Coker's explicit guidance to use the API confirms this.

### Bundle only PD-licensed motors

A larger version of the starter set — every motor tagged *Public Domain* on ThrustCurve.org, bundled. Rejected because:

- The PD subset is a small fraction of the catalog, so the "no motor found" problem persists.
- The PD tags change over time as contributors re-license; we'd need to re-verify each release.
- The freshness problem remains for the PD subset.

The starter set of ~20 hand-verified motors covers the common LPR/MPR cases for offline first use; everything else goes through the API.

### Mirror the catalog on our own server

We'd run a nightly job to fetch the catalog via the API and serve it from `motors.aether.dev` or similar. Rejected because:

- It adds infrastructure cost and a server to maintain.
- It doesn't solve the licensing problem — we'd still be redistributing *Unknown*-tagged files.
- It duplicates ThrustCurve.org's role, which is disrespectful to the original service.

### Bundle the OpenRocket motor-database repo

The OpenRocket motor-database repo (GPL-3.0) contains ThrustCurve.org data reformatted. Rejected because:

- It's GPL-3.0, which conflicts with our MPL-2.0 (ADR-0005) for any code-touching use, and more importantly the *data* in it is governed by the same per-file ThrustCurve.org licenses as the source.
- The repo's own README says to use the ThrustCurve API for non-OpenRocket projects.

### Require users to supply their own motor files

Purist approach: ship no motors, require users to download `.eng` files themselves. Rejected as hostile UX — the first-run experience would be "you can't simulate anything until you go find a motor file." The curated starter set + API browser is the right balance of legal cleanliness and usability.

## References

- Plan: `rocket-simulator-plan.md` §9 (motor library), §18 Q4.
- ThrustCurve.org API documentation: https://www.thrustcurve.org/info/api.html
- ThrustCurve.org contribute page (per-file license tags): https://www.thrustcurve.org/info/contribute.html
- ThrustCurve.org background: https://www.thrustcurve.org/info/background.html
- OpenRocket motor-database repo: https://github.com/openrocket/motor-database
- RASP file format: https://www.thrustcurve.org/info/raspformat.html
