#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub midi: u8,
    /// Sounding duration in ticks; one hundred ticks equal one quarter note.
    pub duration_ticks: u16,
    /// Silence after this note, up to the next score onset, in the same ticks.
    pub rest_ticks: u16,
}

impl Note {
    pub const fn new(midi: u8, duration_ticks: u16, rest_ticks: u16) -> Self {
        Self {
            midi,
            duration_ticks,
            rest_ticks,
        }
    }

    pub fn frequency_hz(self) -> f32 {
        440.0 * 2.0_f32.powf((self.midi as f32 - 69.0) / 12.0)
    }

    pub fn duration_ms(self, bpm: u16) -> u64 {
        ticks_to_millis(self.duration_ticks, bpm).clamp(35, 3_000)
    }

    pub fn rest_ms(self, bpm: u16) -> u64 {
        ticks_to_millis(self.rest_ticks, bpm)
    }
}

const TICKS_PER_QUARTER: u64 = 100;

fn ticks_to_millis(ticks: u16, bpm: u16) -> u64 {
    let denominator = u64::from(bpm.max(1)) * TICKS_PER_QUARTER;
    (u64::from(ticks) * 60_000 + denominator / 2) / denominator
}

/// Tempo of the score-based cue, in quarter notes per minute.
pub const ANTHEM_BPM: u16 = 76;

/// Lead line transcribed from the official symphonic score: first cornet part,
/// transposed to concert pitch in C major. The excerpt contains one verse and refrain.
/// Each recruitment event consumes exactly one note; no score playback runs in the background.
pub const ANTHEM: &[Note] = &[
    Note::new(72, 300, 50),
    Note::new(67, 50, 0),
    Note::new(72, 100, 0),
    Note::new(67, 75, 0),
    Note::new(69, 25, 0),
    Note::new(71, 100, 0),
    Note::new(64, 50, 0),
    Note::new(64, 50, 0),
    Note::new(69, 100, 0),
    Note::new(67, 75, 0),
    Note::new(65, 25, 0),
    Note::new(67, 100, 0),
    Note::new(60, 50, 0),
    Note::new(60, 50, 0),
    Note::new(62, 100, 0),
    Note::new(62, 75, 0),
    Note::new(64, 25, 0),
    Note::new(65, 100, 0),
    Note::new(65, 75, 0),
    Note::new(67, 25, 0),
    Note::new(69, 100, 0),
    Note::new(71, 50, 0),
    Note::new(72, 50, 0),
    Note::new(74, 150, 0),
    Note::new(67, 50, 0),
    Note::new(76, 100, 0),
    Note::new(74, 75, 0),
    Note::new(72, 25, 0),
    Note::new(74, 100, 0),
    Note::new(71, 50, 0),
    Note::new(67, 50, 0),
    Note::new(72, 100, 0),
    Note::new(71, 75, 0),
    Note::new(69, 25, 0),
    Note::new(71, 100, 0),
    Note::new(64, 50, 0),
    Note::new(64, 50, 0),
    Note::new(69, 100, 0),
    Note::new(67, 75, 0),
    Note::new(65, 25, 0),
    Note::new(67, 100, 0),
    Note::new(60, 50, 0),
    Note::new(60, 50, 0),
    Note::new(72, 100, 0),
    Note::new(71, 75, 0),
    Note::new(69, 25, 0),
    Note::new(67, 50, 0),
    Note::new(71, 50, 0),
    Note::new(72, 50, 0),
    Note::new(74, 50, 0),
    Note::new(76, 200, 0),
    Note::new(74, 50, 0),
    Note::new(72, 50, 0),
    Note::new(71, 50, 0),
    Note::new(72, 50, 0),
    Note::new(74, 150, 0),
    Note::new(67, 50, 0),
    Note::new(67, 50, 0),
    Note::new(71, 50, 0),
    Note::new(72, 50, 0),
    Note::new(74, 50, 0),
    Note::new(72, 200, 0),
    Note::new(71, 50, 0),
    Note::new(69, 50, 0),
    Note::new(67, 50, 0),
    Note::new(69, 50, 0),
    Note::new(71, 150, 0),
    Note::new(64, 50, 0),
    Note::new(64, 50, 0),
    Note::new(67, 50, 0),
    Note::new(69, 50, 0),
    Note::new(71, 50, 0),
    Note::new(72, 100, 0),
    Note::new(69, 75, 0),
    Note::new(71, 25, 0),
    Note::new(72, 100, 0),
    Note::new(69, 75, 0),
    Note::new(71, 25, 0),
    Note::new(72, 100, 0),
    Note::new(69, 50, 0),
    Note::new(72, 50, 0),
    Note::new(77, 200, 0),
    Note::new(77, 200, 0),
    Note::new(76, 50, 0),
    Note::new(74, 50, 0),
    Note::new(72, 50, 0),
    Note::new(74, 50, 0),
    Note::new(76, 150, 0),
    Note::new(72, 50, 0),
    Note::new(72, 200, 0),
    Note::new(74, 200, 0),
    Note::new(72, 50, 0),
    Note::new(71, 50, 0),
    Note::new(69, 50, 0),
    Note::new(71, 50, 0),
    Note::new(72, 150, 0),
    Note::new(69, 50, 0),
    Note::new(69, 200, 0),
    Note::new(67, 200, 0),
    Note::new(69, 11, 0),
    Note::new(71, 100, 0),
    Note::new(72, 250, 0),
];

#[derive(Clone, Debug, Default)]
pub struct Sequencer {
    cursor: usize,
}

impl Sequencer {
    pub fn next(&mut self) -> Note {
        let note = ANTHEM[self.cursor];
        self.cursor = (self.cursor + 1) % ANTHEM.len();
        note
    }

    pub fn position(&self) -> usize {
        self.cursor
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::{Note, Sequencer, ANTHEM, ANTHEM_BPM};

    #[test]
    fn opening_matches_the_official_concert_pitch_transcription() {
        let opening: Vec<u8> = ANTHEM.iter().take(8).map(|note| note.midi).collect();
        assert_eq!(opening, [72, 67, 72, 67, 69, 71, 64, 64]);
    }

    #[test]
    fn excerpt_has_one_verse_and_refrain_and_resolves_to_the_tonic() {
        assert_eq!(ANTHEM.len(), 102);
        assert_eq!(ANTHEM.last().map(|note| note.midi), Some(72));
    }

    #[test]
    fn score_notes_are_diatonic_in_c_major() {
        assert!(ANTHEM
            .iter()
            .all(|note| matches!(note.midi % 12, 0 | 2 | 4 | 5 | 7 | 9 | 11)));
        assert!(ANTHEM.iter().all(|note| note.duration_ticks > 0));
        assert_eq!(ANTHEM.iter().filter(|note| note.rest_ticks > 0).count(), 1);
    }

    #[test]
    fn note_and_rest_lengths_use_the_score_tempo() {
        assert_eq!(ANTHEM_BPM, 76);
        assert_eq!(Note::new(60, 100, 0).duration_ms(ANTHEM_BPM), 789);
        assert_eq!(Note::new(60, 50, 0).duration_ms(ANTHEM_BPM), 395);
        assert_eq!(ANTHEM[0], Note::new(72, 300, 50));
        assert_eq!(ANTHEM[0].duration_ms(ANTHEM_BPM), 2_368);
        assert_eq!(ANTHEM[0].rest_ms(ANTHEM_BPM), 395);
    }

    #[test]
    fn sequencer_cycles_through_the_full_excerpt() {
        let mut sequencer = Sequencer::default();
        for note in ANTHEM {
            assert_eq!(sequencer.next(), *note);
        }
        assert_eq!(sequencer.position(), 0);
        assert_eq!(sequencer.next(), ANTHEM[0]);
    }
}
