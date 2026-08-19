use xrtranslate_prompt::PromptProviderTarget;

use super::{TranslationProfile, output::clean_openai_compatible};

pub(super) static PROFILE: TranslationProfile = TranslationProfile {
    target: PromptProviderTarget::OpenAiCompatible,
    temperature: 0.7,
    apply_sampling: |_, _| {},
    clean_output: clean_openai_compatible,
};
