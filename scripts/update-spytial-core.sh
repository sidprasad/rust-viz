#!/usr/bin/env bash
#
# Re-vendor spytial-core and regenerate everything derived from it.
#
#   scripts/update-spytial-core.sh 4.3.0
#
# Pulls the published tarball (not a local build — the tarball is what consumers
# actually get), copies the browser assets and the language manifest into
# templates/vendor/, and regenerates macros/src/spec_tables.rs from the new
# manifest.
#
# The regeneration is not a separate step you can forget: the macro's accepted
# keys and compile-time validation are derived from the manifest, so a version
# bump without it leaves the derive macro describing the old language. CI checks
# for that too (spec-codegen's drift test), but this script is what keeps the two
# in step in the first place.

set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
    echo "usage: $0 <spytial-core version>   e.g. $0 4.3.0" >&2
    exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="$REPO_ROOT/templates/vendor"

for tool in npm cargo rustfmt; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "error: $tool is required but not on PATH" >&2
        exit 1
    }
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "==> fetching spytial-core@$VERSION"
(cd "$WORK" && npm pack "spytial-core@$VERSION" >/dev/null 2>&1) || {
    echo "error: npm pack spytial-core@$VERSION failed" >&2
    exit 1
}
tar xzf "$WORK"/spytial-core-*.tgz -C "$WORK"
PKG="$WORK/package"

# Source in the tarball -> name in templates/vendor. Keep in step with the table
# in templates/vendor/README.md.
declare -a ASSETS=(
    "dist/browser/spytial-core-complete.global.js:spytial-core.global.js"
    "dist/browser/spytial-core-complete.css:spytial-core.css"
    "dist/components/react-component-integration.global.js:react-component-integration.global.js"
    "dist/components/react-component-integration.css:react-component-integration.css"
    "docs/spytial-language.json:spytial-language.json"
    "docs/spytial-spec.schema.json:spytial-spec.schema.json"
    "dist/cli/spytial-check.js:spytial-check.js"
)

for pair in "${ASSETS[@]}"; do
    src="${pair%%:*}"
    if [[ ! -f "$PKG/$src" ]]; then
        echo "error: spytial-core@$VERSION has no $src" >&2
        if [[ "$src" == docs/spytial-language.json ]]; then
            echo "       The language manifest first shipped in 4.3.0; the spec" >&2
            echo "       tables cannot be generated from an earlier release." >&2
        fi
        if [[ "$src" == docs/spytial-spec.schema.json ]]; then
            echo "       The JSON Schema first shipped beside the manifest in 4.3.0;" >&2
            echo "       tests/schema.rs has nothing to validate against before that." >&2
        fi
        if [[ "$src" == dist/cli/* ]]; then
            echo "       The conformance CLI first shipped in 4.4.1; tests/conformance.rs" >&2
            echo "       has nothing to run against an earlier release." >&2
        fi
        exit 1
    fi
done

echo "==> vendoring assets"
for pair in "${ASSETS[@]}"; do
    src="${pair%%:*}"
    dst="${pair##*:}"
    cp "$PKG/$src" "$VENDOR/$dst"
    printf '    %s\n' "$dst"
done

printf 'spytial-core %s\n' "$VERSION" > "$VENDOR/VERSION.txt"

echo "==> regenerating macros/src/spec_tables.rs and macros/src/attributes.md"
cargo run --quiet --manifest-path "$REPO_ROOT/spec-codegen/Cargo.toml"

echo
echo "==> vendored spytial-core $VERSION"
if git -C "$REPO_ROOT" rev-parse --git-dir >/dev/null 2>&1; then
    git -C "$REPO_ROOT" status --short -- templates/vendor macros/src/spec_tables.rs macros/src/attributes.md
    echo
    echo "Review the spec_tables.rs diff: it is the layout-spec language changing"
    echo "under the derive macro. New keys may need parsing and codegen in"
    echo "macros/src/lib.rs; removed or newly-deprecated ones may need migrating."
    echo "attributes.md is the same change as the user will read it in rustdoc."
fi
