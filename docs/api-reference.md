<script setup>
import { withBase } from 'vitepress'
const crates = [
  ['scriptaro_core', 'Scripts, actions, validation, YAML parsing, and shared starter recipes.'],
  ['scriptaro_platform', 'Backend contract, capabilities, errors, and simulation.'],
  ['scriptaro_engine', 'Playback, timing, control handles, and progress events.'],
  ['scriptaro_platform_macos', 'Native macOS backend.'],
  ['scriptaro_desktop', 'Portable document and playback session model for desktop hosts.'],
]
</script>

# Rust API reference

`npm run docs:build` generates the guides and runs `cargo doc` for the workspace.
These links open the bundled Rust reference in a separate tab; they do not depend
on a published documentation site or docs.rs release.

<div class="api-links">
  <a v-for="[name, description] in crates" :key="name"
     :href="withBase(`/api/${name}/index.html`)" target="_blank" rel="noopener">
    {{ name }}
    <small>{{ description }}</small>
  </a>
</div>

The API reference is available in the complete build served with
`npm run docs:preview`. The fast `docs:dev` server serves guides only.
Rustdoc follows the build host's target configuration; building on macOS includes
the macOS-specific implementation. The documentation CI job uses macOS for this
reason.

To generate only the Rust reference:

```sh
cargo doc --workspace --no-deps --locked --open
```
