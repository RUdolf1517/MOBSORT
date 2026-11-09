use crate::anthem::Note;

#[cfg(feature = "audio")]
mod enabled {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicBool, AtomicU32, Ordering},
            mpsc::{self, Receiver, Sender},
            Arc,
        },
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
        Clear,
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

        pub fn play(&self, note: Note, beat_ms: u16) {
            let duration_ms = (u64::from(beat_ms)
                .saturating_mul(u64::from(note.eighths.max(1)))
                / 2)
                .clamp(35, 1_500);
            let _ = self.sender.send(Message::Play(SoundNote {
                frequency: note.frequency_hz(),
                duration_ms,
            }));
        }

        pub fn clear_queue(&self) {
            let _ = self.sender.send(Message::Clear);
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
        pending: VecDeque<SoundNote>,
        sample_rate: f32,
        channels: usize,
        frequency: f32,
        phase: f32,
        samples_left: u64,
        note_samples: u64,
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
            Self {
                receiver,
                pending: VecDeque::new(),
                sample_rate: sample_rate.max(1.0),
                channels: channels.max(1),
                frequency: 0.0,
                phase: 0.0,
                samples_left: 0,
                note_samples: 0,
                volume,
                muted,
                paused,
            }
        }

        fn fill(&mut self, output: &mut [f32]) {
            while let Ok(message) = self.receiver.try_recv() {
                match message {
                    Message::Play(note) => self.pending.push_back(note),
                    Message::Clear => {
                        self.pending.clear();
                        self.samples_left = 0;
                        self.note_samples = 0;
                        self.frequency = 0.0;
                    }
                }
            }

            if self.paused.load(Ordering::Relaxed) {
                output.fill(0.0);
                return;
            }

            let muted = self.muted.load(Ordering::Relaxed);
            let volume = self.volume.load(Ordering::Relaxed) as f32 / 100.0;
            let attack_samples = (self.sample_rate * 0.012).max(1.0) as u64;
            let release_samples = (self.sample_rate * 0.025).max(1.0) as u64;

            for frame in output.chunks_mut(self.channels) {
                if self.samples_left == 0 {
                    if let Some(note) = self.pending.pop_front() {
                        self.frequency = note.frequency;
                        self.note_samples =
                            ((note.duration_ms as f32 / 1_000.0) * self.sample_rate).max(1.0)
                                as u64;
                        self.samples_left = self.note_samples;
                    } else {
                        self.frequency = 0.0;
                        frame.fill(0.0);
                        continue;
                    }
                }

                let elapsed = self.note_samples.saturating_sub(self.samples_left);
                let attack = (elapsed as f32 / attack_samples as f32).min(1.0);
                let release = (self.samples_left as f32 / release_samples as f32).min(1.0);
                let envelope = attack.min(release);
                let tone = self.phase.sin() * 0.78
                    + (self.phase * 2.0).sin() * 0.16
                    + (self.phase * 3.0).sin() * 0.06;
                let value = if muted {
                    0.0
                } else {
                    tone * envelope * volume * 0.22
                };
                frame.fill(value);
                self.phase += std::f32::consts::TAU * self.frequency / self.sample_rate;
                if self.phase >= std::f32::consts::TAU {
                    self.phase -= std::f32::consts::TAU;
                }
                self.samples_left = self.samples_left.saturating_sub(1);
            }
        }
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
        Ok(Self)
    }

    pub fn play(&self, _note: Note, _beat_ms: u16) {}

    pub fn clear_queue(&self) {}

    pub fn set_muted(&self, _muted: bool) {}

    pub fn set_paused(&self, _paused: bool) {}

    pub fn set_volume(&self, _volume: u8) {}
}
