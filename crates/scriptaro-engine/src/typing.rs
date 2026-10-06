use scriptaro_core::TypingTiming;

/// Fresh for every action: repeats and section retakes reproduce the same gaps.
pub(crate) struct Cadence {
    timing: TypingTiming,
    state: u64,
}
impl Cadence {
    pub(crate) fn new(timing: TypingTiming) -> Self {
        Self {
            state: timing.seed,
            timing,
        }
    }

    pub(crate) fn gap_after(&mut self, character: char) -> u64 {
        let t = self.timing;
        let interval = if t.jitter_ms == 0 {
            t.interval_ms
        } else {
            // SplitMix64 with wrapping arithmetic: deterministic across platforms.
            self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
            let mut value = self.state;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            value ^= value >> 31;
            t.interval_ms - t.jitter_ms + value % (2 * t.jitter_ms + 1)
        };
        let pause = if character == '\n' {
            t.line_pause_ms
        } else if character.is_whitespace() {
            t.word_pause_ms
        } else if matches!(character, '.' | ',' | '!' | '?' | ';' | ':') {
            t.punctuation_pause_ms
        } else {
            0
        };
        // Script validation bounds the combined gap to one day before playback.
        interval + pause
    }
}
