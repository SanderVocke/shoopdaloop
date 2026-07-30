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
    case "$binary" in
        *.dSYM/*) continue ;;
        # Static archives and object files are not loadable images. `otool -L`
        # succeeds on an archive and lists its *member object files*, which look
        # exactly like unsatisfiable absolute-path dependencies.
        *.a|*.o) continue ;;
    esac
    # Identify Mach-O images by asking otool, rather than by sniffing magic bytes
    # in shell -- byte-literal matching across grep implementations is not worth
    # the risk of silently skipping every binary and reporting success.
    if ! otool -L "$binary" >/dev/null 2>&1; then
        continue
    fi
    # For a dylib, the first entry `otool -L` prints is the image's own
    # LC_ID_DYLIB install name, not a dependency. It is routinely a versioned
    # name (libFoo.6.9.1.dylib) that does not exist as a file when the library
    # was bundled under its unversioned alias, so treating it as a dependency
    # reports every single bundled dylib as missing. `otool -D` gives just that
    # name, so it can be excluded by value.
    install_name=$(otool -D "$binary" 2>/dev/null | grep -v ':$' | head -n 1 | tr -d '[:space:]')
    # Only tab-indented lines are dependencies. Header lines are unindented, and
    # on a fat binary otool emits one `path (architecture arm64):` header per
    # slice -- taking field 1 of those would invent a dependency named after the
    # file itself.
    deps=$(otool -L "$binary" 2>/dev/null | grep -E '^[[:space:]]' | awk '{print $1}')
    if [ -n "$install_name" ]; then
        deps=$(printf '%s\n' "$deps" | grep -Fxv "$install_name" || true)
    fi
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
    libQt6QuickControls2 \
    libQt6QuickTemplates2 \
    libQt6QuickLayouts
do
    # The bundled name carries a version: libQt6QuickControls2.6.dylib. Requiring
    # a digit after the dot matters -- a bare `${required}*` would also match
    # libQt6QuickControls2Basic.6.dylib and pass on the wrong library.
    if [ -z "$(find "$bundle/lib" -name "${required}.[0-9]*.dylib" -print -quit 2>/dev/null)" ]; then
        echo "MISSING REQUIRED: no ${required}.<version>.dylib in $bundle/lib" >&2
        failures=$((failures + 1))
    fi
done

# Two regular files declaring the same LC_ID_DYLIB are two copies of one library.
# dyld loads both, so Objective-C classes and Qt symbols get registered twice and
# the platform plugin ends up linked against the copy the application did not
# get. The legitimate shape is one regular file plus symlinks for its aliases.
#
# This is the exact failure that motivated the check: bundling `libQt6Core.6.dylib`
# and `libQt6Core.6.9.1.dylib` as two separate regular files produced
# "Class QMetalLayer is implemented in both ..." and an unusable bundle.
if [ -d "$bundle/lib" ]; then
    dupes=$(
        find "$bundle/lib" -maxdepth 1 -type f -name '*.dylib' 2>/dev/null | while IFS= read -r f; do
            id=$(otool -D "$f" 2>/dev/null | grep -v ':$' | head -n 1 | tr -d '[:space:]')
            [ -n "$id" ] && printf '%s\n' "$id"
        done | sort | uniq -d
    )
    if [ -n "$dupes" ]; then
        printf '%s\n' "$dupes" | while IFS= read -r id; do
            echo "DUPLICATE LIBRARY: more than one regular file declares install name $id" >&2
        done
        failures=$((failures + 1))
    fi
fi

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
