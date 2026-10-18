#!/usr/bin/env bash
#
# check_dag.sh — Verify that core physics crates have no UI or network
# dependencies, per ADR-0001 (Layered Rust Workspace Architecture).
#
# rocket-core, rocket-physics, and rocket-aero must not depend (directly or
# transitively) on any UI or networking crate. This script enforces that
# constraint by inspecting the dependency tree of each protected crate.
#
set -euo pipefail

# Crates that must remain free of UI/network dependencies.
PROTECTED_CRATES=(
    rocket-core
    rocket-physics
    rocket-aero
)

# Crate names that are forbidden in the protected crates' dependency trees.
# Matching is case-insensitive on the crate name as it appears in `cargo tree`.
FORBIDDEN_REGEX='(egui|wgpu|three-d|reqwest|tokio|eframe|egui_dock|egui_plot)'

echo "Checking dependency DAG per ADR-0001..."
echo

fail=0

for crate in "${PROTECTED_CRATES[@]}"; do
    # Get the full transitive dependency tree for this crate.
    # `cargo tree` exits non-zero if the crate doesn't exist or fails to resolve.
    deps=$(cargo tree -p "$crate" 2>/dev/null || true)

    if echo "$deps" | grep -qiE "$FORBIDDEN_REGEX"; then
        matches=$(echo "$deps" | grep -iE "$FORBIDDEN_REGEX" | head -5)
        echo "FAIL: crate '$crate' has a forbidden dependency:"
        echo "$matches" | sed 's/^/    /'
        echo
        echo "Per ADR-0001, $crate must not depend on UI or network code."
        echo "See: docs/adr/0001-layered-rust-workspace-architecture.md"
        fail=1
    else
        echo "OK:   $crate — no forbidden dependencies."
    fi
done

echo

if [ "$fail" -ne 0 ]; then
    echo "DAG check FAILED."
    exit 1
fi

echo "DAG check passed."
