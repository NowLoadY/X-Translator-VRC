//! Streaming media-audio decoding for meeting and media player imports.
//!
//! Files are decoded a packet at a time, downmixed to mono, continuously
//! resampled to 16 kHz, and emitted in small `Vec<f32>` chunks. The worker
//! never keeps the complete recording in memory.

mod api;
mod decode;
mod mpv_extract;
mod stream;
mod types;

pub use api::{AudioImportHandle, import_audio_file};
#[allow(unused_imports)]
pub use types::{
    AudioFileInfo, AudioImportError, AudioImportEvent, AudioImportOptions, AudioImportOutcome,
    AudioImportPacing, AudioImportProgress, AudioImportStage, IMPORT_SAMPLE_RATE,
};
