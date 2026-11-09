use crate::anthem::Note;

#[cfg(feature = "audio")]
mod enabled {
    use std::sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    };

    use cpal::{
        traits::{DeviceTrait, HostTrait, StreamTrait},
        SampleFormat, Stream, StreamConfig,
    };

    use crate::anthem::Note;

    #[derive(Clone, Copy)]
    struct SoundNote {
        frequency: f32,
        duration_ms: u64,
    }

    enum Message {
        Play(SoundNote),
        Stop,
    }

    pub struct AudioEngine {
        sender: Sender<Message>,
        volume: Arc<AtomicU32>,
        muted: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
        _stream: Stream,
    }

    impl AudioEngine {
        pub fn new() -> Result<Self, String> {
            let host = cpal::default_host();
            let device = host
                .default_output_device()
                .ok_or_else(|| "Не найдено устройство вывода звука".to_string())?;
            let supported = device
                .default_output_config()
                .map_err(|error| format!("Не удалось открыть аудиоконфигурацию: {error}"))?;
            let sample_format = supported.sample_format();
            let stream_config: StreamConfig = supported.config();
            let sample_rate = stream_config.sample_rate.0 as f32;
            let channels = usize::from(stream_config.channels).max(1);
            let (sender, receiver) = mpsc::channel();
            let volume = Arc::new(AtomicU32::new(30));
            let muted = Arc::new(AtomicBool::new(false));
            let paused = Arc::new(AtomicBool::new(false));

            let stream = match sample_format {
                SampleFormat::F32 => build_f32(
                    &device,
                    &stream_config,
                    receiver,
                    sample_rate,
                    channels,
                    Arc::clone(&volume),
                    Arc::clone(&muted),
                    Arc::clone(&paused),
                ),
                SampleFormat::I16 => build_i16(
                    &device,
                    &stream_config,
                    receiver,
                    sample_rate,
                    channels,
                    Arc::clone(&volume),
                    Arc::clone(&muted),
                    Arc::clone(&paused),
                ),
                SampleFormat::U16 => build_u16(
                    &device,
                    &stream_config,
                    receiver,
                    sample_rate,
                    channels,
                    Arc::clone(&volume),
                    Arc::clone(&muted),
                    Arc::clone(&paused),
                ),
                _ => Err("Аудиоустройство использует неподдерживаемый формат семплов".to_string()),
            }?;
            stream
                .play()
                .map_err(|error| format!("Не удалось запустить звук: {error}"))?;

            Ok(Self {
                sender,
                volume,
                muted,
                paused,
                _stream: stream,
            })
        }

        /// Starts the note caused by one recruitment move and returns its duration.
        /// A new event replaces the current tone; notes are never queued for idle playback.
        pub fn play(&self, note: Note, bpm: u16) -> u64 {
            let duration_ms = note.duration_ms(bpm);
            let _ = self.sender.send(Message::Play(SoundNote {
                frequency: note.frequency_hz(),
                duration_ms,
            }));
            duration_ms
        }

        pub fn stop(&self) {
            let _ = self.sender.send(Message::Stop);
        }

        pub fn set_muted(&self, muted: bool) {
            self.muted.store(muted, Ordering::Relaxed);
        }

        pub fn set_paused(&self, paused: bool) {
            self.paused.store(paused, Ordering::Relaxed);
        }

        pub fn set_volume(&self, volume: u8) {
            self.volume
                .store(u32::from(volume.min(100)), Ordering::Relaxed);
        }
    }

    struct Synth {
        receiver: Receiver<Message>,
        sample_rate: f32,
        channels: usize,
        frequency: f32,
        phase_low: f32,
        phase_high: f32,
        phase_center: f32,
        vibrato_phase: f32,
        vibrato_enabled: bool,
        samples_left: u64,
        note_samples: u64,
        room_left: Vec<f32>,
        room_right: Vec<f32>,
        room_left_cursor: usize,
        room_right_cursor: usize,
        volume: Arc<AtomicU32>,
        muted: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    }

    impl Synth {
        fn new(
            receiver: Receiver<Message>,
            sample_rate: f32,
            channels: usize,
            volume: Arc<AtomicU32>,
            muted: Arc<AtomicBool>,
            paused: Arc<AtomicBool>,
        ) -> Self {
            let sample_rate = sample_rate.max(1.0);
            let room_left_samples = (sample_rate * 0.037).round().max(1.0) as usize;
            let room_right_samples = (sample_rate * 0.053).round().max(1.0) as usize;
            Self {
                receiver,
                sample_rate,
                channels: channels.max(1),
                frequency: 0.0,
                phase_low: 0.0,
                phase_high: 0.0,
                phase_center: 0.0,
                vibrato_phase: 0.0,
                vibrato_enabled: false,
                samples_left: 0,
                note_samples: 0,
                room_left: vec![0.0; room_left_samples],
                room_right: vec![0.0; room_right_samples],
                room_left_cursor: 0,
                room_right_cursor: 0,
                volume,
                muted,
                paused,
            }
        }

        fn fill(&mut self, output: &mut [f32]) {
            // Keep only the most recent command. This is deliberately not a music queue:
            // each tone is causally tied to an individual recruitment event.
            let mut latest = None;
            while let Ok(message) = self.receiver.try_recv() {
                latest = Some(message);
            }
            match latest {
                Some(Message::Play(note)) => {
                    self.frequency = note.frequency;
                    self.phase_low = 0.0;
                    self.phase_high = 0.0;
                    self.phase_center = 0.0;
                    self.vibrato_phase = 0.0;
                    self.vibrato_enabled = note.duration_ms >= 900;
                    self.note_samples =
                        ((note.duration_ms as f32 / 1_000.0) * self.sample_rate).max(1.0) as u64;
                    self.samples_left = self.note_samples;
                }
                Some(Message::Stop) => {
                    self.frequency = 0.0;
                    self.phase_low = 0.0;
                    self.phase_high = 0.0;
                    self.phase_center = 0.0;
                    self.vibrato_phase = 0.0;
                    self.vibrato_enabled = false;
                    self.samples_left = 0;
                    self.note_samples = 0;
                    self.clear_room();
                }
                None => {}
            }

            if self.paused.load(Ordering::Relaxed) {
                output.fill(0.0);
                return;
            }

            if self.muted.load(Ordering::Relaxed) {
                self.clear_room();
                output.fill(0.0);
                return;
            }

            let volume = self.volume.load(Ordering::Relaxed) as f32 / 100.0;
            let attack_samples = (self.sample_rate * 0.018).max(1.0) as u64;
            let release_samples = (self.sample_rate * 0.055).max(1.0) as u64;

            for frame in output.chunks_mut(self.channels) {
                let mut source = 0.0;
                if self.samples_left > 0 {
                    let elapsed = self.note_samples.saturating_sub(self.samples_left);
                    let attack = (elapsed as f32 / attack_samples as f32).min(1.0);
                    let release = (self.samples_left as f32 / release_samples as f32).min(1.0);
                    let envelope = attack.min(release);
                    // Two lightly detuned, harmonic-rich voices and a centered triangle
                    // create a compact brass-like ensemble rather than a pure sine tone.
                    let tone = brass_saw(self.phase_low) * 0.40
                        + brass_saw(self.phase_high) * 0.34
                        + brass_triangle(self.phase_center) * 0.26;
                    source = tone * envelope * 0.22;

                    let vibrato = if self.vibrato_enabled {
                        self.vibrato_phase.sin() * 0.0035
                    } else {
                        0.0
                    };
                    let phase_step =
                        std::f32::consts::TAU * self.frequency * (1.0 + vibrato) / self.sample_rate;
                    self.phase_low += phase_step * 0.996_540_26;
                    self.phase_high += phase_step * 1.003_471_75;
                    self.phase_center += phase_step;
                    self.vibrato_phase += std::f32::consts::TAU * 5.2 / self.sample_rate;
                    if self.phase_low >= std::f32::consts::TAU {
                        self.phase_low -= std::f32::consts::TAU;
                    }
                    if self.phase_high >= std::f32::consts::TAU {
                        self.phase_high -= std::f32::consts::TAU;
                    }
                    if self.phase_center >= std::f32::consts::TAU {
                        self.phase_center -= std::f32::consts::TAU;
                    }
                    if self.vibrato_phase >= std::f32::consts::TAU {
                        self.vibrato_phase -= std::f32::consts::TAU;
                    }
                    self.samples_left -= 1;
                } else {
                    self.frequency = 0.0;
                }

                let (left, right) = self.room_sample(source);
                if frame.len() == 1 {
                    frame[0] = (left + right) * 0.5 * volume;
                } else {
                    frame[0] = left * volume;
                    frame[1] = right * volume;
                    let center = (left + right) * 0.5 * volume;
                    frame.iter_mut().skip(2).for_each(|sample| *sample = center);
                }
            }
        }

        // Cross-fed short delays add a stereo room tail; they never schedule new notes.
        fn room_sample(&mut self, input: f32) -> (f32, f32) {
            let delayed_left = self.room_left[self.room_left_cursor];
            let delayed_right = self.room_right[self.room_right_cursor];
            self.room_left[self.room_left_cursor] = input * 0.42 + delayed_right * 0.58;
            self.room_right[self.room_right_cursor] = input * 0.42 + delayed_left * 0.58;
            self.room_left_cursor = (self.room_left_cursor + 1) % self.room_left.len();
            self.room_right_cursor = (self.room_right_cursor + 1) % self.room_right.len();
            (input * 0.82 + delayed_left * 0.24, input * 0.82 + delayed_right * 0.24)
        }

        fn clear_room(&mut self) {
            self.room_left.fill(0.0);
            self.room_right.fill(0.0);
            self.room_left_cursor = 0;
            self.room_right_cursor = 0;
        }
    }

    fn brass_saw(phase: f32) -> f32 {
        phase.sin() * 0.58
            + (phase * 2.0).sin() * 0.25
            + (phase * 3.0).sin() * 0.12
            + (phase * 4.0).sin() * 0.06
            + (phase * 5.0).sin() * 0.03
    }

    fn brass_triangle(phase: f32) -> f32 {
        phase.sin() * 0.78 - (phase * 3.0).sin() * 0.12 + (phase * 5.0).sin() * 0.045
            - (phase * 7.0).sin() * 0.025
    }

    fn build_f32(
        device: &cpal::Device,
        config: &StreamConfig,
        receiver: Receiver<Message>,
        sample_rate: f32,
        channels: usize,
        volume: Arc<AtomicU32>,
        muted: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> Result<Stream, String> {
        let mut synth = Synth::new(receiver, sample_rate, channels, volume, muted, paused);
        device
            .build_output_stream(
                config,
                move |data: &mut [f32], _| synth.fill(data),
                |error| eprintln!("Audio stream error: {error}"),
                None,
            )
            .map_err(|error| format!("Не удалось создать аудиопоток: {error}"))
    }

    fn build_i16(
        device: &cpal::Device,
        config: &StreamConfig,
        receiver: Receiver<Message>,
        sample_rate: f32,
        channels: usize,
        volume: Arc<AtomicU32>,
        muted: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> Result<Stream, String> {
        let mut synth = Synth::new(receiver, sample_rate, channels, volume, muted, paused);
        let mut scratch = Vec::<f32>::new();
        device
            .build_output_stream(
                config,
                move |data: &mut [i16], _| {
                    scratch.resize(data.len(), 0.0);
                    synth.fill(&mut scratch);
                    for (output, sample) in data.iter_mut().zip(scratch.iter()) {
                        *output = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    }
                },
                |error| eprintln!("Audio stream error: {error}"),
                None,
            )
            .map_err(|error| format!("Не удалось создать аудиопоток: {error}"))
    }

    fn build_u16(
        device: &cpal::Device,
        config: &StreamConfig,
        receiver: Receiver<Message>,
        sample_rate: f32,
        channels: usize,
        volume: Arc<AtomicU32>,
        muted: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> Result<Stream, String> {
        let mut synth = Synth::new(receiver, sample_rate, channels, volume, muted, paused);
        let mut scratch = Vec::<f32>::new();
        device
            .build_output_stream(
                config,
                move |data: &mut [u16], _| {
                    scratch.resize(data.len(), 0.0);
                    synth.fill(&mut scratch);
                    for (output, sample) in data.iter_mut().zip(scratch.iter()) {
                        let normalized = sample.clamp(-1.0, 1.0) * 0.5 + 0.5;
                        *output = (normalized * u16::MAX as f32) as u16;
                    }
                },
                |error| eprintln!("Audio stream error: {error}"),
                None,
            )
            .map_err(|error| format!("Не удалось создать аудиопоток: {error}"))
    }
}

#[cfg(feature = "audio")]
pub use enabled::AudioEngine;

#[cfg(not(feature = "audio"))]
pub struct AudioEngine;

#[cfg(not(feature = "audio"))]
impl AudioEngine {
    pub fn new() -> Result<Self, String> {
        Err("Аудиоподдержка отключена при сборке".to_string())
    }

    pub fn play(&self, note: Note, bpm: u16) -> u64 {
        note.duration_ms(bpm)
    }

    pub fn stop(&self) {}

    pub fn set_muted(&self, _muted: bool) {}

    pub fn set_paused(&self, _paused: bool) {}

    pub fn set_volume(&self, _volume: u8) {}
}
