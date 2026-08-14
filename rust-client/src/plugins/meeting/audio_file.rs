//! Streaming audio-file decoding for meeting imports.
//!
//! Files are decoded a packet at a time, downmixed to mono, continuously
//! resampled to 16 kHz, and emitted in small `Vec<f32>` chunks. The worker
//! never keeps the complete recording in memory.

use std::collections::VecDeque;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use audioadapter_buffers::direct::InterleavedSlice;
use crossbeam_channel::{Receiver, SendTimeoutError, Sender, unbounded};
use rubato::{Fft, FixedSync, Indexing, Resampler};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub const IMPORT_SAMPLE_RATE: u32 = 16_000;
const RESAMPLER_INPUT_FRAMES: usize = 1024;
const SEND_POLL_INTERVAL: Duration = Duration::from_millis(50);
const PROGRESS_AUDIO_INTERVAL_FRAMES: u64 = 5 * IMPORT_SAMPLE_RATE as u64;

/// Controls how quickly decoded audio is delivered to the recognition session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioImportPacing {
    /// Match the recording's playback clock. This is safe with bounded or
    /// unbounded output channels and is the recommended/default backend mode.
    Realtime,
    /// Decode as quickly as the receiver consumes data. This mode requires a
    /// bounded output channel so a long recording cannot be queued in memory.
    AsFastAsPossible,
}

#[derive(Debug, Clone)]
pub struct AudioImportOptions {
    /// Number of mono 16 kHz frames per emitted message. 1600 is 100 ms.
    pub chunk_frames: usize,
    pub pacing: AudioImportPacing,
}

impl Default for AudioImportOptions {
    fn default() -> Self {
        Self {
            chunk_frames: 1_600,
            pacing: AudioImportPacing::Realtime,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AudioFileInfo {
    pub path: PathBuf,
    pub codec: String,
    pub source_sample_rate: u32,
    pub source_channels: usize,
    pub total_source_frames: Option<u64>,
    pub duration: Option<Duration>,
    pub output_sample_rate: u32,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct AudioImportProgress {
    pub decoded_source_frames: u64,
    pub total_source_frames: Option<u64>,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub fraction: Option<f32>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AudioImportEvent {
    Started(AudioFileInfo),
    Progress(AudioImportProgress),
    Completed { output_frames: u64 },
    Stopped { output_frames: u64 },
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioImportOutcome {
    Completed { output_frames: u64 },
    Stopped { output_frames: u64 },
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum AudioImportError {
    InvalidOptions(String),
    Io(io::Error),
    Unsupported(String),
    Decode(String),
    Resample(String),
    OutputClosed,
    WorkerPanicked,
    Cancelled,
}

impl fmt::Display for AudioImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions(message) => write!(formatter, "invalid import options: {message}"),
            Self::Io(error) => write!(formatter, "audio file I/O failed: {error}"),
            Self::Unsupported(message) => write!(formatter, "unsupported audio file: {message}"),
            Self::Decode(message) => write!(formatter, "audio decoding failed: {message}"),
            Self::Resample(message) => write!(formatter, "audio resampling failed: {message}"),
            Self::OutputClosed => formatter.write_str("audio consumer disconnected"),
            Self::WorkerPanicked => formatter.write_str("audio import worker panicked"),
            Self::Cancelled => formatter.write_str("audio import stopped"),
        }
    }
}

impl std::error::Error for AudioImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for AudioImportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Owns a running import worker. Dropping the handle requests cancellation.
pub struct AudioImportHandle {
    stop_requested: Arc<AtomicBool>,
    events: Receiver<AudioImportEvent>,
    _worker: Option<JoinHandle<Result<AudioImportOutcome, AudioImportError>>>,
}

impl AudioImportHandle {
    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
    }

    pub fn events(&self) -> &Receiver<AudioImportEvent> {
        &self.events
    }
}

impl Drop for AudioImportHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Start decoding `path` on a dedicated worker thread.
///
/// The output sender should be the same `Sender<Vec<f32>>` consumed by the
/// recognition session. No frame is silently discarded. Realtime mode paces
/// output; fast mode is rejected unless `audio_tx` is bounded.
pub fn import_audio_file(
    path: impl AsRef<Path>,
    audio_tx: Sender<Vec<f32>>,
    options: AudioImportOptions,
) -> Result<AudioImportHandle, AudioImportError> {
    validate_options(&audio_tx, &options)?;

    let path = path.as_ref().to_path_buf();
    let stop_requested = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop_requested);
    let (event_tx, events) = unbounded();
    let worker = thread::Builder::new()
        .name("meeting-audio-import".into())
        .spawn(move || {
            let sent_frames = AtomicU64::new(0);
            let result = run_import(
                &path,
                audio_tx,
                options,
                &worker_stop,
                &sent_frames,
                &event_tx,
            );
            let outcome = match result {
                Ok(output_frames) => AudioImportOutcome::Completed { output_frames },
                Err(AudioImportError::Cancelled) => AudioImportOutcome::Stopped {
                    output_frames: sent_frames.load(Ordering::Acquire),
                },
                Err(error) => {
                    let _ = event_tx.send(AudioImportEvent::Error(error.to_string()));
                    return Err(error);
                }
            };
            let event = match outcome {
                AudioImportOutcome::Completed { output_frames } => {
                    AudioImportEvent::Completed { output_frames }
                }
                AudioImportOutcome::Stopped { output_frames } => {
                    AudioImportEvent::Stopped { output_frames }
                }
            };
            let _ = event_tx.send(event);
            Ok(outcome)
        })
        .map_err(AudioImportError::Io)?;

    Ok(AudioImportHandle {
        stop_requested,
        events,
        _worker: Some(worker),
    })
}

fn validate_options(
    audio_tx: &Sender<Vec<f32>>,
    options: &AudioImportOptions,
) -> Result<(), AudioImportError> {
    if options.chunk_frames == 0 {
        return Err(AudioImportError::InvalidOptions(
            "chunk_frames must be greater than zero".into(),
        ));
    }
    if options.pacing == AudioImportPacing::AsFastAsPossible && audio_tx.capacity().is_none() {
        return Err(AudioImportError::InvalidOptions(
            "fast imports require a bounded audio channel".into(),
        ));
    }
    Ok(())
}

fn run_import(
    path: &Path,
    audio_tx: Sender<Vec<f32>>,
    options: AudioImportOptions,
    stop_requested: &AtomicBool,
    sent_frames: &AtomicU64,
    event_tx: &Sender<AudioImportEvent>,
) -> Result<u64, AudioImportError> {
    let file = File::open(path)?;
    let media_source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            media_source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(map_probe_error)?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| AudioImportError::Unsupported("no decodable audio track".into()))?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let codec_name = format!("{:?}", codec_params.codec);
    let total_source_frames = codec_params.n_frames;
    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(map_decode_error)?;

    let mut source_format = None;
    let mut resampler = None;
    let mut sink = ChunkSink::new(
        audio_tx,
        options.chunk_frames,
        options.pacing,
        stop_requested,
        sent_frames,
    );
    let mut decoded_source_frames = 0_u64;
    let mut next_progress_frame = 0_u64;

    loop {
        check_cancelled(stop_requested)?;
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error)) if error.kind() == io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                return Err(AudioImportError::Decode(
                    "mid-stream format reset is not supported".into(),
                ));
            }
            Err(error) => return Err(map_decode_error(error)),
        };
        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A damaged packet need not make the rest of a long meeting unusable.
            Err(SymphoniaError::DecodeError(message)) => {
                log::warn!("Skipping damaged imported-audio packet: {message}");
                continue;
            }
            Err(error) => return Err(map_decode_error(error)),
        };
        let spec = *decoded.spec();
        let source_rate = spec.rate;
        let channels = spec.channels.count();
        if channels == 0 || source_rate == 0 {
            return Err(AudioImportError::Decode(
                "invalid decoded audio format".into(),
            ));
        }

        match source_format {
            None => {
                source_format = Some((source_rate, channels));
                resampler = Some(StreamingResampler::new(source_rate)?);
                let duration = duration_from_frames(total_source_frames, source_rate);
                let _ = event_tx.send(AudioImportEvent::Started(AudioFileInfo {
                    path: path.to_path_buf(),
                    codec: codec_name.clone(),
                    source_sample_rate: source_rate,
                    source_channels: channels,
                    total_source_frames,
                    duration,
                    output_sample_rate: IMPORT_SAMPLE_RATE,
                }));
            }
            Some((expected_rate, expected_channels))
                if expected_rate != source_rate || expected_channels != channels =>
            {
                return Err(AudioImportError::Decode(format!(
                    "audio format changed from {expected_rate} Hz/{expected_channels} channels to {source_rate} Hz/{channels} channels"
                )));
            }
            _ => {}
        }

        let mut converted = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        converted.copy_interleaved_ref(decoded);
        let mono = downmix_interleaved(converted.samples(), channels);
        decoded_source_frames += mono.len() as u64;
        resampler
            .as_mut()
            .expect("resampler is initialized with the source format")
            .push(&mono, &mut sink)?;

        if decoded_source_frames >= next_progress_frame {
            let (source_rate, _) = source_format.expect("source format is initialized");
            send_progress(
                event_tx,
                decoded_source_frames,
                total_source_frames,
                source_rate,
            );
            next_progress_frame = decoded_source_frames
                + (PROGRESS_AUDIO_INTERVAL_FRAMES * source_rate as u64 / IMPORT_SAMPLE_RATE as u64)
                    .max(1);
        }
    }

    let Some((source_rate, _)) = source_format else {
        return Err(AudioImportError::Decode(
            "the selected track contained no audio frames".into(),
        ));
    };
    resampler
        .as_mut()
        .expect("resampler exists for a decoded stream")
        .finish(&mut sink)?;
    sink.finish()?;
    send_progress(
        event_tx,
        decoded_source_frames,
        total_source_frames,
        source_rate,
    );
    Ok(sink.sent_frames)
}

fn downmix_interleaved(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }
    let scale = 1.0 / channels as f32;
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() * scale)
        .collect()
}

fn send_progress(
    event_tx: &Sender<AudioImportEvent>,
    decoded_source_frames: u64,
    total_source_frames: Option<u64>,
    source_rate: u32,
) {
    let position =
        duration_from_frames(Some(decoded_source_frames), source_rate).unwrap_or(Duration::ZERO);
    let duration = duration_from_frames(total_source_frames, source_rate);
    let fraction = total_source_frames
        .filter(|total| *total > 0)
        .map(|total| (decoded_source_frames as f64 / total as f64).clamp(0.0, 1.0) as f32);
    let _ = event_tx.send(AudioImportEvent::Progress(AudioImportProgress {
        decoded_source_frames,
        total_source_frames,
        position,
        duration,
        fraction,
    }));
}

fn duration_from_frames(frames: Option<u64>, sample_rate: u32) -> Option<Duration> {
    frames.map(|frames| Duration::from_secs_f64(frames as f64 / sample_rate as f64))
}

fn check_cancelled(stop_requested: &AtomicBool) -> Result<(), AudioImportError> {
    if stop_requested.load(Ordering::Acquire) {
        Err(AudioImportError::Cancelled)
    } else {
        Ok(())
    }
}

struct ChunkSink<'a> {
    sender: Sender<Vec<f32>>,
    chunk_frames: usize,
    pending: VecDeque<f32>,
    pacing: AudioImportPacing,
    pacing_started: Instant,
    stop_requested: &'a AtomicBool,
    sent_frames_counter: &'a AtomicU64,
    sent_frames: u64,
}

impl<'a> ChunkSink<'a> {
    fn new(
        sender: Sender<Vec<f32>>,
        chunk_frames: usize,
        pacing: AudioImportPacing,
        stop_requested: &'a AtomicBool,
        sent_frames_counter: &'a AtomicU64,
    ) -> Self {
        Self {
            sender,
            chunk_frames,
            pending: VecDeque::with_capacity(chunk_frames * 2),
            pacing,
            pacing_started: Instant::now(),
            stop_requested,
            sent_frames_counter,
            sent_frames: 0,
        }
    }

    fn push(&mut self, samples: &[f32]) -> Result<(), AudioImportError> {
        self.pending.extend(samples.iter().copied());
        while self.pending.len() >= self.chunk_frames {
            let chunk = self.pending.drain(..self.chunk_frames).collect();
            self.send(chunk)?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AudioImportError> {
        if !self.pending.is_empty() {
            let chunk = self.pending.drain(..).collect();
            self.send(chunk)?;
        }
        Ok(())
    }

    fn send(&mut self, mut chunk: Vec<f32>) -> Result<(), AudioImportError> {
        let chunk_frames = chunk.len() as u64;
        loop {
            check_cancelled(self.stop_requested)?;
            match self.sender.send_timeout(chunk, SEND_POLL_INTERVAL) {
                Ok(()) => break,
                Err(SendTimeoutError::Timeout(returned)) => chunk = returned,
                Err(SendTimeoutError::Disconnected(_)) => {
                    return Err(AudioImportError::OutputClosed);
                }
            }
        }
        self.sent_frames += chunk_frames;
        self.sent_frames_counter
            .store(self.sent_frames, Ordering::Release);

        if self.pacing == AudioImportPacing::Realtime {
            let deadline = self.pacing_started
                + Duration::from_secs_f64(self.sent_frames as f64 / IMPORT_SAMPLE_RATE as f64);
            while Instant::now() < deadline {
                check_cancelled(self.stop_requested)?;
                thread::sleep((deadline - Instant::now()).min(SEND_POLL_INTERVAL));
            }
        }
        Ok(())
    }
}

enum StreamingResampler {
    Passthrough,
    Fft {
        inner: Box<Fft<f32>>,
        source_rate: u32,
        pending: VecDeque<f32>,
        trim_remaining: usize,
        input_frames: u64,
        output_frames: u64,
    },
}

impl StreamingResampler {
    fn new(source_rate: u32) -> Result<Self, AudioImportError> {
        if source_rate == IMPORT_SAMPLE_RATE {
            return Ok(Self::Passthrough);
        }
        let inner = Fft::<f32>::new(
            source_rate as usize,
            IMPORT_SAMPLE_RATE as usize,
            RESAMPLER_INPUT_FRAMES,
            1,
            FixedSync::Input,
        )
        .map_err(|error| AudioImportError::Resample(error.to_string()))?;
        let trim_remaining = inner.output_delay();
        Ok(Self::Fft {
            inner: Box::new(inner),
            source_rate,
            pending: VecDeque::new(),
            trim_remaining,
            input_frames: 0,
            output_frames: 0,
        })
    }

    fn push(&mut self, samples: &[f32], sink: &mut ChunkSink<'_>) -> Result<(), AudioImportError> {
        match self {
            Self::Passthrough => sink.push(samples),
            Self::Fft {
                inner,
                pending,
                trim_remaining,
                input_frames,
                output_frames,
                ..
            } => {
                *input_frames += samples.len() as u64;
                pending.extend(samples.iter().copied());
                while pending.len() >= inner.input_frames_next() {
                    let input_len = inner.input_frames_next();
                    let input: Vec<f32> = pending.drain(..input_len).collect();
                    let output = process_resampler(inner, &input, None)?;
                    emit_resampled(output, trim_remaining, output_frames, None, sink)?;
                }
                Ok(())
            }
        }
    }

    fn finish(&mut self, sink: &mut ChunkSink<'_>) -> Result<(), AudioImportError> {
        let Self::Fft {
            inner,
            source_rate,
            pending,
            trim_remaining,
            input_frames,
            output_frames,
        } = self
        else {
            return Ok(());
        };

        let expected_output = ((*input_frames as u128 * IMPORT_SAMPLE_RATE as u128)
            .div_ceil(*source_rate as u128)) as u64;
        if !pending.is_empty() {
            let valid = pending.len();
            let required = inner.input_frames_next();
            let mut input: Vec<f32> = pending.drain(..).collect();
            input.resize(required, 0.0);
            let output = process_resampler(inner, &input, Some(valid))?;
            emit_resampled(
                output,
                trim_remaining,
                output_frames,
                Some(expected_output),
                sink,
            )?;
        }

        while *output_frames < expected_output {
            let input = vec![0.0; inner.input_frames_next()];
            let output = process_resampler(inner, &input, Some(0))?;
            let before = *output_frames;
            emit_resampled(
                output,
                trim_remaining,
                output_frames,
                Some(expected_output),
                sink,
            )?;
            if *output_frames == before {
                return Err(AudioImportError::Resample(
                    "resampler could not flush its delayed output".into(),
                ));
            }
        }
        Ok(())
    }
}

fn process_resampler(
    resampler: &mut Fft<f32>,
    input: &[f32],
    partial_len: Option<usize>,
) -> Result<Vec<f32>, AudioImportError> {
    let input_adapter = InterleavedSlice::new(input, 1, input.len())
        .map_err(|error| AudioImportError::Resample(error.to_string()))?;
    let output_capacity = resampler.output_frames_max();
    let mut output = vec![0.0; output_capacity];
    let mut output_adapter = InterleavedSlice::new_mut(&mut output, 1, output_capacity)
        .map_err(|error| AudioImportError::Resample(error.to_string()))?;
    let indexing = partial_len.map(|partial_len| Indexing {
        partial_len: Some(partial_len),
        ..Indexing::default()
    });
    let (_, written) = resampler
        .process_into_buffer(&input_adapter, &mut output_adapter, indexing.as_ref())
        .map_err(|error| AudioImportError::Resample(error.to_string()))?;
    output.truncate(written);
    Ok(output)
}

fn emit_resampled(
    mut samples: Vec<f32>,
    trim_remaining: &mut usize,
    output_frames: &mut u64,
    output_limit: Option<u64>,
    sink: &mut ChunkSink<'_>,
) -> Result<(), AudioImportError> {
    if *trim_remaining > 0 {
        let trim = (*trim_remaining).min(samples.len());
        samples.drain(..trim);
        *trim_remaining -= trim;
    }
    if let Some(limit) = output_limit {
        samples.truncate(limit.saturating_sub(*output_frames) as usize);
    }
    *output_frames += samples.len() as u64;
    sink.push(&samples)
}

fn map_probe_error(error: SymphoniaError) -> AudioImportError {
    match error {
        SymphoniaError::IoError(error) => AudioImportError::Io(error),
        SymphoniaError::Unsupported(message) => AudioImportError::Unsupported(message.into()),
        other => AudioImportError::Unsupported(other.to_string()),
    }
}

fn map_decode_error(error: SymphoniaError) -> AudioImportError {
    match error {
        SymphoniaError::IoError(error) => AudioImportError::Io(error),
        SymphoniaError::Unsupported(message) => AudioImportError::Unsupported(message.into()),
        other => AudioImportError::Decode(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::bounded;

    #[test]
    fn downmixes_interleaved_stereo() {
        let mono = downmix_interleaved(&[1.0, -1.0, 0.25, 0.75], 2);
        assert_eq!(mono, vec![0.0, 0.5]);
    }

    #[test]
    fn continuous_resampling_preserves_expected_duration() {
        let stop = AtomicBool::new(false);
        let sent_frames = AtomicU64::new(0);
        let (tx, rx) = bounded(32);
        let mut sink = ChunkSink::new(
            tx,
            160,
            AudioImportPacing::AsFastAsPossible,
            &stop,
            &sent_frames,
        );
        let mut resampler = StreamingResampler::new(48_000).unwrap();
        let input: Vec<f32> = (0..4_800)
            .map(|frame| ((frame as f32 / 48_000.0) * 440.0 * std::f32::consts::TAU).sin())
            .collect();
        for packet in input.chunks(317) {
            resampler.push(packet, &mut sink).unwrap();
        }
        resampler.finish(&mut sink).unwrap();
        sink.finish().unwrap();
        drop(sink);

        let output: Vec<f32> = rx.into_iter().flatten().collect();
        assert_eq!(output.len(), 1_600);
        assert!(output.iter().all(|sample| sample.is_finite()));
        assert!(output.iter().any(|sample| sample.abs() > 0.01));
    }

    #[test]
    fn fast_mode_rejects_unbounded_output() {
        let (tx, _rx) = unbounded();
        let options = AudioImportOptions {
            pacing: AudioImportPacing::AsFastAsPossible,
            ..AudioImportOptions::default()
        };
        assert!(matches!(
            validate_options(&tx, &options),
            Err(AudioImportError::InvalidOptions(_))
        ));
    }
}
