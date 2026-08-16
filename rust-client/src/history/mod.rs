//! Recognition and translation history models plus deterministic stream merging.

mod merge;
mod model;

pub(crate) use merge::{
    collect_recognition_window, merge_stream_recognition, merge_stream_translation,
};
pub(crate) use model::{
    PendingFinalAsr, PendingRecognitionWindow, RecognitionHistoryEntry, TranslationHistoryEntry,
};
