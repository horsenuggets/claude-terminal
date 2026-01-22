//! Audio recording using cpal

use anyhow::Result;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::mpsc;

use crate::app::AppMessage;

use super::whisper::transcribe;

/// Voice recorder that captures audio and sends to Whisper for transcription
pub struct VoiceRecorder {
    message_tx: mpsc::Sender<AppMessage>,
    recording: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: Arc<Mutex<u32>>,
}

impl VoiceRecorder {
    pub fn new(message_tx: mpsc::Sender<AppMessage>) -> Self {
        Self {
            message_tx,
            recording: Arc::new(AtomicBool::new(false)),
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: Arc::new(Mutex::new(16000)),
        }
    }

    /// Start recording audio
    pub async fn start(&self) -> Result<()> {
        // Clear previous samples
        {
            let mut samples = self.samples.lock().unwrap();
            samples.clear();
        }

        self.recording.store(true, Ordering::SeqCst);

        let samples = self.samples.clone();
        let sample_rate_store = self.sample_rate.clone();
        let recording = self.recording.clone();
        let tx = self.message_tx.clone();

        // Run recording in a dedicated thread (cpal Stream isn't Send)
        std::thread::spawn(move || {
            if let Err(e) = run_recording(samples, sample_rate_store, recording) {
                tracing::error!("Recording error: {}", e);
                let _ = tx.blocking_send(AppMessage::VoiceError(e.to_string()));
            }
        });

        Ok(())
    }

    /// Stop recording and transcribe
    pub async fn stop(&self) -> Result<()> {
        self.recording.store(false, Ordering::SeqCst);

        // Give time for the stream to finish
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let samples = {
            let samples = self.samples.lock().unwrap();
            samples.clone()
        };

        let sample_rate = {
            let sr = self.sample_rate.lock().unwrap();
            *sr
        };

        if samples.is_empty() {
            self.message_tx
                .send(AppMessage::VoiceError("No audio recorded".to_string()))
                .await?;
            return Ok(());
        }

        let tx = self.message_tx.clone();

        // Transcribe in background
        tokio::spawn(async move {
            match transcribe(&samples, sample_rate).await {
                Ok(text) => {
                    let _ = tx.send(AppMessage::VoiceTranscription(text)).await;
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::VoiceError(e.to_string())).await;
                }
            }
        });

        Ok(())
    }

    /// Cancel recording without transcribing
    pub async fn cancel(&self) {
        self.recording.store(false, Ordering::SeqCst);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let mut samples = self.samples.lock().unwrap();
        samples.clear();
    }
}

/// Run the recording loop in a dedicated thread
fn run_recording(
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate_store: Arc<Mutex<u32>>,
    recording: Arc<AtomicBool>,
) -> Result<()> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    // Store the sample rate
    {
        let mut sr = sample_rate_store.lock().unwrap();
        *sr = sample_rate;
    }

    tracing::debug!("Recording at {} Hz, {} channels", sample_rate, channels);

    // Build stream based on sample format
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let samples = samples.clone();
            let recording = recording.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if recording.load(Ordering::SeqCst) {
                        let mut samples = samples.lock().unwrap();
                        // Convert to mono if stereo
                        if channels > 1 {
                            for chunk in data.chunks(channels) {
                                let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
                                samples.push(mono);
                            }
                        } else {
                            samples.extend_from_slice(data);
                        }
                    }
                },
                |err| {
                    tracing::error!("Audio input error: {}", err);
                },
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let samples = samples.clone();
            let recording = recording.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if recording.load(Ordering::SeqCst) {
                        let mut samples = samples.lock().unwrap();
                        if channels > 1 {
                            for chunk in data.chunks(channels) {
                                let mono: f32 = chunk.iter().map(|&s| s as f32 / 32768.0).sum::<f32>()
                                    / channels as f32;
                                samples.push(mono);
                            }
                        } else {
                            for &sample in data {
                                samples.push(sample as f32 / 32768.0);
                            }
                        }
                    }
                },
                |err| {
                    tracing::error!("Audio input error: {}", err);
                },
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let samples = samples.clone();
            let recording = recording.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    if recording.load(Ordering::SeqCst) {
                        let mut samples = samples.lock().unwrap();
                        if channels > 1 {
                            for chunk in data.chunks(channels) {
                                let mono: f32 = chunk
                                    .iter()
                                    .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                    .sum::<f32>()
                                    / channels as f32;
                                samples.push(mono);
                            }
                        } else {
                            for &sample in data {
                                samples.push((sample as f32 - 32768.0) / 32768.0);
                            }
                        }
                    }
                },
                |err| {
                    tracing::error!("Audio input error: {}", err);
                },
                None,
            )?
        }
        _ => return Err(anyhow::anyhow!("Unsupported sample format")),
    };

    stream.play()?;

    // Keep stream alive while recording
    while recording.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Stream is dropped here, stopping recording
    Ok(())
}
