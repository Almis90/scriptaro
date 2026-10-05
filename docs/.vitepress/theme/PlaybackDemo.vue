<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'

const text = 'A small idea, written one character at a time.\nScriptaro keeps the timing in the script.'
const characters = Array.from(text)
const stages = [
  { name: 'Activate Demo Notes', kind: 'activate_app', duration: 400 },
  { name: 'Type the prepared text', kind: 'type_text', duration: (characters.length - 1) * 45 },
  { name: 'Wait one second', kind: 'wait', duration: 1000 },
  { name: 'Press the save shortcut', kind: 'key_press', duration: 200 },
]
const step = ref(0)
const elapsed = ref(0)
const state = ref<'ready' | 'playing' | 'paused' | 'completed'>('ready')
const speed = ref(1)
let timer: ReturnType<typeof setInterval> | undefined
let previous = 0
const documentText = computed(() => step.value > 1 ? text : step.value === 1
  ? characters.slice(0, Math.min(characters.length, 1 + Math.floor(elapsed.value / 45))).join('') : '')
const status = computed(() => ({ ready: 'Ready to play', playing: 'Playing', paused: 'Paused', completed: 'Sequence complete' })[state.value])
const progress = computed(() => step.value >= stages.length ? 100 : Math.round(
  (step.value + elapsed.value / stages[step.value].duration) / stages.length * 100))

function stopTimer() {
  if (timer !== undefined) clearInterval(timer)
  timer = undefined
}
function reset() {
  stopTimer()
  step.value = 0
  elapsed.value = 0
  state.value = 'ready'
}
function play() {
  if (state.value === 'playing') return
  if (state.value === 'completed') reset()
  state.value = 'playing'
  previous = performance.now()
  timer = setInterval(() => {
    const now = performance.now()
    elapsed.value += (now - previous) * speed.value
    previous = now
    while (step.value < stages.length && elapsed.value >= stages[step.value].duration) {
      elapsed.value -= stages[step.value].duration
      step.value++
    }
    if (step.value === stages.length) {
      elapsed.value = 0
      state.value = 'completed'
      stopTimer()
    }
  }, 25)
}
function pause() {
  stopTimer()
  state.value = 'paused'
}
onBeforeUnmount(stopTimer)
</script>

<template>
  <section class="demo" aria-label="Simulated desktop playback">
    <div class="toolbar">
      <div class="controls">
        <button v-if="state !== 'playing'" class="primary" @click="play">{{ state === 'paused' ? 'Resume' : state === 'completed' ? 'Replay' : 'Play sequence' }}</button>
        <button v-else class="primary" @click="pause">Pause</button>
        <button @click="reset" :disabled="state === 'ready'">Reset</button>
      </div>
      <label>Pace
        <select v-model.number="speed" aria-label="Playback speed">
          <option :value="0.5">0.5×</option><option :value="1">1×</option>
          <option :value="2">2×</option><option :value="4">4×</option>
        </select>
      </label>
    </div>
    <div class="workspace">
      <ol class="steps" aria-label="Sequence steps">
        <li v-for="(stage, index) in stages" :key="stage.kind"
            :class="{ active: index === step && state !== 'ready', done: index < step }"
            :aria-current="index === step ? 'step' : undefined">
          <span class="number" aria-hidden="true">{{ index < step ? '✓' : index + 1 }}</span>
          <div><strong>{{ stage.name }}</strong><code>{{ stage.kind }}</code></div>
        </li>
      </ol>
      <div class="document">
        <div class="document-bar"><span>Demo Notes</span><span class="badge">{{ state === 'completed' ? 'Saved' : 'Simulation' }}</span></div>
        <pre aria-label="Simulated document text"><span v-if="!documentText" class="placeholder">Your sequence will write here.</span><template v-else>{{ documentText }}</template><span v-if="state === 'playing' && step === 1" class="cursor" aria-hidden="true">▌</span></pre>
      </div>
    </div>
    <div class="footer"><span role="status" aria-live="polite">{{ status }}</span><span>{{ Math.min(step + 1, stages.length) }} / {{ stages.length }} steps</span></div>
    <div class="progress" role="progressbar" aria-label="Sequence progress" :aria-valuenow="progress" aria-valuemin="0" aria-valuemax="100"><div :style="{ width: `${progress}%` }" /></div>
  </section>
</template>

<style scoped>
.demo { margin: 28px 0; border: 1px solid var(--vp-c-divider); border-radius: 16px; overflow: hidden; background: var(--vp-c-bg); }
.toolbar { padding: 18px; display: flex; flex-wrap: wrap; align-items: center; gap: 14px; justify-content: space-between; background: var(--vp-c-bg-soft); }
.controls { display: flex; gap: 8px; }
button, select { border: 1px solid var(--vp-c-divider); border-radius: 7px; background: var(--vp-c-bg); padding: 7px 12px; font: inherit; font-size: 13px; color: var(--vp-c-text-1); }
button { cursor: pointer; font-weight: 600; min-height: 38px; }
button.primary { background: var(--vp-c-brand-3); border-color: var(--vp-c-brand-3); color: white; min-width: 122px; }
button:disabled { opacity: .45; cursor: default; }
button:focus-visible, select:focus-visible { outline: 2px solid var(--vp-c-brand-1); outline-offset: 3px; }
label { font-size: 13px; display: flex; align-items: center; gap: 8px; }
.workspace { display: grid; grid-template-columns: 1fr 1.2fr; }
.steps { list-style: none; margin: 0; padding: 20px 12px; border-right: 1px solid var(--vp-c-divider); }
.steps li { display: flex; align-items: center; gap: 12px; margin: 0; padding: 14px 10px; border-radius: 8px; font-size: 12px; line-height: 1.5; }
.steps strong { display: block; font-size: 13px; font-weight: 550; }
.steps code { padding: 0; background: transparent; font-size: 11px; color: var(--vp-c-text-2); }
.number { width: 26px; height: 26px; display: grid; place-items: center; flex-shrink: 0; border: 1px solid var(--vp-c-divider); border-radius: 50%; font-size: 11px; }
.active { background: var(--vp-c-brand-soft); }
.active .number, .done .number { color: var(--vp-c-brand-1); border-color: var(--vp-c-brand-1); }
.document { min-width: 0; }
.document-bar { display: flex; flex-wrap: wrap; gap: 8px; justify-content: space-between; padding: 18px; border-bottom: 1px solid var(--vp-c-divider); font-size: 12px; font-weight: 600; }
.badge { font-weight: 400; color: var(--vp-c-text-2); font-size: 11px; }
pre { padding: 22px 18px; margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; min-height: 220px; font-size: 13px; line-height: 1.85; font-family: var(--vp-font-family-mono); }
.placeholder { color: var(--vp-c-text-2); font-size: 12px; }
.cursor { color: var(--vp-c-brand-1); }
.footer { display: flex; justify-content: space-between; gap: 8px; padding: 12px 18px; border-top: 1px solid var(--vp-c-divider); color: var(--vp-c-text-2); font-size: 12px; }
.progress { height: 3px; background: var(--vp-c-bg-soft); }
.progress > div { height: 100%; background: var(--vp-c-brand-1); }
@media (max-width: 640px) { .workspace { grid-template-columns: 1fr; } .steps { border-right: 0; border-bottom: 1px solid var(--vp-c-divider); padding: 12px; } .steps li { padding: 9px; } pre { min-height: 170px; } }
</style>
