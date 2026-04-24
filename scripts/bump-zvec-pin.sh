#!/usr/bin/env bash
# Rewrite every place that pins an upstream zvec version to point at
# $NEW_VERSION, and bump our own crate versions by a patch level.
#
# Consumed by .github/workflows/upstream-track.yml, but is also
# usable from a laptop — see the bottom of the script for the exact
# side-effects.
#
# Usage: scripts/bump-zvec-pin.sh <new-zvec-version>
#   e.g. scripts/bump-zvec-pin.sh 0.3.2
#
# Requires: curl, jq, unzip, sha256sum (or shasum -a 256 on macOS),
# sed, awk, a working `cargo` on PATH (for the resulting `cargo fmt`).

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <new-zvec-version>" >&2
  exit 2
fi
NEW="$1"

# Targets that must be kept in lockstep with `build.rs::select_wheel`.
# Keep this mapping sorted alphabetically so diffs stay readable.
declare -a TARGETS=(
  "x86_64-unknown-linux-gnu:manylinux_2_28_x86_64"
  "aarch64-unknown-linux-gnu:manylinux_2_28_aarch64"
  "aarch64-apple-darwin:macosx_11_0_arm64"
  "x86_64-pc-windows-msvc:win_amd64"
)

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

# ---------- helpers ----------------------------------------------------------

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

current_pin() {
  # Source of truth for "the zvec version we currently track".
  grep -m1 '^REF=' scripts/build-zvec.sh | sed -E 's/^REF="\$\{ZVEC_REF:-v([0-9.]+)\}"$/\1/'
}

bump_patch() {
  # 0.1.7 -> 0.1.8.  Errors out on non-x.y.z input.
  python3 - "$1" <<'PY'
import sys
major, minor, patch = (int(p) for p in sys.argv[1].split("."))
print(f"{major}.{minor}.{patch + 1}")
PY
}

# ---------- pre-flight checks ------------------------------------------------

for bin in curl jq unzip sed awk python3 cargo; do
  command -v "$bin" >/dev/null 2>&1 || {
    echo "error: missing required tool: $bin" >&2
    exit 1
  }
done

OLD_ZVEC="$(current_pin)"
if [[ -z "$OLD_ZVEC" ]]; then
  echo "error: could not detect the currently-pinned zvec version" >&2
  exit 1
fi
if [[ "$OLD_ZVEC" == "$NEW" ]]; then
  echo "no-op: already pinned to zvec $NEW"
  exit 0
fi

OLD_SELF="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
NEW_SELF="$(bump_patch "$OLD_SELF")"

echo "bumping zvec    $OLD_ZVEC -> $NEW"
echo "bumping zvec-rs $OLD_SELF -> $NEW_SELF"

# ---------- 1. fetch PyPI metadata ------------------------------------------

pypi_json="$(mktemp)"
trap 'rm -f "$pypi_json"' EXIT
curl -sSfL "https://pypi.org/pypi/zvec/$NEW/json" -o "$pypi_json"

lookup_wheel() {
  # Given a wheel-tag substring (e.g. manylinux_2_28_x86_64), print
  # "<url>\t<expected sha256>" from PyPI's JSON metadata.
  local tag="$1"
  jq -r --arg ver "$NEW" --arg tag "$tag" '
    .urls[]
    | select(.packagetype == "bdist_wheel")
    | select(.filename | contains("zvec-\($ver)-cp311-cp311-\($tag).whl"))
    | "\(.url)\t\(.digests.sha256)"
  ' "$pypi_json" | head -1
}

# Download each wheel, verify SHA, extract c_api.h from the first one.
wheels_dir="$(mktemp -d)"
trap 'rm -rf "$wheels_dir"; rm -f "$pypi_json"' EXIT

declare -A WHEEL_URL=()
declare -A WHEEL_SHA=()
for row in "${TARGETS[@]}"; do
  triple="${row%%:*}"
  tag="${row##*:}"
  meta="$(lookup_wheel "$tag")"
  if [[ -z "$meta" ]]; then
    echo "error: PyPI has no cp311 wheel for zvec $NEW with tag $tag" >&2
    exit 1
  fi
  url="$(printf '%s' "$meta" | cut -f1)"
  want_sha="$(printf '%s' "$meta" | cut -f2)"

  file="$wheels_dir/$(basename "$url")"
  curl -sSfL "$url" -o "$file"
  got_sha="$(sha256 "$file")"
  if [[ "$got_sha" != "$want_sha" ]]; then
    echo "error: sha mismatch for $url: wanted $want_sha got $got_sha" >&2
    exit 1
  fi
  WHEEL_URL["$triple"]="$url"
  WHEEL_SHA["$triple"]="$want_sha"
  echo "  ok: $triple $(basename "$url") sha=${want_sha:0:12}…"
done

# Grab c_api.h from the linux x86 wheel (header is identical across
# platforms for a given zvec version).
linux_x86="$wheels_dir/$(basename "${WHEEL_URL[x86_64-unknown-linux-gnu]}")"
unzip -p "$linux_x86" include/zvec/c_api.h > vendor/c_api.h.new
mv vendor/c_api.h.new vendor/c_api.h
echo "  ok: vendor/c_api.h refreshed"

# ---------- 2. rewrite source files -----------------------------------------

replace_line() {
  # Replace the *line* matching $1 in file $3 with literal $2. Anchored
  # with grep first so a missed match is a loud error, not a silent drop.
  local pattern="$1"
  local replacement="$2"
  local file="$3"
  if ! grep -qE "$pattern" "$file"; then
    echo "error: pattern not found in $file: $pattern" >&2
    exit 1
  fi
  python3 - "$pattern" "$replacement" "$file" <<'PY'
import re, sys
pattern, replacement, file = sys.argv[1], sys.argv[2], sys.argv[3]
with open(file) as f:
    text = f.read()
new, n = re.subn(f"(?m)^.*{pattern}.*$", replacement, text, count=1)
assert n == 1, f"expected exactly one match for {pattern!r} in {file!r}, got {n}"
with open(file, "w") as f:
    f.write(new)
PY
}

# scripts/build-zvec.sh — the canonical pin.
replace_line \
  '^#   ZVEC_REF' \
  "#   ZVEC_REF      — git ref to check out (default: v$NEW, matching" \
  scripts/build-zvec.sh
replace_line \
  '^REF=' \
  "REF=\"\${ZVEC_REF:-v$NEW}\"" \
  scripts/build-zvec.sh

# .github/workflows/ci.yml — a duplicate pin for the source-build job.
replace_line \
  '^  ZVEC_REF: ' \
  "  ZVEC_REF: v$NEW" \
  .github/workflows/ci.yml

# build.rs — the "Pinned wheels" comment + each target's URL/SHA.
replace_line \
  'Pinned wheels for zvec' \
  "    /// Pinned wheels for zvec $NEW, matching the header vendored at" \
  build.rs
for triple in "${!WHEEL_URL[@]}"; do
  url="${WHEEL_URL[$triple]}"
  sha="${WHEEL_SHA[$triple]}"
  python3 - "$triple" "$url" "$sha" <<'PY'
import re, sys
triple, url, sha = sys.argv[1], sys.argv[2], sys.argv[3]
path = "build.rs"
with open(path) as f:
    text = f.read()
# Match the 3-line block for this target:
#   "TRIPLE" => Wheel {
#       url: "…",
#       sha256: "…",
#   },
pattern = re.compile(
    r'(?m)^(\s*)"' + re.escape(triple) + r'" => Wheel \{\n'
    r'\s+url: "[^"]+",\n'
    r'\s+sha256: "[^"]+",\n'
    r'\s+\},\n'
)
replacement = (
    f'            "{triple}" => Wheel {{\n'
    f'                url: "{url}",\n'
    f'                sha256: "{sha}",\n'
    f'            }},\n'
)
new, n = pattern.subn(replacement, text, count=1)
assert n == 1, f"did not find exactly one {triple} block in {path}"
with open(path, "w") as f:
    f.write(new)
PY
done

# Cargo.toml + zvec-derive/Cargo.toml — bump our own version.
replace_line \
  '^version = ' \
  "version = \"$NEW_SELF\"" \
  Cargo.toml
replace_line \
  '^version = ' \
  "version = \"$NEW_SELF\"" \
  zvec-derive/Cargo.toml
# Cargo.toml's internal path-dep pin:
replace_line \
  '^zvec-derive = \{ version' \
  "zvec-derive = { version = \"${NEW_SELF%.*}\", path = \"zvec-derive\", optional = true }" \
  Cargo.toml

# CHANGELOG.md — promote [Unreleased] to [NEW_SELF] with today's date and
# seed a fresh [Unreleased] above it.
today="$(date -u +%Y-%m-%d)"
python3 - "$NEW_SELF" "$today" "$NEW" "$OLD_ZVEC" <<'PY'
import re, sys
ver, today, zvec_new, zvec_old = sys.argv[1:]
with open("CHANGELOG.md") as f:
    text = f.read()

# Insert/update release section
if "## [Unreleased]" not in text:
    raise SystemExit("CHANGELOG.md is missing an [Unreleased] header")
note = (
    f"### Changed\n\n"
    f"- Track zvec {zvec_new} (was {zvec_old}). See upstream's release notes.\n\n"
)
text = text.replace(
    "## [Unreleased]\n",
    f"## [Unreleased]\n\n## [{ver}] — {today}\n\n{note}",
    1,
)

# Add compare link at the bottom
def link(v): return f"[{v}]: https://github.com/oly-wan-kenobi/zvec-rs/releases/tag/v{v}"
lines = text.rstrip().splitlines()
# Replace the "Unreleased" compare link to point at the new tag.
for i, line in enumerate(lines):
    if line.startswith("[Unreleased]:"):
        lines[i] = f"[Unreleased]: https://github.com/oly-wan-kenobi/zvec-rs/compare/v{ver}...HEAD"
        break
# Insert the new version's tag link just below [Unreleased]:.
for i, line in enumerate(lines):
    if line.startswith("[Unreleased]:"):
        lines.insert(i + 1, link(ver))
        break
with open("CHANGELOG.md", "w") as f:
    f.write("\n".join(lines) + "\n")
PY

# ---------- 3. formatting + sanity check ------------------------------------

cargo fmt --all
# Cargo.lock gets updated by a subsequent cargo invocation; the workflow
# runs `cargo build`/`cargo test` after this script so the lockfile
# naturally refreshes there.

cat <<SUMMARY
------------------------------------------------------------
Bumped:
  zvec:         $OLD_ZVEC -> $NEW  (vendor/c_api.h + build.rs + scripts/build-zvec.sh + ci.yml)
  zvec-rs:      $OLD_SELF -> $NEW_SELF
Review with: git diff --stat
------------------------------------------------------------
SUMMARY
