#[derive(Clone, Copy, Debug)]
pub struct Note {
    pub midi: u8,
    /// Duration in eighth-note units (2 = one quarter note).
    pub eighths: u8,
}

impl Note {
    pub const fn new(midi: u8, eighths: u8) -> Self {
        Self { midi, eighths }
    }

    pub fn frequency_hz(self) -> f32 {
        440.0 * 2.0_f32.powf((self.midi as f32 - 69.0) / 12.0)
    }
}

/// Lead melody of the Russian national anthem in E-flat major.
/// Each pair is (MIDI pitch, duration in eighth-note units). The sequence is
/// kept in one place and is never reset at mobilization-wave boundaries.
pub const ANTHEM: &[Note] = &[
    Note::new(63, 4), Note::new(68, 4), Note::new(65, 5), Note::new(67, 2),
    Note::new(68, 2), Note::new(70, 2), Note::new(72, 2), Note::new(73, 2),
    Note::new(75, 4), Note::new(75, 4), Note::new(73, 4), Note::new(70, 4),
    Note::new(68, 5), Note::new(72, 2), Note::new(70, 2), Note::new(68, 2),
    Note::new(67, 2), Note::new(65, 2), Note::new(63, 4), Note::new(63, 2),
    Note::new(63, 2), Note::new(63, 4), Note::new(68, 4), Note::new(65, 5),
    Note::new(67, 2), Note::new(68, 2), Note::new(70, 2), Note::new(72, 2),
    Note::new(73, 2), Note::new(75, 4), Note::new(75, 4), Note::new(77, 4),
    Note::new(68, 4), Note::new(72, 6), Note::new(73, 2), Note::new(72, 4),
    Note::new(70, 3), Note::new(72, 1), Note::new(68, 4), Note::new(70, 4),
    Note::new(70, 4), Note::new(70, 4), Note::new(68, 2), Note::new(70, 2),
    Note::new(72, 4), Note::new(73, 3), Note::new(72, 1), Note::new(75, 4),
    Note::new(72, 3), Note::new(70, 4), Note::new(70, 4), Note::new(70, 4),
    Note::new(72, 2), Note::new(70, 2), Note::new(68, 4), Note::new(67, 2),
    Note::new(68, 2), Note::new(65, 4), Note::new(63, 2), Note::new(63, 4),
    Note::new(68, 4), Note::new(65, 6), Note::new(67, 2), Note::new(68, 2),
    Note::new(70, 2), Note::new(72, 2), Note::new(73, 2), Note::new(75, 4),
    Note::new(75, 4), Note::new(77, 4), Note::new(68, 4), Note::new(72, 6),
    Note::new(73, 2), Note::new(72, 4), Note::new(70, 3), Note::new(72, 1),
    Note::new(68, 4),
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
