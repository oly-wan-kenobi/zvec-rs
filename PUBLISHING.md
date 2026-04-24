# Publishing to crates.io

This workspace ships two crates and they must be published in a
specific order because `zvec` depends on `zvec-derive`:

1. `zvec-derive` — proc-macro crate.
2. `zvec` — the main crate. Cargo will resolve `zvec-derive` from
   crates.io, so step 1 must have landed and indexed first.

## Prerequisites

- Signed in to crates.io with a token in scope: `cargo login`.
- Clean working tree on `main` at the release commit.
- `vendor/c_api.h`, `ZVEC_REF` in `scripts/build-zvec.sh` and the
  pinned wheels in `build.rs` all agree on the same zvec version.
- Version bumped in both `Cargo.toml` and `zvec-derive/Cargo.toml`
  and matched in the `zvec-derive` path-dep: `zvec-derive = {
  version = "X.Y", path = "zvec-derive", optional = true }`.
- `CHANGELOG.md` has a dated section for this version.

## One-time prep (already done for 0.1.0)

- Metadata in both `Cargo.toml`s: `description`, `license`,
  `repository`, `documentation`, `homepage`, `readme`, `keywords`,
  `categories`, `rust-version`.
- `exclude` list on the main crate trims `.github/`, `target/`,
  `scripts/`, `CHANGELOG.md`, `PUBLISHING.md`, and the nested
  `zvec-derive/target` so only the library and examples ship.
- docs.rs metadata (`all-features = true`, `docsrs` cfg) so the
  published crate renders every feature with proper badges.

## Dry-run each crate

Dry-runs don't hit the index and don't produce an uploadable
tarball — they just check that everything packages cleanly.

```sh
# Proc-macro subcrate — no platform-specific linking, so the verify
# step is cheap.
cargo publish -p zvec-derive --dry-run

# Main crate. This will FAIL the first time you run it before
# zvec-derive has been published: cargo resolves zvec-derive from
# crates.io, and until it's there, the dependency can't be found.
# Re-run after step 2 of the real publish below.
cargo publish -p zvec --dry-run
```

## Real publish

```sh
# 1. Publish zvec-derive first.
cargo publish -p zvec-derive

# 2. Wait for crates.io to index it (usually 30-60 seconds). You can
# verify by watching:
#     https://crates.io/crates/zvec-derive
# or polling:
curl -sI https://static.crates.io/crates/zvec-derive/zvec-derive-0.1.0.crate | head -1

# 3. Publish zvec.
cargo publish -p zvec
```

If step 3 fails with `no matching package named zvec-derive found`,
the index hasn't caught up yet — wait another 30 seconds and retry.

## Tag + GitHub release

```sh
git tag -a v0.1.0 -m "zvec 0.1.0"
git push origin v0.1.0
```

Then on GitHub: *Releases → Draft a new release → tag `v0.1.0`*, copy
the CHANGELOG section into the description, and publish.

## Post-publish verification

- <https://crates.io/crates/zvec> shows 0.1.0.
- <https://docs.rs/zvec> renders. docs.rs runs the build under
  `DOCS_RS=1`, so `build.rs` short-circuits the bundled-wheel fetch
  and only runs `bindgen`; linking is skipped. Expect the doc build
  to finish in under 60 s.
- README badges on GitHub resolve (CI, docs.rs, crates.io version).

## Bumping for the next release

```sh
# Update the version in both Cargo.tomls and the path-dep version
# pin, then:
cargo update --workspace
# Move CHANGELOG's [Unreleased] into a new dated section.
```
