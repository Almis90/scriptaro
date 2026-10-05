# Contributing

Use a current stable Rust toolchain with rustfmt and Clippy. Node.js 24 and npm
are required only for the documentation site.

## Rust changes

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p scriptaro-example
```

Keep platform APIs in the platform backend and application-specific recipes in
examples/integrations. Core and engine behavior must remain platform-neutral.
Test behavioral changes with a simulated backend; `cargo test` must not control
the desktop. See [architecture](docs/architecture.md) for the interface contract.

## Documentation changes

```sh
npm ci
npm run docs:dev
```

Build and inspect the complete site, including Rustdoc, before submitting:

```sh
npm run docs:build
npm run docs:preview
```

The preview is at `http://127.0.0.1:4173/scriptaro/`. The build fails on broken
guide links and missing generated API pages. API links require the complete
preview, not the guide-only development server.

Source is public, but package and documentation publishing remain disabled for
routine development. Do not add automatic publish/deploy triggers. The manual
Pages workflow is reserved for an explicit future hosting request. See
[the documentation workflow](docs/guide/development.md#publishing-policy).
