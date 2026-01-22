//! OpenAI Whisper API integration

use anyhow::Result;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;

const WHISPER_API_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

/// Transcribe audio samples using OpenAI Whisper API
pub async fn transcribe(samples: &[f32], sample_rate: u32) -> Result<String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;

    // Resample to 16kHz if needed (Whisper expects 16kHz)
    let samples = if sample_rate != 16000 {
        resample(samples, sample_rate, 16000)
    } else {
        samples.to_vec()
    };

    // Encode as WAV
    let wav_data = encode_wav(&samples, 16000)?;

    // Create multipart form
    let part = Part::bytes(wav_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let form = Form::new()
        .part("file", part)
        .text("model", "whisper-1")
        .text("language", "en");

    // Send request
    let client = reqwest::Client::new();
    let response = client
        .post(WHISPER_API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        return Err(anyhow::anyhow!("Whisper API error: {}", error));
    }

    let result: TranscriptionResponse = response.json().await?;
    Ok(result.text)
}

/// Simple linear resampling
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let new_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < samples.len() {
            samples[idx] * (1.0 - frac as f32) + samples[idx + 1] * frac as f32
        } else if idx < samples.len() {
            samples[idx]
        } else {
            0.0
        };

        output.push(sample);
    }

    output
}

/// Encode samples as WAV
fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    use std::io::Cursor;

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for &sample in samples {
            let amplitude = (sample * 32767.0) as i16;
            writer.write_sample(amplitude)?;
        }
        writer.finalize()?;
    }

    Ok(cursor.into_inner())
}
