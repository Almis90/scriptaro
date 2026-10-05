---
layout: home
hero:
  name: Scriptaro
  text: Give your desktop a script.
  tagline: Prepare a sequence, set the pace, and play it back. A general-purpose automation engine written in Rust, with native macOS control.
  image:
    src: /logo.svg
    alt: Scriptaro
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: Try the playground
      link: /playground
    - theme: alt
      text: View on GitHub
      link: https://github.com/Almis90/scriptaro
features:
  - title: Put the timing in the script
    details: Type prepared text character by character, wait between actions, switch applications, and replay the sequence at your chosen pace.
  - title: Built for any workflow
    details: Desktop actions stay generic. Tutorials, product walkthroughs, and document demonstrations use the same engine.
  - title: Native where it matters
    details: AppKit and CoreGraphics handle macOS control. The platform interface leaves room for Windows, X11, and supported Wayland operations.
  - title: Rehearse before playback
    details: Validate YAML and simulate your sequence without touching the desktop. Pause, resume, cancel, and observe the real engine through its API.
---

## One sequence. Clear steps.

<<< ../example/sequence.yaml

Run this recipe with the [Rust host example](/guide/examples#rust-host-example).
It uses a simulated desktop, so it works without installing an editor or granting
permissions. Use the [CLI](/guide/getting-started) when you are ready for live input.

::: info Development milestone
Scriptaro currently provides a CLI and reusable Rust crates. Live desktop control
is implemented on macOS. Windows and Linux support validation and simulation;
their native backends and native views are future work. A macOS desktop interface
now shares the same engine as the CLI; see [desktop usage](/guide/desktop).
:::
