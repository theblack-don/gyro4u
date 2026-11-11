#!/usr/bin/env bash
#
# Build ds4u with Nix and update the copy installed in the user profile.
# Run from anywhere; the repository is resolved from the script location.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROFILE_PACKAGE="ds4u"

cd "$REPO_DIR"

# Nix only sees files tracked by git; untracked build inputs would be
# silently missing from the build.
untracked="$(git ls-files --others --exclude-standard -- src assets build.rs Cargo.toml Cargo.lock flake.nix flake.lock)"
if [ -n "$untracked" ]; then
    echo "error: these build inputs are untracked and would NOT be visible to Nix:" >&2
    printf '%s\n' "$untracked" | sed 's/^/  /' >&2
    echo "       run 'git add -N <file>' (or 'git add <file>') and try again." >&2
    exit 1
fi

echo "==> nix build .# -L"
nix build .# -L

echo "==> updating the user profile package '$PROFILE_PACKAGE'"
profile_names="$(nix profile list 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g')"
if printf '%s\n' "$profile_names" | grep -q "^Name:[[:space:]]*$PROFILE_PACKAGE[[:space:]]*$"; then
    nix profile upgrade "$PROFILE_PACKAGE"
else
    nix profile add .#ds4u
fi

installed="$HOME/.nix-profile/bin/ds4u"
if [ -x "$installed" ]; then
    echo "==> installed: $installed"
else
    echo "error: $installed was not found after the profile update" >&2
    exit 1
fi
