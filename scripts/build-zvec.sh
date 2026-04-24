#!/usr/bin/env bash
#
# Build libzvec_c_api from the upstream zvec source tree and install it into
# a flat prefix consumable by this crate's build.rs (via `ZVEC_ROOT`).
#
# Usage:
#   scripts/build-zvec.sh [<install-prefix>]
#
# Environment overrides:
#   ZVEC_REF      — git ref to check out (default: v0.3.1, matching
#                   vendor/c_api.h in this repo).
#   ZVEC_REPO     — upstream repo URL (default: https://github.com/alibaba/zvec).
#   ZVEC_SRC_DIR  — path to an existing zvec checkout; skips cloning.
#   ZVEC_BUILD_DIR — CMake build dir (default: <workdir>/build).
#   CMAKE_GENERATOR — e.g. "Ninja" (default: "Unix Makefiles").
#   JOBS           — parallelism passed to `cmake --build` (default: nproc).

set -euo pipefail

PREFIX="${1:-$(pwd)/zvec-install}"
REF="${ZVEC_REF:-v0.3.1}"
REPO="${ZVEC_REPO:-https://github.com/alibaba/zvec.git}"
GEN="${CMAKE_GENERATOR:-Unix Makefiles}"
JOBS="${JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)}"

WORK_DIR="$(pwd)/.zvec-build-work"
mkdir -p "$WORK_DIR"

if [[ -n "${ZVEC_SRC_DIR:-}" ]]; then
    SRC_DIR="$ZVEC_SRC_DIR"
    echo ">> Using existing zvec source at $SRC_DIR"
else
    SRC_DIR="$WORK_DIR/zvec-src"
    if [[ ! -d "$SRC_DIR/.git" ]]; then
        echo ">> Cloning $REPO @ $REF into $SRC_DIR"
        git clone --depth 1 --branch "$REF" \
            --recurse-submodules --shallow-submodules \
            "$REPO" "$SRC_DIR"
    else
        echo ">> Reusing existing zvec checkout at $SRC_DIR"
    fi
fi

BUILD_DIR="${ZVEC_BUILD_DIR:-$WORK_DIR/build}"
mkdir -p "$BUILD_DIR"

echo ">> Configuring CMake (prefix=$PREFIX, generator=$GEN, jobs=$JOBS)"
cmake -S "$SRC_DIR" -B "$BUILD_DIR" \
    -G "$GEN" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$PREFIX" \
    -DBUILD_TOOLS=OFF \
    -DBUILD_PYTHON_BINDINGS=OFF

echo ">> Building zvec_c_api"
cmake --build "$BUILD_DIR" --target zvec_c_api --parallel "$JOBS"

echo ">> Copying artifacts into $PREFIX"
# Instead of `cmake --install`, which would try to install every target with
# install rules (including transitive third-party deps like googletest), just
# pick up the artifacts this crate actually needs.
mkdir -p "$PREFIX/lib" "$PREFIX/include/zvec"
LIB_SRC=""
for candidate in \
    "$BUILD_DIR/src/binding/c/libzvec_c_api.so" \
    "$BUILD_DIR/src/binding/c/libzvec_c_api.dylib" \
    "$BUILD_DIR/src/binding/c/zvec_c_api.dll"; do
    if [[ -f "$candidate" ]]; then
        LIB_SRC="$candidate"
        break
    fi
done
if [[ -z "$LIB_SRC" ]]; then
    echo "!! libzvec_c_api.{so,dylib,dll} not found under $BUILD_DIR/src/binding/c/"
    find "$BUILD_DIR" -maxdepth 4 -name 'libzvec_c_api*' -o -name 'zvec_c_api*.dll'
    exit 1
fi
cp -v "$LIB_SRC" "$PREFIX/lib/"
cp -v "$SRC_DIR/src/include/zvec/c_api.h" "$PREFIX/include/zvec/"

# Verify the layout matches what this crate's build.rs expects via ZVEC_ROOT.
if [[ ! -f "$PREFIX/lib/libzvec_c_api.so" \
   && ! -f "$PREFIX/lib/libzvec_c_api.dylib" \
   && ! -f "$PREFIX/lib/zvec_c_api.dll" ]]; then
    echo "!! Installed library not found under $PREFIX/lib; contents:"
    find "$PREFIX" -maxdepth 3 -type f | sort
    exit 1
fi

cat <<EOF

>> Built libzvec_c_api at $PREFIX

Point the crate at it:
  export ZVEC_ROOT="$PREFIX"
  export LD_LIBRARY_PATH="$PREFIX/lib\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
  cargo test

Or, if cargo only needs the runtime directory:
  export ZVEC_LIB_DIR="$PREFIX/lib"
EOF
