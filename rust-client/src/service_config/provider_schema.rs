#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ProviderFieldEditor {
    Default,
    ModelLevel,
    UnsignedRange {
        minimum: u32,
        maximum: u32,
        speed: f64,
    },
    Options(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderFieldVisibility {
    Default,
    NativeModel,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ProviderFieldDescriptor {
    pub name: &'static str,
    pub label: &'static str,
    pub help: Option<&'static str>,
    pub editor: ProviderFieldEditor,
    visibility: ProviderFieldVisibility,
}

impl ProviderFieldDescriptor {
    pub(super) const fn is_visible(self, native_model: bool) -> bool {
        match self.visibility {
            ProviderFieldVisibility::Default => !native_model,
            ProviderFieldVisibility::NativeModel => native_model,
            ProviderFieldVisibility::Hidden => false,
        }
    }
}

const DEVICE_OPTIONS: &[&str] = &["cuda", "cpu", "mps", "auto"];

const PROVIDER_FIELDS: &[ProviderFieldDescriptor] = &[
    ProviderFieldDescriptor {
        name: "context_window_tokens",
        label: "Context tokens per request",
        help: Some("Input and output context available to each parallel model request."),
        editor: ProviderFieldEditor::UnsignedRange {
            minimum: 256,
            maximum: 32_768,
            speed: 128.0,
        },
        visibility: ProviderFieldVisibility::NativeModel,
    },
    ProviderFieldDescriptor {
        name: "max_tokens",
        label: "Max output tokens",
        help: Some("Maximum tokens generated for one result."),
        editor: ProviderFieldEditor::UnsignedRange {
            minimum: 16,
            maximum: 4_096,
            speed: 1.0,
        },
        visibility: ProviderFieldVisibility::NativeModel,
    },
    ProviderFieldDescriptor {
        name: "parallel_slots",
        label: "Parallel requests",
        help: Some(
            "Concurrent llama.cpp request slots. Total context cache is context tokens multiplied by this value.",
        ),
        editor: ProviderFieldEditor::UnsignedRange {
            minimum: 1,
            maximum: 16,
            speed: 1.0,
        },
        visibility: ProviderFieldVisibility::Hidden,
    },
    ProviderFieldDescriptor {
        name: "model_asset",
        label: "Level",
        help: None,
        editor: ProviderFieldEditor::ModelLevel,
        visibility: ProviderFieldVisibility::NativeModel,
    },
    ProviderFieldDescriptor {
        name: "supports_prompt_context",
        label: "Prompt context",
        help: None,
        editor: ProviderFieldEditor::Default,
        visibility: ProviderFieldVisibility::Hidden,
    },
    ProviderFieldDescriptor {
        name: "supports_language",
        label: "Language selection",
        help: None,
        editor: ProviderFieldEditor::Default,
        visibility: ProviderFieldVisibility::Hidden,
    },
    ProviderFieldDescriptor {
        name: "supports_prompt",
        label: "Custom prompt",
        help: None,
        editor: ProviderFieldEditor::Default,
        visibility: ProviderFieldVisibility::Hidden,
    },
    ProviderFieldDescriptor {
        name: "prompt_field",
        label: "Prompt field",
        help: None,
        editor: ProviderFieldEditor::Default,
        visibility: ProviderFieldVisibility::Hidden,
    },
    ProviderFieldDescriptor {
        name: "url",
        label: "Endpoint URL",
        help: None,
        editor: ProviderFieldEditor::Default,
        visibility: ProviderFieldVisibility::Default,
    },
    ProviderFieldDescriptor {
        name: "model",
        label: "Model",
        help: None,
        editor: ProviderFieldEditor::Default,
        visibility: ProviderFieldVisibility::Default,
    },
    ProviderFieldDescriptor {
        name: "device",
        label: "device",
        help: None,
        editor: ProviderFieldEditor::Options(DEVICE_OPTIONS),
        visibility: ProviderFieldVisibility::Default,
    },
];

pub(super) fn provider_field_descriptor(name: &str) -> Option<ProviderFieldDescriptor> {
    PROVIDER_FIELDS
        .iter()
        .copied()
        .find(|descriptor| descriptor.name == name)
}

#[cfg(test)]
mod tests {
    use super::{ProviderFieldEditor, provider_field_descriptor};

    #[test]
    fn native_visibility_matches_the_existing_provider_form() {
        assert!(
            provider_field_descriptor("model_asset")
                .unwrap()
                .is_visible(true)
        );
        assert!(
            provider_field_descriptor("context_window_tokens")
                .unwrap()
                .is_visible(true)
        );
        assert!(
            provider_field_descriptor("max_tokens")
                .unwrap()
                .is_visible(true)
        );
        assert!(!provider_field_descriptor("url").unwrap().is_visible(true));
        assert!(provider_field_descriptor("url").unwrap().is_visible(false));
        assert!(
            !provider_field_descriptor("parallel_slots")
                .unwrap()
                .is_visible(false)
        );
    }

    #[test]
    fn numeric_editor_keeps_context_range_and_speed() {
        assert_eq!(
            provider_field_descriptor("context_window_tokens")
                .unwrap()
                .editor,
            ProviderFieldEditor::UnsignedRange {
                minimum: 256,
                maximum: 32_768,
                speed: 128.0,
            }
        );
    }
}
