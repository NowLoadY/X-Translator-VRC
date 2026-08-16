use crate::{client_settings::CaptureSource, streaming::RevisableText};
use xrtranslate_protocol::CorpusTermMatch;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecognitionHistoryEntry {
    pub(crate) stream_id: Option<u64>,
    pub(crate) live: bool,
    pub(crate) text: String,
    pub(crate) turn_id: String,
    pub(crate) speaker_id: String,
    pub(crate) source_start_ms: f64,
    pub(crate) source_end_ms: f64,
    pub(crate) activation_matches: Vec<CorpusTermMatch>,
    pub(crate) context_matches: Vec<CorpusTermMatch>,
    pub(crate) revisable: bool,
    pub(crate) overlap_ratio: f32,
    pub(crate) revision: Option<RevisableText>,
}

pub(crate) struct PendingRecognitionWindow {
    pub(crate) stream_id: u64,
    pub(super) continuous: bool,
    pub(super) turn_id: String,
    pub(super) segment_count: u32,
    pub(super) segments: Vec<(u32, RecognitionHistoryEntry)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingFinalAsr {
    pub(crate) text: String,
    pub(crate) turn_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranslationHistoryEntry {
    pub(crate) turn_id: String,
    pub(crate) segment_index: u32,
    pub(crate) stream_id: Option<u64>,
    pub(crate) audio_source: CaptureSource,
    pub(crate) live: bool,
    pub(crate) source: String,
    pub(crate) translated: String,
    pub(crate) speaker_id: String,
    pub(crate) source_start_ms: f64,
    pub(crate) source_end_ms: f64,
    pub(crate) term_matches: Vec<CorpusTermMatch>,
    pub(crate) revisable: bool,
    pub(crate) overlap_ratio: f32,
    pub(crate) source_revision: Option<RevisableText>,
    pub(crate) translated_revision: Option<RevisableText>,
}

pub(crate) struct StreamMerge {
    pub(crate) entry: TranslationHistoryEntry,
    pub(crate) rolled_over: bool,
    pub(crate) changed: bool,
}
