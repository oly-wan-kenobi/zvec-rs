# Publishing to crates.io

Two release paths depending on why you're cutting one:

1. **Automatic — a new upstream zvec release.** The
   [`Track upstream zvec` workflow](.github/workflows/upstream-track.yml)
   polls PyPI daily, bumps our pins when upstream ships, runs the
   bundled-feature matrix, and hands off to `release.yml` to publish.
   No human action needed when CI is green.
2. **Manual — anything else** (fix that isn't tied to a zvec bump,
   hotfix from a laptop, initial publish of a new sibling crate).
   Push a `v*.*.*` git tag and the
   [`Release` workflow](.github/workflows/release.yml) takes over.

In both cases the `Release` workflow publishes `zvec-derive` first,
waits for the crates.io sparse index to pick it up, then publishes
`zvec`, and finally cuts a GitHub Release from the matching
`CHANGELOG.md` section.

## One-time setup

1. Generate a crates.io API token at <https://crates.io/settings/tokens>.
   Recommended scopes:
   - **First release** (when the crate names don't exist yet on
     crates.io): `publish-new` **and** `publish-update`.
     `publish-update` alone fails the first publish with
     `403 Forbidden: this token does not have the required permissions
     to perform this action`.
   - **Subsequent releases** (both crates already live): just
     `publish-update`, restricted to `zvec` and `zvec-derive`.
2. Add it to the repo at *Settings → Secrets and variables → Actions
   → New repository secret*:
   - **Name:** `CARGO_REGISTRY_TOKEN`
   - **Value:** the token from step 1.
3. If you used an init-only token for step 1, rotate to a
   `publish-update`-only token once both crates are live.

## Automatic upstream tracking

Runs on two triggers:

- **Schedule:** daily at 06:00 UTC.
- **Manual:** `workflow_dispatch` with an optional `version` input
  (useful for forcing a specific zvec release, e.g. tracking a
  pre-release).

Pipeline (all in one workflow run):

1. `detect` — query `https://pypi.org/pypi/zvec/json`; compare with
   our pin in `scripts/build-zvec.sh`.
2. `bump` — if there's something newer, run
   [`scripts/bump-zvec-pin.sh`](scripts/bump-zvec-pin.sh) against it.
   That script:
   - downloads each target's cp311 wheel from PyPI and verifies its
     SHA-256 against PyPI's digest,
   - extracts `c_api.h` (identical across platforms) into
     `vendor/c_api.h`,
   - rewrites the URLs + SHAs in `build.rs`, the pin in
     `scripts/build-zvec.sh`, the `ZVEC_REF` in `ci.yml`, and both
     `Cargo.toml`s (root + `zvec-derive/`),
   - bumps zvec-rs by one patch level and prepends a
     `CHANGELOG.md` entry,
   - then commits on `auto-bump/zvec-v<X.Y.Z>` and pushes.
3. `test` — the same matrix `ci.yml` runs (ubuntu-22.04 + macos-14:
   rustfmt, clippy, build, test) against the auto-bump branch.
4. `merge-and-tag` — fast-forward main to the bump commit and push
   the `vX.Y.Z` tag.
5. `publish` — invokes `release.yml` via `workflow_call`.

**On failure** the bump branch stays on origin and the workflow opens
a PR so someone can investigate. Common triggers:

- Upstream added an enum variant (`zvec_data_type_t`, `zvec_error_code_t`)
  that our tests assert on — usually a one-line test update.
- Upstream changed a C signature; the `sys` module re-binds cleanly
  but a safe wrapper won't compile.
- Upstream stopped publishing a cp311 wheel for one of the targets;
  `bump-zvec-pin.sh` errors out early before touching any files.

### Running the bump by hand

```sh
./scripts/bump-zvec-pin.sh 0.3.2
cargo test --features bundled       # optional sanity check
git checkout -b auto-bump/zvec-v0.3.2
git commit -am "chore(deps): track zvec 0.3.2"
git push -u origin auto-bump/zvec-v0.3.2
```

Then open a PR. Once that PR is merged, tag `v<crate-version>`
(`bump-zvec-pin.sh` already set `Cargo.toml` to that version) and
push.

## Manual release (tag-driven)

For anything unrelated to an upstream zvec bump:

```sh
# 1. Bump both Cargo.toml versions (root + zvec-derive) and the
#    path-dep pin:
#       zvec-derive = { version = "X.Y", path = "zvec-derive", optional = true }
#    and promote [Unreleased] to [X.Y.Z] — DATE in CHANGELOG.md.
#
# 2. Open as a normal PR; merge when CI is green.
#
# 3. From main, tag and push:
git pull origin main
git tag -a vX.Y.Z -m "zvec-rs X.Y.Z"
git push origin vX.Y.Z
```

That push triggers the `Release` workflow at
<https://github.com/oly-wan-kenobi/zvec-rs/actions/workflows/release.yml>.
Usually finishes in 2–3 minutes (most of which is waiting for the
crates.io index to surface `zvec-derive` before `zvec` can depend on
it).

## What `release.yml` does

1. Checks out the tagged commit.
2. Verifies the tag matches the version in both `Cargo.toml`s.
3. `cargo publish -p zvec-derive`.
4. Polls `https://index.crates.io/zv/ec/zvec-derive` until the new
   version shows up (timeout ~5 min).
5. `cargo publish -p zvec --features bundled` — the `bundled` feature
   gives `cargo`'s verify step a `libzvec_c_api` to link against
   without any external setup.
6. Extracts the matching `## [X.Y.Z]` block from `CHANGELOG.md` and
   creates a GitHub Release named `zvec X.Y.Z` with that body.

The workflow accepts two triggers: a `v*.*.*` tag push (manual flow)
and `workflow_call` (what the upstream-track pipeline uses).

## Last-resort hotfix from a laptop

If you ever need to publish without the workflow at all:

```sh
cargo login                       # one-time, with your crates.io token
cargo publish -p zvec-derive
# wait ~30-90s for the index to catch up
cargo publish -p zvec --features bundled
git tag -a vX.Y.Z -m "zvec-rs X.Y.Z" && git push origin vX.Y.Z
gh release create vX.Y.Z --notes-from-tag
```

## Post-publish verification

- <https://crates.io/crates/zvec> shows the new version.
- <https://docs.rs/zvec> renders. docs.rs runs the build under
  `DOCS_RS=1`, so `build.rs` short-circuits the bundled-wheel fetch
  and only runs `bindgen`; linking is skipped.
- README badges on GitHub resolve (CI, docs.rs, crates.io version).
