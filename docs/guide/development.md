# Contributing and documentation

The project uses a Rust workspace, a separate runnable example, VitePress guides,
and a generated Rust API reference. The documentation workflow follows the
structure used by `opa_rfs`, adapted for a native Rust application.

## Verify the workspace

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p scriptaro-example
```

CI runs Rust checks on macOS, Windows, and Linux. Tests use simulated backends
and do not type into the runner's desktop. A separate macOS documentation job
builds guides and native API docs without deploying them.

## Work on guides

Use Node.js 24 and npm, along with Rust on your `PATH`:

```sh
npm ci
npm run docs:dev
```

VitePress serves the site at the `/scriptaro/` base path. Guides, local search,
and the browser playground update during development.

## Build the complete documentation

```sh
npm run docs:build
npm run docs:preview
```

Open `http://127.0.0.1:4173/scriptaro/`. The static output lives in
`docs/.vitepress/dist/` and contains VitePress guides, the browser simulation,
generated API files under `api/`, and a `.nojekyll` marker. The build never
deploys or publishes anything.

## Publishing policy

The source repository is public. Package registries, release binaries, and a
hosted documentation site are not part of this milestone.

- Workspace packages inherit `publish = false`, preventing `cargo publish`.
- The documentation npm package is private.
- There is no package-publish or release workflow.
- `.github/workflows/pages.yml` has only a manual trigger. Its publish input
  defaults to false, and both build and deploy jobs are gated by that input.
- GitHub Pages is not enabled, and pushes/PRs only verify the project.

If documentation hosting is explicitly wanted later, enable GitHub Pages with
GitHub Actions as the source, then manually run **Deploy documentation (manual
only)** on `main` with the publish input enabled. This is a future opt-in path;
the current workflow must not be dispatched to deploy during routine development.

## Optional native test

On a prepared macOS desktop:

```sh
cargo build --locked
python3 tests/macos/smoke.py target/debug/scriptaro
```

This builds a disposable Cocoa text window, verifies native activation/file
opening/shortcuts/Unicode typing, and closes the window. It temporarily changes
focus and requires Accessibility access. It is intentionally outside CI.
