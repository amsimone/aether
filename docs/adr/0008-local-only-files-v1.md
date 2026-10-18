# ADR-0008: Local-only files for v1, format designed for future sync

**Status:** Accepted
**Date:** 2026-10-18

## Context

Aether saves rocket designs as `.rocket` files (JSON, per plan §13). The question is whether v1 supports real-time collaboration — multiple users editing the same design simultaneously, like Google Docs — or remains a local-file tool like OpenRocket.

The forces:

1. **Scope.** Real-time collaboration is a 3–6 month project unto itself: accounts, authentication, a server (or a CRDT layer with peer-to-peer sync), conflict resolution, presence indicators, undo/undo across users. This would dominate v1 scope and push the physics and UI work to v2.
2. **Audience.** Rocketry clubs *do* share designs — by email, by Git, by Dropbox. OpenRocket users do this today with `.ork` files. The collaboration they need is "I'll send you my file, you tweak it, send it back" — not "let's both edit at the same time."
3. **Format lock-in risk.** If we build a file format that can't later support collaboration, we pay a painful migration cost when we add it. The format decisions made now either enable or block future sync.
4. **Complexity budget.** A local-file tool is dramatically simpler: no server, no auth, no conflict resolution, no network failures. The simplicity shows up as fewer bugs and faster development of the features that matter (physics, UI).
5. **Future optionality.** A Git-based collaboration flow (designs as a repo, pull-request-style review) is the lightest-weight real-collab option and fits a file-based format naturally. A hosted sync server is the heaviest. We want to keep both options open.

## Decision

For v1, Aether will be **local-files only** — no built-in collaboration, no sync server, no accounts. Designs are `.rocket` files (JSON) that users share however they like (email, Git, Dropbox, a shared folder), exactly as OpenRocket users share `.ork` files today.

However, the **file format will be designed sync-friendly from day one**:

1. **Stable component IDs.** Every component in a rocket design has a stable, unique ID (a UUID or a content-addressable hash). References between components (e.g., "this fin set is mounted on body tube #abc123") use IDs, not array positions. This means a partial edit (e.g., "change fin span on fin set #abc123") can be expressed as a patch against the ID, without worrying about where the component sits in the tree.
2. **No positional references.** Nothing in the format depends on the order of components in an array or the byte offset of a field. Reordering components is a valid operation that doesn't break references.
3. **Operations expressible as patches.** Internally, the UI maintains an `Edit` enum (AddComponent, RemoveComponent, ModifyComponent, ReorderComponent, …) even though v1 only applies these directly to the in-memory model. The format and the edit log are structured so that a future sync layer can serialize and transmit these patches.
4. **Versioned schema.** The `.rocket` format includes a `version` field. Migrations are explicit, one-directional functions. This means a future sync server can handle clients on different schema versions by migrating patches.
5. **No hidden state.** Everything needed to reproduce a design is in the file. No user preferences, no UI state, no cache. (UI state lives in a separate settings file.)

**Deferred to v2.x if demand appears:** a Git-based collaboration flow (designs as a repo, branch/merge/PR) — the lightest-weight real-collab option, and a natural fit for the file format we're building. A hosted sync server is the heaviest option; only if a clear paying audience emerges.

**v1.x feature:** a "Compare designs" view that diffs two `.rocket` files and shows the differences component-by-component. This is cheap to build on the ID-based format, useful to clubs today, and doubles as the foundation for any future merge logic.

## Consequences

**Positive:**

- v1 ships faster, with the engineering budget spent on physics and UI rather than collaboration infrastructure.
- No server to maintain, no auth to secure, no privacy concerns about user data.
- Designs work offline, always.
- The file format is forward-compatible with future collaboration, so we don't paint ourselves into a corner.
- Users can use whatever sharing mechanism they already use (Git, Dropbox, email) — we don't impose a workflow.
- The "Compare designs" feature in v1.x gives clubs a real collaboration tool (review a teammate's design before a launch) without a sync layer.

**Negative:**

- No real-time multi-user editing in v1. If two users edit the same `.rocket` file simultaneously (e.g., both have it open in Dropbox), the last save wins and the other's changes are lost. We mitigate by detecting external file modifications (file-watcher) and warning the user, but we don't merge.
- The format design has a small ongoing cost: every feature added to the data model must preserve the sync-friendly properties (stable IDs, no positional refs). This is a discipline, not a heavy burden, but it must be enforced in code review.
- The `Edit` enum is dead code in v1 (the UI applies edits directly rather than via patches). This is intentional — it's a forward-looking design that costs nothing now and saves a future migration.

**Neutral:**

- The `.rocket` format being JSON means it's diff-friendly in Git out of the box (line-oriented diffs, mergeable in most cases). This is a side benefit of the format choice, not a goal.

## Alternatives considered

### Build real-time collaboration for v1

Rejected on scope. A 3–6 month collaboration layer would push the physics and UI work — the actual differentiation of Aether — to v2. The audience's collaboration needs are met (imperfectly but adequately) by file sharing. We can add real-time collaboration later if the file-sharing model proves insufficient.

### Use a hosted sync server from day one

Rejected. A server adds operational burden (hosting, monitoring, backups, security), a privacy surface (user data on our servers), and a business-model question (free? paid? freemium?). None of these are justified for v1. If a sync server is built later, it can be built on the patch-based format we're designing now.

### Use a CRDT library (Yjs, Automerge, Yrs) for offline-first sync

Tempting — these libraries solve the merge problem generically. Rejected for v1 because:

- They add a substantial dependency and a learning curve.
- The CRDT model imposes constraints on the data model (everything must be a CRDT type) that may not fit our component tree cleanly.
- Without a transport layer (server or P2P), a CRDT library is just an in-memory data structure — and we don't need merge logic without a transport.

If we add sync later, a Rust CRDT library (like `yrs`) is a strong candidate for the merge layer. The format design in this ADR is compatible with wrapping the data model in a CRDT.

### Use Git as the collaboration layer (designs as a repo)

Attractive — Git already solves versioning, branching, merging, and review. Rejected for v1 because:

- It requires the user to have Git installed and understand it, which excludes much of our audience.
- The UX of "commit, push, pull request" is heavier than "save file" for the common case of a single user iterating on a design.

It's a strong candidate for a v2.x "power-user collaboration" feature, layered on top of the file format we're designing. This ADR explicitly leaves that door open.

### Proprietary cloud-synced format (no local files)

Rejected. Locks users into our service, breaks offline use, and creates a single point of failure. The local-file model is more robust and more respectful of users.

## References

- Plan: `rocket-simulator-plan.md` §8 (data model), §13 (persistence), §18 Q6.
- "CRDTs for collaborative editing": https://crdt.tech/
- `yrs` (Rust port of Yjs): https://crates.io/crates/yrs
- OpenRocket file format (`.ork`): https://openrocket.info/documentation/ofileformat.html
