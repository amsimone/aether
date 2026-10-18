# ADR-0004: Per-OS installers, no app store for v1

**Status:** Accepted
**Date:** 2026-10-18

## Context

Aether is a native desktop application (per ADR-0001, ADR-0002). Users need to install it. The distribution mechanism affects:

- **First-run experience** — how the user gets from "I heard about this tool" to "I'm designing a rocket." Friction here loses users.
- **Updates** — how bug fixes and new features reach existing users. Rocketry is a small community; word-of-mouth means a good update experience compounds.
- **Sandboxing and filesystem access** — Aether writes to a motor cache (`~/.cache/aether/motors/` or `%LOCALAPPDATA%\Aether\motors`), reads/writes design files anywhere on the user's filesystem, and may write Parquet telemetry logs to arbitrary paths. Sandboxed app-store environments complicate all of this.
- **Signing and notarization** — macOS Gatekeeper and Windows SmartScreen warn users about unsigned binaries. Without signing, the first-run experience is "right-click → Open → confirm" or "click More Info → Run anyway," which is hostile.
- **Revenue and review cadence** — app stores take 15–30% and impose 1–5 day review latency. For a small project with a fast iteration cadence, this is a real cost.
- **Budget** — code-signing certificates cost real money (Apple Developer ID ~$99/yr; Windows cert ~$200–400/yr). These are project expenses, not user expenses.

The audience is rocketry hobbyists and engineers, ranging from teenagers with no technical sophistication to professional aerospace engineers. They are comfortable downloading installers from a project website but expect a smooth, signed experience.

## Decision

For v1, Aether will be distributed as **per-OS installers, signed and notarized, with auto-update via GitHub Releases**. No app store submission for v1.

Specifically:

- **macOS**: notarized `.dmg`, built via `cargo-bundle` + `xcrun notarytool`. Universal binary (aarch64 + x86_64).
- **Windows**: signed `.msi` via `cargo-wix`. x86_64 (arm64 Windows is rare in our audience; revisit if demand appears).
- **Linux**: `.deb` + `.rpm` + a static binary tarball. Publish a Flatpak to Flathub as the "store" path (no revenue share, no surprise sandbox issues, broad distro coverage).
- **Updates**: the `self_update` crate polling a release JSON on GitHub Releases. User-controlled (check + prompt), never silent. Delta updates (zsync-style patching) deferred to v1.x.

**Signing certs** (project budget items):

- Apple Developer ID ($99/yr) — required for notarization.
- Windows code-signing certificate (~$200–400/yr) — OV (Organization Validation) is sufficient for v1; EV (Extended Validation) gives immediate SmartScreen reputation but costs more and requires a hardware token. Start with OV, build reputation, consider EV if SmartScreen complaints appear.

**Build pipeline**: GitHub Actions matrix (macos-latest, windows-latest, ubuntu-latest) producing the installers on every tagged release. Signing keys stored as GitHub Actions secrets.

## Consequences

**Positive:**

- No app store review latency — we can ship a fix the same day a bug is reported.
- No 15–30% revenue share (relevant if we ever add paid tiers).
- No sandbox restrictions on filesystem access — the motor cache and design file flows work as designed.
- GitHub Releases gives us a canonical download URL, version history, and release notes for free.
- `self_update` provides a smooth update path: users get notified, click update, the app replaces itself and restarts.
- Flatpak covers the Linux "I want it from a store" audience without the downsides of Mac App Store / Microsoft Store.

**Negative:**

- Users must find the website to download. We mitigate with good SEO, a clear landing page, and presence on rocketry forums.
- macOS notarization and Windows signing require CI infrastructure that handles certificates securely. Apple notarization in particular requires the Apple ID credentials as CI secrets, with rate-limit handling.
- SmartScreen reputation builds slowly on OV certs — early Windows users may see a warning until enough downloads accrue. We will document the "click More Info → Run anyway" path in the install guide and work to build reputation.
- Linux distribution is fragmented: `.deb` covers Debian/Ubuntu, `.rpm` covers Fedora/RHEL, but Arch, NixOS, and others need community packaging. We will provide a static binary tarball as a fallback and welcome community-maintained packages.
- Auto-update via `self_update` requires careful handling of permissions (the binary must be writable by the user; system-wide installs on Linux need root) and of interrupted updates (download to a temp file, verify checksum, atomic rename).

**Neutral:**

- Universal binary on macOS roughly doubles the binary size (~60 MB vs 30 MB) but saves maintaining separate aarch64/x86_64 downloads. Acceptable.

## Alternatives considered

### Mac App Store + Microsoft Store

Rejected for v1. The sandboxing model conflicts with our filesystem usage (motor cache, design files, telemetry logs), the 15–30% revenue share is unjustified for a small project, and the review latency (1–5 days) hurts our iteration cadence. Revisit only if (a) we add a paid tier that benefits from store billing, and (b) we're willing to refactor the filesystem access to use app-sandbox-safe directories.

### Single static binary (no installer)

Rejected as the primary path. A single binary (which Rust produces natively) is great for power users and CI, but non-technical users expect a `.dmg` / `.msi` / `.deb` that integrates with their OS's package management, file associations, and application launcher. We will *also* ship a static binary tarball for power users, but it's not the recommended path.

### Homebrew / Scoop / AUR as primary distribution

Useful as secondary channels (and we will support community-maintained Homebrew formulae, Scoop manifests, and AUR packages), but they require the user to have those package managers installed. The primary path must work for a user who has only a browser.

### Snap / AppImage (Linux)

Snap requires Canonical's store for the primary path (or self-hosting a snap store, which is overkill). AppImage is a single-file portable binary — attractive, but lacks the OS integration (file associations, launcher entry, auto-update) that a `.deb`/`.rpm` provides. We will publish an AppImage as an additional Linux option, not the primary one. Flatpak was chosen as the "store-like" Linux path because Flathub is broadly trusted and the sandbox model is well-understood.

### No auto-update, manual download only

Rejected. Manual update is a poor experience and leads to users running old versions with known bugs. The `self_update` crate provides a good balance: user-controlled (no silent updates), but smooth (one click). The risk of a broken update (e.g., a corrupted download) is mitigated by checksum verification and atomic file replacement.

## References

- Plan: `rocket-simulator-plan.md` §18 Q2.
- `cargo-bundle`: https://crates.io/crates/cargo-bundle
- `cargo-wix`: https://crates.io/crates/cargo-wix
- `self_update`: https://crates.io/crates/self_update
- Flathub publishing: https://docs.flathub.org/docs/for-app-authors/submission
- Apple notarization: https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution
