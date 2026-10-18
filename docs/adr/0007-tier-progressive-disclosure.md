# ADR-0007: Tier progressive disclosure with auto-suggestion

**Status:** Accepted
**Date:** 2026-10-18

## Context

Aether's physics is tiered (plan §2): T0 (model/LPR-MPR, subsonic, ISA atmosphere, flat Earth), T1 (HPR, supersonic, US-1976 atmosphere), T2 (amateur/sounding, NRLMSISE-00, round rotating Earth, J2 gravity). Each tier adds models without replacing the lower-tier ones; a rocket simulated at T0 should give the same answer as at T1 in the regime where T1's additions don't activate (sub-Mach-0.6, below 5 km).

The UI question is how to expose these tiers to the user. The forces:

1. **Don't overwhelm newcomers.** A Cub Scout designing an Estes Alpha should never see NRLMSISE-00 solar-flux inputs or Coriolis-force toggles. Showing all tier-relevant controls always violates the "not amateurish" goal in the opposite direction from OpenRocket — instead of too-simple, it's too-complex.
2. **Don't hide features from power users.** An amateur rocketry engineer flying to 30 km needs T2 and needs to find it. Hiding T2 behind an obscure settings toggle creates a "where's the feature?" discovery problem.
3. **Respect the user's intelligence.** A model rocketeer who grows into HPR should not have to "unlock" features — the tool should grow with them naturally.
4. **Avoid wrong-tier results.** A user simulating a supersonic flight at T0 will get wrong numbers (no compressibility corrections) without knowing why. The tool should make it hard to accidentally pick the wrong tier.
5. **Be transparent.** Whatever tier is chosen, the user should know which tier is active and what models it includes. Black-box simulation is the hallmark of amateur tools.

## Decision

Aether will use **progressive disclosure with auto-suggestion** for tier selection. Specifically:

1. **A 4-way segmented control** in the Simulate panel: `Auto · T0 · T1 · T2`. The user can always pick any tier explicitly.
2. **Auto is the default.** When Auto is selected, the simulator inspects the rocket + environment and picks a tier:
   - Estimate apogee and peak Mach cheaply (1-DOF, constant Cd, ISA atmosphere).
   - Pick T0 if apogee < 5 km and Mach < 0.6.
   - Pick T1 if apogee < 30 km and Mach < 3.
   - Else T2.
3. **Auto explains itself.** Hovering the Auto choice shows a one-line reason: *"Auto: T1 — estimated apogee 8.2 km, peak Mach 0.94."* The user sees *why* Auto picked what it did, and can override.
4. **Tier-relevant inputs are conditionally shown.** T2-only inputs (solar F10.7, geomagnetic Ap index, latitude/longitude for Coriolis) live behind an "Advanced environment" expander that is grayed out unless T2 (or Auto→T2) is active. Same for T1-only inputs (sounding data upload, wind shear profile) unless T1+ is active.
5. **First-run toast.** A one-time toast on first sim explains the tier system in two sentences: *"Aether adapts its physics to your flight. Auto picks the right level; click to override."* With a "Learn more" link to the docs.
6. **Tier is always visible.** A small badge in the status bar shows the active tier (e.g., "T1: HPR") on every sim, so the user always knows what models are running.

## Consequences

**Positive:**

- A newcomer sees only what they need. An Estes Alpha at Auto→T0 shows no advanced controls; the experience is as simple as OpenRocket.
- A power user finds T2 via the segmented control — one click away, no settings menu diving.
- Auto-suggestion prevents the most common error: simulating a supersonic flight at T0 and getting wrong numbers. If the user picks T0 explicitly for a Mach-2 rocket, the UI warns: *"T0 is below the estimated Mach 2.1; results will not include compressibility corrections. Switch to T1?"*
- The reason tooltip teaches users the tier boundaries over time, building intuition for which regimes need which models.
- The status-bar badge makes the active tier impossible to miss, addressing the "black box" concern.

**Negative:**

- The auto-suggestion function is a small additional component (~30 lines in `rocket-sim`). It must be cheap (a 1-DOF estimation) and robust (never crash on weird inputs). Low cost, but real.
- The "Advanced environment" expander's enable/disable logic must track the tier correctly across Auto→tier transitions. A user who sets solar F10.7, then switches from T2 to T0, must not lose the value (it's preserved but inactive) and must see it again if they switch back. State management for conditionally-shown inputs is a known UI friction point.
- The warning when the user explicitly picks a too-low tier could be seen as paternalistic. We mitigate by making it dismissible and never blocking.

**Neutral:**

- The auto-suggestion's thresholds (5 km / Mach 0.6 for T0; 30 km / Mach 3 for T1) are the same as the tier definitions themselves, so there's no inconsistency. They are constants in one place.

## Alternatives considered

### Show all controls always

Rejected. Overwhelms newcomers; violates "not amateurish" in the complexity direction. A Cub Scout facing solar-flux inputs will close the app.

### Single tier (no T0/T1/T2 distinction)

Rejected. Either everything is T0 (wrong for HPR+) or everything is T2 (slow and over-complex for model rockets, and the NRLMSISE-00 inputs are noise for 99% of users). The tiered approach is the whole point of ADR-0001's "layered physics."

### Hide T2 behind a settings toggle

Worse than Auto. The user has to know to go looking for it, the setting is global (so they have to remember to toggle it back for T0 flights), and the discovery problem is real. Auto with explicit override is strictly better.

### Always show all tiers, but disable irrelevant inputs

A hybrid: show every control, but gray out the ones that don't apply to the current tier. Rejected because the visual clutter is the same as "show all always" — the inputs are still there, just disabled. Progressive disclosure (hide rather than disable) is cleaner.

### Skill-based profiles (Beginner / Intermediate / Expert)

The user picks a profile at first run, which sets the default tier and controls the UI complexity. Rejected because:

- It asks the user to self-assess, which is uncomfortable for newcomers (who don't know what "Intermediate" means in this context) and insulting to experts (who don't want to be called "Beginner").
- It's static — a user who grows from Beginner to Expert has to find and change the profile, which is the same discovery problem as hiding T2.
- Auto-suggestion achieves the same adaptivity without asking the user to label themselves.

### Force the user to pick a tier explicitly (no Auto)

Rejected as hostile to newcomers, who don't know what T0/T1/T2 mean on first use. Auto gives them a sensible default; the tooltip teaches them the boundaries over time.

## References

- Plan: `rocket-simulator-plan.md` §2 (scope tiers), §11.3 (Simulate panel), §18 Q5.
- "Progressive Disclosure" (Nielsen Norman Group): https://www.nngroup.com/articles/progressive-disclosure/
