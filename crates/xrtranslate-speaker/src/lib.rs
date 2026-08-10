//! Native speaker embeddings and deterministic online speaker tracking.
//!
//! The ONNX contract follows 3D-Speaker's official ERes2NetV2 exporter:
//! `feature` is `[batch, frames, 80]` Kaldi-compatible log filterbanks and
//! `embedding` is `[batch, 192]`.  Audio preprocessing and clustering live in
//! this crate so the latency-sensitive backend does not need a Python sidecar.

#![forbid(unsafe_code)]

use std::{error::Error, f32::consts::PI, fmt, path::Path};

use ndarray::Array3;
use ort::{session::Session, value::Value};

pub const SAMPLE_RATE_HZ: u32 = 16_000;
pub const FEATURE_DIMENSIONS: usize = 80;
pub const EMBEDDING_DIMENSIONS: usize = 192;
const FRAME_SAMPLES: usize = 400;
const FRAME_SHIFT_SAMPLES: usize = 160;
const FFT_SIZE: usize = 512;
const FFT_BINS: usize = FFT_SIZE / 2;

#[derive(Debug)]
pub enum SpeakerError {
    Ort(ort::Error),
    EmptyEmbedding,
    NonFiniteEmbedding,
    InvalidModelOutput,
    InvalidTrackerConfig(&'static str),
}

impl fmt::Display for SpeakerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ort(error) => write!(formatter, "ONNX Runtime speaker inference failed: {error}"),
            Self::EmptyEmbedding => {
                formatter.write_str("speaker model returned an empty embedding")
            }
            Self::NonFiniteEmbedding => {
                formatter.write_str("speaker model returned a non-finite embedding")
            }
            Self::InvalidModelOutput => {
                formatter.write_str("speaker model output is not a valid float tensor")
            }
            Self::InvalidTrackerConfig(message) => write!(
                formatter,
                "invalid speaker tracker configuration: {message}"
            ),
        }
    }
}

impl Error for SpeakerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Ort(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ort::Error> for SpeakerError {
    fn from(value: ort::Error) -> Self {
        Self::Ort(value)
    }
}

/// Stateless ERes2NetV2 ONNX inference. Keep one instance on one worker task.
#[derive(Debug)]
pub struct SpeakerEmbeddingModel {
    session: Session,
    fbank: FbankExtractor,
}

impl SpeakerEmbeddingModel {
    pub fn from_file(
        model_path: impl AsRef<Path>,
        intra_threads: usize,
    ) -> Result<Self, SpeakerError> {
        let session = Session::builder()?
            .with_intra_threads(intra_threads.max(1))?
            .with_inter_threads(1)?
            .commit_from_file(model_path)?;
        Ok(Self {
            session,
            fbank: FbankExtractor::new(),
        })
    }

    /// Extracts one L2-normalized embedding from mono PCM16 at 16 kHz.
    pub fn extract(&mut self, samples: &[i16]) -> Result<Vec<f32>, SpeakerError> {
        let features = self.fbank.compute(samples);
        let frames = features.len() / FEATURE_DIMENSIONS;
        let feature = Array3::from_shape_vec((1, frames, FEATURE_DIMENSIONS), features)
            .expect("fbank output always has a complete final dimension");
        let feature = Value::from_array(feature)?;
        let outputs = self.session.run(ort::inputs!["feature" => feature])?;
        let embedding = outputs
            .get("embedding")
            .ok_or(SpeakerError::InvalidModelOutput)?;
        let (_, values) = embedding
            .try_extract_tensor::<f32>()
            .map_err(|_| SpeakerError::InvalidModelOutput)?;
        if values.len() != EMBEDDING_DIMENSIONS {
            return Err(SpeakerError::InvalidModelOutput);
        }
        normalize_embedding(values.to_vec())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackerConfig {
    pub similarity_threshold: f32,
    pub same_speaker_hysteresis: f32,
    pub max_speakers: usize,
}

impl TrackerConfig {
    pub fn validate(self) -> Result<Self, SpeakerError> {
        if !self.similarity_threshold.is_finite()
            || !(0.0..=1.0).contains(&self.similarity_threshold)
        {
            return Err(SpeakerError::InvalidTrackerConfig(
                "similarity_threshold must be finite and within 0..=1",
            ));
        }
        if !self.same_speaker_hysteresis.is_finite()
            || !(0.0..=0.25).contains(&self.same_speaker_hysteresis)
        {
            return Err(SpeakerError::InvalidTrackerConfig(
                "same_speaker_hysteresis must be finite and within 0..=0.25",
            ));
        }
        if self.max_speakers == 0 || self.max_speakers > 64 {
            return Err(SpeakerError::InvalidTrackerConfig(
                "max_speakers must be within 1..=64",
            ));
        }
        Ok(self)
    }
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.62,
            same_speaker_hysteresis: 0.04,
            max_speakers: 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpeakerAssignment {
    pub speaker_id: String,
    pub similarity: f32,
    pub is_new: bool,
}

#[derive(Debug)]
struct SpeakerCentroid {
    embedding: Vec<f32>,
    observations: u32,
}

/// Arrival-ordered, memory-bounded online cosine clustering.
///
/// IDs never change inside an audio generation. A small hysteresis prevents
/// adjacent chunks from the same speaker from oscillating near the threshold.
#[derive(Debug)]
pub struct OnlineSpeakerTracker {
    config: TrackerConfig,
    centroids: Vec<SpeakerCentroid>,
    previous_speaker: Option<usize>,
}

impl OnlineSpeakerTracker {
    pub fn new(config: TrackerConfig) -> Result<Self, SpeakerError> {
        Ok(Self {
            config: config.validate()?,
            centroids: Vec::with_capacity(config.max_speakers),
            previous_speaker: None,
        })
    }

    pub fn reset(&mut self) {
        self.centroids.clear();
        self.previous_speaker = None;
    }

    pub fn speaker_count(&self) -> usize {
        self.centroids.len()
    }

    pub fn assign(&mut self, embedding: &[f32]) -> Result<SpeakerAssignment, SpeakerError> {
        let embedding = normalize_embedding(embedding.to_vec())?;
        let best = self
            .centroids
            .iter()
            .enumerate()
            .map(|(index, centroid)| (index, cosine(&embedding, &centroid.embedding)))
            .max_by(|left, right| left.1.total_cmp(&right.1));

        let accepted = best.filter(|(index, similarity)| {
            let threshold = if self.previous_speaker == Some(*index) {
                self.config.similarity_threshold - self.config.same_speaker_hysteresis
            } else {
                self.config.similarity_threshold
            };
            *similarity >= threshold
        });

        let (index, similarity, is_new) = if let Some((index, similarity)) = accepted {
            self.update_centroid(index, &embedding)?;
            (index, similarity, false)
        } else if self.centroids.len() < self.config.max_speakers {
            let index = self.centroids.len();
            self.centroids.push(SpeakerCentroid {
                embedding,
                observations: 1,
            });
            (index, 1.0, true)
        } else if let Some((index, similarity)) = best {
            // The configured memory bound is strict. Once full, choose the
            // nearest known speaker but do not contaminate its centroid with a
            // below-threshold observation.
            (index, similarity, false)
        } else {
            return Err(SpeakerError::EmptyEmbedding);
        };
        self.previous_speaker = Some(index);
        Ok(SpeakerAssignment {
            speaker_id: format!("speaker-{:02}", index + 1),
            similarity,
            is_new,
        })
    }

    fn update_centroid(&mut self, index: usize, embedding: &[f32]) -> Result<(), SpeakerError> {
        let centroid = &mut self.centroids[index];
        // Cap historical weight so a speaker profile can slowly adapt to a
        // changed microphone or room without reacting to one noisy segment.
        let history = centroid.observations.min(19) as f32;
        let incoming_weight = 1.0 / (history + 1.0);
        for (mean, value) in centroid.embedding.iter_mut().zip(embedding) {
            *mean = *mean * (1.0 - incoming_weight) + *value * incoming_weight;
        }
        centroid.embedding = normalize_embedding(std::mem::take(&mut centroid.embedding))?;
        centroid.observations = centroid.observations.saturating_add(1);
        Ok(())
    }
}

/// Combines one model session with its generation-local online tracker.
#[derive(Debug)]
pub struct OnlineSpeakerDiarizer {
    model: SpeakerEmbeddingModel,
    tracker: OnlineSpeakerTracker,
}

impl OnlineSpeakerDiarizer {
    pub fn from_file(
        model_path: impl AsRef<Path>,
        intra_threads: usize,
        tracker: TrackerConfig,
    ) -> Result<Self, SpeakerError> {
        Ok(Self {
            model: SpeakerEmbeddingModel::from_file(model_path, intra_threads)?,
            tracker: OnlineSpeakerTracker::new(tracker)?,
        })
    }

    pub fn identify(&mut self, samples: &[i16]) -> Result<SpeakerAssignment, SpeakerError> {
        let embedding = self.model.extract(samples)?;
        self.tracker.assign(&embedding)
    }

    pub fn reset(&mut self) {
        self.tracker.reset();
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Complex {
    re: f32,
    im: f32,
}

#[derive(Debug)]
struct FbankExtractor {
    window: [f32; FRAME_SAMPLES],
    mel_filters: Vec<Vec<(usize, f32)>>,
}

impl FbankExtractor {
    fn new() -> Self {
        let window = std::array::from_fn(|index| {
            let phase = 2.0 * PI * index as f32 / (FRAME_SAMPLES - 1) as f32;
            (0.5 - 0.5 * phase.cos()).powf(0.85)
        });
        let mel_low = mel_scale(20.0);
        let mel_high = mel_scale(SAMPLE_RATE_HZ as f32 / 2.0);
        let delta = (mel_high - mel_low) / (FEATURE_DIMENSIONS + 1) as f32;
        let bin_width = SAMPLE_RATE_HZ as f32 / FFT_SIZE as f32;
        let mel_filters = (0..FEATURE_DIMENSIONS)
            .map(|band| {
                let left = mel_low + band as f32 * delta;
                let middle = left + delta;
                let right = middle + delta;
                (0..FFT_BINS)
                    .filter_map(|bin| {
                        let mel = mel_scale(bin as f32 * bin_width);
                        let weight = if mel > left && mel <= middle {
                            (mel - left) / (middle - left)
                        } else if mel > middle && mel < right {
                            (right - mel) / (right - middle)
                        } else {
                            0.0
                        };
                        (weight > 0.0).then_some((bin, weight))
                    })
                    .collect()
            })
            .collect();
        Self {
            window,
            mel_filters,
        }
    }

    fn compute(&self, samples: &[i16]) -> Vec<f32> {
        let mut normalized = samples
            .iter()
            .map(|sample| f32::from(*sample) / 32_768.0)
            .collect::<Vec<_>>();
        if normalized.len() < FRAME_SAMPLES {
            normalized.resize(FRAME_SAMPLES, 0.0);
        }
        let frames = 1 + (normalized.len() - FRAME_SAMPLES) / FRAME_SHIFT_SAMPLES;
        let mut features = Vec::with_capacity(frames * FEATURE_DIMENSIONS);
        let mut fft = [Complex::default(); FFT_SIZE];
        for frame_index in 0..frames {
            let start = frame_index * FRAME_SHIFT_SAMPLES;
            let frame = &normalized[start..start + FRAME_SAMPLES];
            let mean = frame.iter().sum::<f32>() / FRAME_SAMPLES as f32;
            let mut previous = frame[0] - mean;
            for index in 0..FRAME_SAMPLES {
                let centered = frame[index] - mean;
                let emphasized = if index == 0 {
                    centered - 0.97 * centered
                } else {
                    centered - 0.97 * previous
                };
                previous = centered;
                fft[index] = Complex {
                    re: emphasized * self.window[index],
                    im: 0.0,
                };
            }
            fft[FRAME_SAMPLES..].fill(Complex::default());
            fft_in_place(&mut fft);
            let power = std::array::from_fn::<_, FFT_BINS, _>(|bin| {
                fft[bin].re.mul_add(fft[bin].re, fft[bin].im * fft[bin].im)
            });
            for filter in &self.mel_filters {
                let energy = filter
                    .iter()
                    .map(|(bin, weight)| power[*bin] * weight)
                    .sum::<f32>()
                    .max(f32::EPSILON);
                features.push(energy.ln());
            }
        }

        // 3D-Speaker inference applies utterance-level cepstral mean
        // normalization after Kaldi fbank extraction.
        for dimension in 0..FEATURE_DIMENSIONS {
            let mean = (0..frames)
                .map(|frame| features[frame * FEATURE_DIMENSIONS + dimension])
                .sum::<f32>()
                / frames as f32;
            for frame in 0..frames {
                features[frame * FEATURE_DIMENSIONS + dimension] -= mean;
            }
        }
        features
    }
}

fn mel_scale(frequency: f32) -> f32 {
    1127.0 * (1.0 + frequency / 700.0).ln()
}

fn fft_in_place(values: &mut [Complex; FFT_SIZE]) {
    for index in 1..FFT_SIZE {
        let reversed = index.reverse_bits() >> (usize::BITS - FFT_SIZE.trailing_zeros());
        if index < reversed {
            values.swap(index, reversed);
        }
    }
    let mut length = 2;
    while length <= FFT_SIZE {
        let angle = -2.0 * PI / length as f32;
        let root = Complex {
            re: angle.cos(),
            im: angle.sin(),
        };
        for start in (0..FFT_SIZE).step_by(length) {
            let mut twiddle = Complex { re: 1.0, im: 0.0 };
            for offset in 0..length / 2 {
                let even = values[start + offset];
                let odd = multiply(values[start + offset + length / 2], twiddle);
                values[start + offset] = Complex {
                    re: even.re + odd.re,
                    im: even.im + odd.im,
                };
                values[start + offset + length / 2] = Complex {
                    re: even.re - odd.re,
                    im: even.im - odd.im,
                };
                twiddle = multiply(twiddle, root);
            }
        }
        length *= 2;
    }
}

fn multiply(left: Complex, right: Complex) -> Complex {
    Complex {
        re: left.re.mul_add(right.re, -(left.im * right.im)),
        im: left.re.mul_add(right.im, left.im * right.re),
    }
}

fn normalize_embedding(mut embedding: Vec<f32>) -> Result<Vec<f32>, SpeakerError> {
    if embedding.is_empty() {
        return Err(SpeakerError::EmptyEmbedding);
    }
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err(SpeakerError::NonFiniteEmbedding);
    }
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if !norm.is_finite() || norm <= f32::EPSILON {
        return Err(SpeakerError::EmptyEmbedding);
    }
    for value in &mut embedding {
        *value /= norm;
    }
    Ok(embedding)
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() {
        return -1.0;
    }
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fbank_is_finite_mean_normalized_and_has_expected_shape() {
        let samples = (0..SAMPLE_RATE_HZ as usize)
            .map(|index| {
                let phase = 2.0 * PI * 440.0 * index as f32 / SAMPLE_RATE_HZ as f32;
                (phase.sin() * 12_000.0) as i16
            })
            .collect::<Vec<_>>();
        let features = FbankExtractor::new().compute(&samples);
        let frames = 1 + (samples.len() - FRAME_SAMPLES) / FRAME_SHIFT_SAMPLES;
        assert_eq!(features.len(), frames * FEATURE_DIMENSIONS);
        assert!(features.iter().all(|value| value.is_finite()));
        for dimension in 0..FEATURE_DIMENSIONS {
            let mean = (0..frames)
                .map(|frame| features[frame * FEATURE_DIMENSIONS + dimension])
                .sum::<f32>()
                / frames as f32;
            assert!(mean.abs() < 1e-4, "dimension {dimension} mean was {mean}");
        }
    }

    #[test]
    fn tracker_reuses_close_voiceprints_and_bounds_new_speakers() {
        let mut tracker = OnlineSpeakerTracker::new(TrackerConfig {
            similarity_threshold: 0.8,
            same_speaker_hysteresis: 0.04,
            max_speakers: 2,
        })
        .unwrap();
        assert_eq!(
            tracker.assign(&[1.0, 0.0]).unwrap().speaker_id,
            "speaker-01"
        );
        let same = tracker.assign(&[0.98, 0.05]).unwrap();
        assert_eq!(same.speaker_id, "speaker-01");
        assert!(!same.is_new);
        assert_eq!(
            tracker.assign(&[0.0, 1.0]).unwrap().speaker_id,
            "speaker-02"
        );
        assert_eq!(tracker.speaker_count(), 2);
        assert_eq!(
            tracker.assign(&[-1.0, 0.0]).unwrap().speaker_id,
            "speaker-02"
        );
        assert_eq!(tracker.speaker_count(), 2);
    }

    #[test]
    fn reset_restores_arrival_order_ids() {
        let mut tracker = OnlineSpeakerTracker::new(TrackerConfig::default()).unwrap();
        tracker.assign(&[1.0, 0.0]).unwrap();
        tracker.assign(&[0.0, 1.0]).unwrap();
        tracker.reset();
        let first = tracker.assign(&[0.0, 1.0]).unwrap();
        assert_eq!(first.speaker_id, "speaker-01");
        assert_eq!(tracker.speaker_count(), 1);
    }

    #[test]
    #[ignore = "requires the release ERes2NetV2 ONNX asset"]
    fn exported_eres2netv2_model_runs_with_native_features() {
        let model_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../models/3D-Speaker-ERes2NetV2/speaker_embedding.onnx");
        assert!(model_path.is_file(), "missing {}", model_path.display());
        let samples = (0..SAMPLE_RATE_HZ as usize)
            .map(|index| {
                let time = index as f32 / SAMPLE_RATE_HZ as f32;
                let sample = (2.0 * PI * 180.0 * time).sin() * 9_000.0
                    + (2.0 * PI * 720.0 * time).sin() * 2_000.0;
                sample as i16
            })
            .collect::<Vec<_>>();
        let mut model = SpeakerEmbeddingModel::from_file(model_path, 2).unwrap();
        let embedding = model.extract(&samples).unwrap();
        assert_eq!(embedding.len(), EMBEDDING_DIMENSIONS);
        assert!(embedding.iter().all(|value| value.is_finite()));
        let norm = embedding
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "embedding norm was {norm}");
    }
}
