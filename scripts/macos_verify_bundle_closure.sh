#!/usr/bin/env bash
#
# Verify that a macOS app bundle is self-contained.
#
# For every Mach-O image in the bundle, each of its recorded dependencies must
# either resolve inside the bundle or be a library macOS itself provides.
# Anything else means the packaging dependency walker missed something, and the
# bundle will fail to load on a machine that does not happen to have the build
# environment installed.
#
# This deliberately uses `otool` and its own copy of the system-path list rather
# than anything from the packaging tool: a check that shares its inputs with the
# thing it is checking cannot disagree with it.
#
# Usage: macos_verify_bundle_closure.sh <bundle dir>

set -uo pipefail

bundle="${1:-}"
if [ -z "$bundle" ] || [ ! -d "$bundle" ]; then
    echo "usage: $0 <bundle dir>" >&2
    exit 2
fi

# Prefixes for libraries provided by the operating system. On macOS 11+ most of
# these live only in the dyld shared cache and have no file on disk.
# Intentionally excludes /usr/local and /opt/homebrew: those are Homebrew, and
# anything found there has to be bundled.
is_system_path() {
    case "$1" in
        /usr/lib/*|/System/*|/Library/Apple/*) return 0 ;;
        *) return 1 ;;
    esac
}

if ! command -v otool >/dev/null 2>&1; then
    echo "otool not found; cannot verify the bundle." >&2
    exit 2
fi

failures=0
checked=0

report() {
    echo "MISSING: $1" >&2
    failures=$((failures + 1))
}

# Skip symlinks (the versioned-dylib aliases in lib/) and dSYM payloads, which
# are parseable Mach-O images but are not loaded.
while IFS= read -r binary; do
    case "$binary" in *.dSYM/*) continue ;; esac
    # Identify Mach-O images by asking otool, rather than by sniffing magic bytes
    # in shell -- byte-literal matching across grep implementations is not worth
    # the risk of silently skipping every binary and reporting success.
    if ! otool -L "$binary" >/dev/null 2>&1; then
        continue
    fi
    deps=$(otool -L "$binary" 2>/dev/null | tail -n +2 | awk '{print $1}')
    [ -z "$deps" ] && continue
    checked=$((checked + 1))

    while IFS= read -r dep; do
        [ -z "$dep" ] && continue
        case "$dep" in
            @rpath/*|@loader_path/*|@executable_path/*)
                # Must be satisfiable from within the bundle. Checking by
                # basename rather than replaying dyld's search is deliberate:
                # this asks the weaker, independent question "is a file with
                # this name present at all", which is enough to catch a library
                # that was never copied.
                base=$(basename "$dep")
                if [ -z "$(find "$bundle" -name "$base" -print -quit 2>/dev/null)" ]; then
                    report "$dep (needed by ${binary#"$bundle"/})"
                fi
                ;;
            /*)
                if ! is_system_path "$dep"; then
                    report "$dep is an absolute path outside the bundle (needed by ${binary#"$bundle"/})"
                fi
                ;;
            *)
                # A bare relative install name resolves against the loader's
                # directory, which is fragile; flag it.
                if [ ! -f "$(dirname "$binary")/$dep" ]; then
                    report "$dep (relative install name, needed by ${binary#"$bundle"/})"
                fi
                ;;
        esac
    done <<< "$deps"
done <<< "$(find "$bundle" -type f)"

# Named checks for the libraries this verification exists to protect. Without
# these, a regression that drops the QtQuick.Controls stack again would only show
# up as a generic "MISSING" line among others.
for required in \
    libQt6QuickControls2.dylib \
    libQt6QuickTemplates2.dylib \
    libQt6QuickLayouts.dylib
do
    if [ -z "$(find "$bundle/lib" -name "$required*" -print -quit 2>/dev/null)" ]; then
        echo "MISSING REQUIRED: $required is not in $bundle/lib" >&2
        failures=$((failures + 1))
    fi
done

echo "Checked $checked Mach-O images in $bundle"
# A check that examined nothing is not a passing check.
if [ "$checked" -eq 0 ]; then
    echo "No Mach-O images were examined in $bundle; the check is inconclusive." >&2
    exit 1
fi
if [ "$failures" -gt 0 ]; then
    echo "Bundle is not self-contained: $failures problem(s)." >&2
    exit 1
fi
echo "Bundle closure verified."
