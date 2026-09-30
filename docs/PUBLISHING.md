# Automated publishing

Every push to `main` runs `.github/workflows/release.yml`. It reserves a new
patch version, tests the source, builds all eight native targets, validates all
nine archives, publishes to npm, verifies Salesforce CLI installation, and
creates a GitHub Release. No manual version bump or tag is needed.

## Versioning and retries

The version in `package.json` is a minimum. The workflow chooses that version
when it exceeds every reserved release, otherwise it increments the latest
reserved patch. An explicit higher version starts a new minor or major series.

A release commit with synchronized npm and Rust metadata is tagged `vX.Y.Z`.
It contains a `Release-Source` trailer identifying the source commit. The workflow
pushes only the tag, leaving `main` untouched and avoiding a release loop. Tags
are immutable. Rerunning the same source commit reuses its reserved snapshot.
Failed builds can leave gaps in version numbers.

Releases run sequentially with up to 100 pending runs. Tests and native builds
run in parallel inside each release. Publication waits for every check to pass.
If publication partially succeeds, a retry skips a package only if its registry
integrity matches the exact release artifact. Different bytes are an error.

## Authentication

GitHub Actions uses npm Trusted Publishing with short-lived OIDC credentials.
No `NPM_TOKEN` is stored in the repository. The publisher configuration for all
nine packages is:

- Owner: `CarlosIribar`
- Repository: `inheriteame`
- Workflow: `release.yml`
- Environment: `npm-release`

The environment permits `main` and has no required reviewers. Each new package
needs one initial owner-authenticated publication and Trusted Publisher setup.
For bootstrap, publish the exact validated artifacts from the tagged workflow,
then configure `npm trust github` for each package and rerun the failed publish
job. Initial local publications have no GitHub provenance; subsequent OIDC
publications request provenance automatically.

## Native packages

| Package | Rust target |
| --- | --- |
| inheriteame-linux-x64-gnu | x86_64-unknown-linux-gnu |
| inheriteame-linux-arm64-gnu | aarch64-unknown-linux-gnu |
| inheriteame-linux-x64-musl | x86_64-unknown-linux-musl |
| inheriteame-linux-arm64-musl | aarch64-unknown-linux-musl |
| inheriteame-darwin-x64 | x86_64-apple-darwin |
| inheriteame-darwin-arm64 | aarch64-apple-darwin |
| @carlosiribar/inheriteame-win32-x64 | x86_64-pc-windows-msvc |
| @carlosiribar/inheriteame-win32-arm64 | aarch64-pc-windows-msvc |

Native packages always match the parent version. The workflow publishes them
before the parent and waits for npm to serve every package before installation.
There is no install-time compilation or download script.

## Local packaging

`npm run pack:local` creates the host-native tarball and plugin in `release/local`.
`npm run verify:packages` verifies the host package metadata and executable.
`npm run pack:release` requires all eight binaries to be staged and verifies
each package before packing. Build outputs and tarballs are not committed.

## Recover a release

Inspect the failed GitHub Actions job and fix the cause. Retry failed jobs when
the source is unchanged. Push a source fix to `main` when code must change; this
reserves a new version. Never replace an existing npm version or move a release
tag. For a publication retry, reuse the existing artifacts from that run.
