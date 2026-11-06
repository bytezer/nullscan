#!/bin/sh
# Fails unless the release tag matches the Rust workspace and the VS Code extension.
#
# Usage: scripts/check-version.sh v0.1.0
set -eu

usage() {
    echo "usage: $(basename "$0") <tag>   e.g. v0.1.0 or v0.1.0-beta.1" >&2
    exit 2
}

repo_root() {
    script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
    CDPATH= cd -- "$script_dir/.." && pwd
}

# Reads `version` from the `[workspace.package]` table only.
cargo_version() {
    awk '
        /^[[:space:]]*\[/ {
            in_section = ($0 ~ /^[[:space:]]*\[workspace\.package\][[:space:]]*$/)
            next
        }
        in_section && /^[[:space:]]*version[[:space:]]*=/ {
            value = $0
            sub(/^[^=]*=[[:space:]]*/, "", value)
            sub(/[[:space:]]*(#.*)?$/, "", value)
            gsub(/"/, "", value)
            print value
            exit
        }
    ' "$1"
}

# Reads the top-level `version` field of package.json (first match).
package_json_version() {
    sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | head -n 1
}

[ "$#" -eq 1 ] || usage
tag=$1
[ -n "$tag" ] || usage

version=${tag#v}
if [ -z "$version" ]; then
    echo "error: tag '$tag' has no version after the leading 'v'." >&2
    exit 2
fi

root=$(repo_root)
cargo_toml="$root/Cargo.toml"
package_json="$root/editors/vscode/package.json"

[ -f "$cargo_toml" ] || { echo "error: missing $cargo_toml" >&2; exit 1; }
[ -f "$package_json" ] || { echo "error: missing $package_json" >&2; exit 1; }

cargo_declared=$(cargo_version "$cargo_toml")
if [ -z "$cargo_declared" ]; then
    echo "error: $cargo_toml has no [workspace.package].version." >&2
    exit 1
fi

extension_declared=$(package_json_version "$package_json")
if [ -z "$extension_declared" ]; then
    echo "error: $package_json has no top-level \"version\" field." >&2
    exit 1
fi

status=0
if [ "$version" != "$cargo_declared" ]; then
    echo "error: tag '$tag' does not match [workspace.package].version '$cargo_declared' in Cargo.toml." >&2
    echo "       Bump the workspace version and tag again." >&2
    status=1
fi
if [ "$version" != "$extension_declared" ]; then
    echo "error: tag '$tag' does not match \"version\" '$extension_declared' in editors/vscode/package.json." >&2
    echo "       Bump the extension version and tag again." >&2
    status=1
fi

if [ "$status" -ne 0 ]; then
    exit 1
fi

echo "ok: tag '$tag' matches Cargo.toml and editors/vscode/package.json (version $version)."
