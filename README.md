# inheriteame

A Salesforce CLI plugin that adds `inherited sharing` to top-level Apex classes
without an explicit sharing declaration. Rust performs parsing, configuration,
file selection and edits; TypeScript provides the Salesforce CLI command.

Existing `with sharing`, `without sharing` and `inherited sharing` declarations
are preserved. Inner classes, interfaces, enums and triggers are unchanged.
The plugin does not change DML, queries or other source content.

## Install

```sh
sf plugins install inheriteame
```

The correct precompiled native package is installed automatically.

## Local installation

Requirements: Node.js 20.17+, npm, Rust 1.98.0 and Salesforce CLI.

```sh
npm ci --omit=optional
npm run build
npm run build:native
sf plugins link .
```

Then, from a Salesforce project:

```sh
sf apex inherited-sharing fix --dry-run
sf apex inherited-sharing fix
sf apex inherited-sharing fix --all --dry-run
sf apex inherited-sharing fix --all --check
```

Published packages use precompiled optional native dependencies, without a Rust
installation or install-time downloads or compilation scripts.

## Selection and output

By default, the command processes staged added, copied, modified and renamed
`.cls` files within `packageDirectories` from `sfdx-project.json`. It reads and
edits working-tree content, never the Git index. Partially staged files cause an
error before any writes; `--allow-unstaged` explicitly permits processing their
working-tree content. Files excluded by configuration or a header marker do not
trigger this protection.

`--all` selects every `.cls` within the package directories and does not require
Git. Paths are deduplicated; symlinks are not traversed. Package directories must
be inside the Salesforce project. `.git` and `node_modules` are not traversed.
`--project-dir PATH` locates the project from that directory or its parents.

- `--dry-run` prints unified diffs without writing.
- `--check` reports pending corrections without writing and exits 1 when any exist.
- `--verbose` explains the decision for each selected file.
- `--quiet` suppresses ordinary human output, including diffs, but keeps errors.
- `--json` returns structured per-file decisions, proposed diffs, counts and duration.

`--check` and `--dry-run`, `--quiet` and `--verbose`, and `--all` and
`--allow-unstaged` are mutually exclusive pairs.

Exit codes: **0** successful execution, **1** analysis errors or pending corrections
in check mode, **2** configuration or execution errors. A successful dry run can
have pending corrections. An empty selection succeeds.

## Disable a file

Place this marker in a leading comment, before any code or annotations:

```apex
// apex-inherited-sharing-ignore: framework controls the sharing boundary
public class FrameworkAdapter {}
```

The marker also works in a `/* ... */` or ApexDoc header. A reason is optional.
Ordinary license comments do not disable processing. Markers in strings, after
annotations, inside classes, or embedded in a longer identifier do not count.

## Project configuration

Create `.inheriteame.json` next to `sfdx-project.json`:

```json
{
  "version": 1,
  "ignoreClassPatterns": ["*Controller", "*Test"],
  "excludeFiles": [
    "**/generated/**",
    "force-app/main/default/classes/LegacyAdapter.cls"
  ]
}
```

These are optional examples, **not defaults**. Without configuration, no class
name or path is automatically excluded, including test and controller classes.
Both arrays may be omitted and default to empty.

`ignoreClassPatterns` matches the declared top-level class name, case-insensitively.
`excludeFiles` matches case-sensitive paths relative to the Salesforce project
root, using `/` on every platform. Exact paths are supported. Glob syntax supports
`*`, `?`, character classes such as `[AB]`, and alternatives such as `{one,two}`;
`*` cannot cross directories, while `**` can. Patterns are full matches, not
substring searches. Directory exclusions should use `**/directory/**`.

Any exclusion skips the file in all command modes. Path exclusions apply before
reading; the header marker applies before parsing; class patterns apply after
extracting the top-level class name. If syntax damage prevents identifying a
class name, use a path exclusion or header marker instead.

The version must be 1. Unknown fields, malformed JSON, wrong types, malformed
globs, empty patterns, backslashes, absolute paths and `.`/`..` path segments
are rejected before writes. Class patterns cannot contain `/`.

See [the generic example](examples/inheriteame.example.json).

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm run lint
npm test
npm run test:integration
npm run pack:local
npm run verify:packages
```

The integration suite creates disposable projects and Git repositories. It checks
idempotence, exact source preservation, exclusions, staged scope, partial staging,
JSON output, dry runs, exit codes and index preservation.

## Limitations

Parsing uses tree-sitter-sfapex 3.0.1. Some valid Apex constructs may not be accepted,
including comments or unusual whitespace between the words of a sharing modifier.
Parser errors leave the file unchanged and are reported. Rewrites preserve original
bytes outside the inserted modifier and are parsed again before writing.

All files are planned before writes, but a later write failure can leave earlier
files modified; this is not a multi-file transaction. The command checks that a
file has not changed since analysis before writing it. No Salesforce org is
contacted and no Apex deployment or org compilation is performed.

## Automatic releases

Every push to `main` reserves the next patch version, runs tests, builds all eight
native targets, publishes the native packages and plugin to npm, verifies a
Salesforce CLI installation, and creates a GitHub Release. No manual version bump,
tag or release approval is required. A higher committed version sets a new minimum.
Release commits only update immutable tags; they do not change `main`.

Publication uses npm Trusted Publishing through GitHub Actions OIDC without an
npm token. Retries reuse the same release snapshot and verify any already
published artifact before skipping it.

See [automated publishing](docs/PUBLISHING.md) for authentication, platform
packages, versioning and recovery.
