mod adapter;
mod profile;
mod types;

pub use adapter::TranslationAdapter;
pub use profile::{build_translation_messages, is_probable_translation_context_leak};
pub use types::{TranslationOptions, TranslationProvider, TranslationResult};
pub use xrtranslate_prompt::{
    PromptCondition, PromptGraphError, PromptLink, PromptMessage, PromptMessageRole, PromptNode,
    PromptNodeGraph, PromptNodeKind, PromptProviderTarget, PromptTemplateLibrary,
    PromptTemplateProfile, PromptTurn, PromptVariable, SurroundingSource, TranslationPromptBlock,
    TranslationPromptContext,
};
