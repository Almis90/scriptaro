<script setup>
import PlaybackDemo from './.vitepress/theme/PlaybackDemo.vue'
</script>

# Interactive playground

Play the four-step notes example, pause it, and change the pace. This browser
simulation illustrates the sequence; it does not execute the Rust engine or
control any desktop applications.

<PlaybackDemo />

## Run the real engine

The Rust host example uses the same recipe with `RecordingBackend`:

```sh
cargo run --locked -p scriptaro-example
```

It runs on macOS, Windows, and Linux without desktop permissions. For live
automation, use the CLI with one of the [prepared recipes](/guide/examples).

<<< ../example/sequence.yaml
