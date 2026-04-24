# Publishing to crates.io

Releases are cut by **pushing a `v*.*.*` git tag**. The
[`Release` workflow](.github/workflows/release.yml) takes it from
there: publishes both crates to crates.io in the right order, waits
for the index to catch up, and creates a GitHub Release with the
matching `CHANGELOG.md` section.

## One-time setup

1. Generate a crates.io API token at <https://crates.io/settings/tokens>.
   Recommended scopes:
   - **First release** (when the crate names aren't on crates.io yet):
     `publish-new` **and** `publish-update`. `publish-update` alone
     fails the first publish with
     `403 Forbidden: this token does not have the required permissions
     to perform this action`, because the crates don't exist yet.
   - **Subsequent releases** (both crates are already on crates.io):
     `publish-update`, optionally restricted to `zvec` and
     `zvec-derive`.
2. Add it to the repo at *Settings → Secrets and variables → Actions
   → New repository secret*:
   - **Name:** `CARGO_REGISTRY_TOKEN`
   - **Value:** the token from step 1.
3. If you used an init-only token for step 1, rotate to a
   `publish-update`-only token once both crates are live.

## Cutting a release

```sh
# 1. Bump the version in BOTH Cargo.toml files (root + zvec-derive),
#    plus the path-dep version pin in the root:
#       zvec-derive = { version = "X.Y.Z", path = "zvec-derive", optional = true }
#    and update CHANGELOG.md: rename [Unreleased] to [X.Y.Z] — DATE.
#
# 2. Open the bump as a normal PR; merge to main once CI is green.
#
# 3. From main, tag and push:
git pull origin main
git tag -a vX.Y.Z -m "zvec X.Y.Z"
git push origin vX.Y.Z
```

That last `git push` triggers the `Release` workflow on
<https://github.com/oly-wan-kenobi/zvec-rs/actions/workflows/release.yml>.
Watch it from there — it usually finishes in 2-3 minutes (most of
that is waiting for the crates.io index to surface `zvec-derive`
before publishing `zvec`).

## What the workflow does

1. Checks out the tagged commit.
2. Verifies the tag matches the version in both `Cargo.toml`s.
3. `cargo publish -p zvec-derive`.
4. Polls `https://index.crates.io/zv/ec/zvec-derive` until the new
   version shows up (timeout ~5 min).
5. `cargo publish -p zvec --features bundled` — the `bundled`
   feature gives `cargo`'s verify step a `libzvec_c_api` to link
   against without any extra runner setup.
6. Extracts the matching `## [X.Y.Z]` block from `CHANGELOG.md` and
   creates a GitHub Release named `zvec X.Y.Z` with that body.

## Manual fallback

If you ever need to publish without the workflow (e.g. a hotfix from
your laptop), the steps are exactly what the workflow runs:

```sh
cargo login                       # one-time, with your crates.io token
cargo publish -p zvec-derive
# wait ~30-90s for the index to catch up
cargo publish -p zvec --features bundled
git tag -a vX.Y.Z -m "zvec X.Y.Z" && git push origin vX.Y.Z
gh release create vX.Y.Z --notes-from-tag
```

## Post-publish verification

- <https://crates.io/crates/zvec> shows the new version.
- <https://docs.rs/zvec> renders. docs.rs runs the build under
  `DOCS_RS=1`, so `build.rs` short-circuits the bundled-wheel fetch
  and only runs `bindgen`; linking is skipped.
- README badges on GitHub resolve (CI, docs.rs, crates.io version).
