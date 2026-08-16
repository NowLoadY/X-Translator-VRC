use super::PluginSessionOwner;

/// Controls which application-wide presentation channels receive a plugin
/// session. Domain subscribers still receive the typed session event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOutputPolicy {
    /// Feed the normal history, overlay, TTS and external caption outputs.
    Host,
    /// Keep results out of host presentation and deliver them to subscribers.
    PluginOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeakerRecognitionPolicy {
    HostDefault,
    Enabled,
    Disabled,
}

impl SpeakerRecognitionPolicy {
    pub const fn resolve(self, host_default: bool) -> bool {
        match self {
            Self::HostDefault => host_default,
            Self::Enabled => true,
            Self::Disabled => false,
        }
    }
}

/// Uniform configuration supplied by a plugin before it uses recognition and
/// translation infrastructure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSessionBinding {
    pub owner: PluginSessionOwner,
    pub output_policy: SessionOutputPolicy,
    pub speaker_recognition: SpeakerRecognitionPolicy,
    pub host_tts: bool,
    pub external_audio_gate: bool,
    pub finish_when_audio_ends: bool,
}

impl PluginSessionBinding {
    pub const fn publish_to_host_outputs(&self) -> bool {
        matches!(self.output_policy, SessionOutputPolicy::Host)
    }

    pub const fn speaker_recognition_enabled(&self, host_default: bool) -> bool {
        self.speaker_recognition.resolve(host_default)
    }
}

/// Implemented by plugins that acquire the exclusive translation capability.
/// The infrastructure consumes only this neutral binding, never plugin types.
pub trait TranslationSessionPlugin {
    fn translation_session_binding(&self) -> Option<PluginSessionBinding>;
}
