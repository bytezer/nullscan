#!/bin/sh
# Coupling guards: the CLI stays thin, the workspace crate stays editor-agnostic and the
# language server never grows a CLI parser. Run from anywhere inside the repository.
set -eu

cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

failures=0

check_absent() {
    package=$1
    crate=$2

    if cargo tree --package "$package" --prefix none --no-dedupe \
        --edges normal,build 2>/dev/null | grep -q "^$crate v"; then
        echo "error: $package must not depend on $crate" >&2
        failures=$((failures + 1))
    else
        echo "ok: $package does not depend on $crate"
    fi
}

check_absent nullscan nullscan-lsp
check_absent nullscan toml_edit
check_absent nullscan-workspace clap
check_absent nullscan-workspace lsp-server
check_absent nullscan-workspace lsp-types
check_absent nullscan-workspace toml_edit
check_absent nullscan-lsp clap

if [ "$failures" -ne 0 ]; then
    echo "error: $failures coupling guard(s) failed" >&2
    exit 1
fi

echo "ok: all coupling guards passed"
