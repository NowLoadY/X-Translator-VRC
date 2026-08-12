// This is a desktop GUI executable. Use the Windows GUI subsystem in every
// build so double-clicking the executable never creates a transient console.
#![cfg_attr(windows, windows_subsystem = "windows")]

use crossbeam_channel::{Sender, unbounded};
use eframe::egui;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
};

mod app_update;
mod audio;
mod backend;
mod client_settings;
mod feature_access;
mod i18n;
mod model_install;
mod network;
mod osc;
mod overlay_ipc;
mod overlay_manager;
#[cfg(windows)]
mod overlay_native;
mod runtime_install;
mod service_config;
mod sys_info;
mod ui;
pub mod version;

use audio::{AudioSystem, InputConfigInfo, InputDevice};
use client_settings::{CaptureSource, ClientSettings};
use i18n::UiLanguage;
use network::{SessionEvent, SessionHandle, start_session};
use osc::{OscManager, OscSettings};
use ui::{NavigationState, Page};

#[derive(Clone, Debug, PartialEq)]
struct RecognitionHistoryEntry {
    text: String,
    turn_id: String,
    speaker_id: String,
    source_start_ms: f64,
    source_end_ms: f64,
    activation_matches: Vec<xrtranslate_protocol::CorpusTermMatch>,
    context_matches: Vec<xrtranslate_protocol::CorpusTermMatch>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingFinalAsr {
    text: String,
    turn_id: String,
}

#[derive(Clone, Debug, PartialEq)]
struct TranslationHistoryEntry {
    source: String,
    translated: String,
    speaker_id: String,
    source_start_ms: f64,
    source_end_ms: f64,
    term_matches: Vec<xrtranslate_protocol::CorpusTermMatch>,
}

const LANGUAGE_OPTIONS: &[(&str, &str)] = &[
    ("zh", "Chinese"),
    ("en", "English"),
    ("fr", "French"),
    ("pt", "Portuguese"),
    ("es", "Spanish"),
    ("ja", "Japanese"),
    ("ru", "Russian"),
    ("ko", "Korean"),
    ("th", "Thai"),
    ("it", "Italian"),
    ("de", "German"),
    ("vi", "Vietnamese"),
    ("id", "Indonesian"),
    ("pl", "Polish"),
    ("cs", "Czech"),
    ("nl", "Dutch"),
];

fn language_label(ui_language: UiLanguage, code: &str) -> &'static str {
    if code == "auto" {
        return i18n::tr(ui_language, "Auto (bidirectional)");
    }
    LANGUAGE_OPTIONS
        .iter()
        .find(|(value, _)| *value == code)
        .map(|(_, label)| i18n::tr(ui_language, label))
        .unwrap_or_else(|| i18n::tr(ui_language, "Unknown language"))
}

fn route_label(ui_language: UiLanguage, _source: &str, target: &str) -> &'static str {
    language_label(ui_language, target)
}

struct XRTranslateApp {
    audio_system: AudioSystem,
    devices: Vec<InputDevice>,
    selected_device_id: String,
    loopback_devices: Vec<InputDevice>,
    selected_loopback_device_id: String,
    capture_source: CaptureSource,
    selected_input_config: Option<InputConfigInfo>,
    is_translating: bool,
    audio_tx: Option<Sender<Vec<f32>>>,
    input_level: Arc<AtomicU32>,
    session: Option<SessionHandle>,
    event_tx: Sender<SessionEvent>,
    connection_status: String,
    partial_text: String,
    recognition_history: Vec<RecognitionHistoryEntry>,
    translations: Vec<TranslationHistoryEntry>,
    last_error: Option<String>,
    server_url: String,
    source_lang: String,
    target_lang: String,
    tts_enabled: bool,
    speaker_recognition_enabled: bool,
    osc_manager: OscManager,
    osc_draft: OscSettings,
    service_config: service_config::ServiceConfigEditor,
    backend_manager: backend::BackendManager,
    model_task_manager: model_install::NativeModelTaskManager,
    runtime_installer: runtime_install::RuntimeInstaller,
    app_update_manager: app_update::AppUpdateManager,
    backend_start_deadline: Option<std::time::Instant>,
    pub settings_section: ui::pages::settings::SettingsSection,
    pub modal_dialog: ui::modal::ModalDialog,
    pub first_run: bool,
    pub onboarding_page: usize,
    pub ui_language: UiLanguage,
    navigation: NavigationState,
    mute_self_pauses_translation: Arc<AtomicBool>,
    pub floating_subtitles_enabled: bool,
    pub floating_subtitles_max_count: usize,
    pub floating_subtitles_font_size: f64,
    pub overlay_manager: Arc<Mutex<overlay_manager::OverlayManager>>,
    shared_session_state: Arc<Mutex<SharedSessionState>>,
    overlay_enabled_atomic: Arc<AtomicBool>,
    overlay_max_count_atomic: Arc<AtomicUsize>,
    overlay_font_size_atomic: Arc<AtomicU32>,
}

#[derive(Default)]
struct SharedSessionState {
    connection_status: String,
    partial_text: String,
    pending_final_asr: Vec<PendingFinalAsr>,
    recognition_history: Vec<RecognitionHistoryEntry>,
    translations: Vec<TranslationHistoryEntry>,
    last_error: Option<String>,
    is_translating: bool,
}

impl Default for XRTranslateApp {
    fn default() -> Self {
        let audio_system = AudioSystem::new();
        let devices = audio_system.available_devices();
        let loopback_devices = audio_system.available_loopback_devices();
        let (event_tx, event_rx) = unbounded();
        let backend_manager = backend::BackendManager::load();
        let mut settings = ClientSettings::load(&backend_manager.project_root());
        settings.sanitize_devices(&devices, &loopback_devices);

        let osc_draft = settings.osc_settings.clone();
        let osc_manager = OscManager::new(osc_draft.clone());

        let selected_input_config = match settings.capture_source {
            CaptureSource::Microphone => {
                audio_system.input_config(&settings.selected_device_id).ok()
            }
            CaptureSource::SystemAudio => audio_system
                .loopback_config(&settings.selected_loopback_device_id)
                .ok(),
        };

        let shared_session_state = Arc::new(Mutex::new(SharedSessionState {
            connection_status: "Ready".into(),
            ..Default::default()
        }));
        let overlay_manager = Arc::new(Mutex::new(overlay_manager::OverlayManager::new()));
        let overlay_enabled_atomic = Arc::new(AtomicBool::new(settings.floating_subtitles_enabled));
        let overlay_max_count_atomic =
            Arc::new(AtomicUsize::new(settings.floating_subtitles_max_count));
        let overlay_font_size_atomic =
            Arc::new(AtomicU32::new(settings.floating_subtitles_font_size as u32));

        // Background session event pump thread
        let shared_state_clone = Arc::clone(&shared_session_state);
        let overlay_mgr_clone = Arc::clone(&overlay_manager);
        let overlay_enabled_clone = Arc::clone(&overlay_enabled_atomic);
        let overlay_max_count_clone = Arc::clone(&overlay_max_count_atomic);
        let overlay_font_size_clone = Arc::clone(&overlay_font_size_atomic);
        let rx = event_rx.clone();

        std::thread::Builder::new()
            .name("session-event-pump".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    let mut state = shared_state_clone.lock().unwrap();
                    match event {
                        SessionEvent::Connected => {
                            state.connection_status = "Connected - listening".into()
                        }
                        SessionEvent::Disconnected(reason) => {
                            state.connection_status = reason;
                            state.is_translating = false;
                        }
                        SessionEvent::Status(status) => state.connection_status = status,
                        SessionEvent::Asr {
                            kind,
                            text,
                            turn_id,
                        } => {
                            if kind == "final" && !text.is_empty() {
                                state.pending_final_asr.push(PendingFinalAsr {
                                    text: text.clone(),
                                    turn_id: turn_id.clone(),
                                });
                                if state.pending_final_asr.len() > 100 {
                                    state.pending_final_asr.remove(0);
                                }
                                let is_duplicate =
                                    state.recognition_history.last().is_some_and(|entry| {
                                        entry.text == text
                                            && entry.turn_id == turn_id
                                            && entry.speaker_id.is_empty()
                                    });
                                if !is_duplicate {
                                    state.recognition_history.push(RecognitionHistoryEntry {
                                        text: text.clone(),
                                        turn_id,
                                        speaker_id: String::new(),
                                        source_start_ms: 0.0,
                                        source_end_ms: 0.0,
                                        activation_matches: Vec::new(),
                                        context_matches: Vec::new(),
                                    });
                                    if state.recognition_history.len() > 100 {
                                        state.recognition_history.remove(0);
                                    }
                                }
                            }
                            state.partial_text = if kind == "partial" || kind == "blank" {
                                text
                            } else {
                                String::new()
                            };
                        }
                        SessionEvent::SourceSegment {
                            text,
                            activation_matches,
                            context_matches,
                            turn_id,
                            speaker_id,
                            source_start_ms,
                            source_end_ms,
                            segment_index,
                        } => {
                            if text.is_empty() {
                                continue;
                            }
                            if segment_index == 1 {
                                let pending_index = state
                                    .pending_final_asr
                                    .iter()
                                    .position(|pending| {
                                        (!turn_id.is_empty() && pending.turn_id == turn_id)
                                            || (turn_id.is_empty() && pending.turn_id.is_empty())
                                    })
                                    .or_else(|| {
                                        state
                                            .pending_final_asr
                                            .iter()
                                            .position(|pending| pending.turn_id.is_empty())
                                    });
                                if let Some(pending_index) = pending_index {
                                    let pending = state.pending_final_asr.remove(pending_index);
                                    let temporary_index =
                                        state.recognition_history.iter().rposition(|entry| {
                                            entry.speaker_id.is_empty()
                                                && if pending.turn_id.is_empty() {
                                                    entry.turn_id.is_empty()
                                                        && entry.text == pending.text
                                                } else {
                                                    entry.turn_id == pending.turn_id
                                                }
                                        });
                                    if let Some(temporary_index) = temporary_index {
                                        state.recognition_history.remove(temporary_index);
                                    }
                                }
                            }
                            let entry = RecognitionHistoryEntry {
                                text,
                                turn_id,
                                speaker_id,
                                source_start_ms,
                                source_end_ms,
                                activation_matches,
                                context_matches,
                            };
                            if state.recognition_history.last() != Some(&entry) {
                                state.recognition_history.push(entry);
                                if state.recognition_history.len() > 100 {
                                    state.recognition_history.remove(0);
                                }
                            }
                        }
                        SessionEvent::Translation {
                            source,
                            translated,
                            speaker_id,
                            source_start_ms,
                            source_end_ms,
                            term_matches,
                        } => {
                            state.translations.push(TranslationHistoryEntry {
                                source,
                                translated,
                                speaker_id,
                                source_start_ms,
                                source_end_ms,
                                term_matches,
                            });
                            if state.translations.len() > 100 {
                                state.translations.remove(0);
                            }
                        }
                        SessionEvent::TtsAudio(_audio) => {}
                        SessionEvent::BackendError(error) => {
                            state.last_error = Some(error);
                        }
                        SessionEvent::Error(error) => {
                            state.last_error = Some(error);
                            state.connection_status = "Connection error".into();
                            state.is_translating = false;
                        }
                    }

                    // Send state to overlay process immediately (unblocked by main window minimization)
                    if overlay_enabled_clone.load(Ordering::Relaxed) {
                        let max_items = overlay_max_count_clone.load(Ordering::Relaxed);
                        let font_size = overlay_font_size_clone.load(Ordering::Relaxed);
                        let total = state.translations.len();
                        let start = total.saturating_sub(max_items);
                        let visible = state.translations[start..]
                            .iter()
                            .map(|t| (t.source.clone(), t.translated.clone()))
                            .collect();

                        let overlay_state = overlay_ipc::OverlayState {
                            font_size,
                            max_items,
                            visible_entries: visible,
                            partial_text: if state.partial_text.is_empty() {
                                None
                            } else {
                                Some(state.partial_text.clone())
                            },
                        };

                        if let Ok(mut mgr) = overlay_mgr_clone.lock() {
                            mgr.send_state(&overlay_state);
                        }
                    }
                }
            })
            .expect("failed to spawn session-event-pump thread");

        Self {
            audio_system,
            devices,
            selected_device_id: settings.selected_device_id,
            loopback_devices,
            selected_loopback_device_id: settings.selected_loopback_device_id,
            capture_source: settings.capture_source,
            selected_input_config,
            is_translating: false,
            audio_tx: None,
            input_level: Arc::new(AtomicU32::new(0.0_f32.to_bits())),
            session: None,
            event_tx,
            connection_status: "Ready".into(),
            partial_text: String::new(),
            recognition_history: Vec::new(),
            translations: Vec::new(),
            last_error: None,
            server_url: settings.server_url,
            source_lang: settings.source_lang,
            target_lang: settings.target_lang,
            tts_enabled: settings.tts_enabled,
            speaker_recognition_enabled: settings.speaker_recognition_enabled,
            osc_manager,
            osc_draft,
            service_config: service_config::ServiceConfigEditor::load(),
            backend_manager,
            model_task_manager: model_install::NativeModelTaskManager::default(),
            runtime_installer: runtime_install::RuntimeInstaller::default(),
            app_update_manager: app_update::AppUpdateManager::default(),
            backend_start_deadline: None,
            settings_section: ui::pages::settings::SettingsSection::default(),
            modal_dialog: ui::modal::ModalDialog::default(),
            first_run: settings.first_run,
            onboarding_page: 0,
            ui_language: settings.ui_language,
            navigation: NavigationState {
                collapsed: settings.sidebar_collapsed,
                page: settings.active_page,
            },
            mute_self_pauses_translation: Arc::new(AtomicBool::new(
                settings.mute_self_pauses_translation,
            )),
            floating_subtitles_enabled: settings.floating_subtitles_enabled,
            floating_subtitles_max_count: settings.floating_subtitles_max_count,
            floating_subtitles_font_size: settings.floating_subtitles_font_size,
            overlay_manager,
            shared_session_state,
            overlay_enabled_atomic,
            overlay_max_count_atomic,
            overlay_font_size_atomic,
        }
    }
}

impl XRTranslateApp {
    pub fn project_root(&self) -> std::path::PathBuf {
        self.backend_manager.project_root()
    }

    pub fn save_settings(&self) {
        let settings = ClientSettings {
            capture_source: self.capture_source,
            selected_device_id: self.selected_device_id.clone(),
            selected_loopback_device_id: self.selected_loopback_device_id.clone(),
            source_lang: self.source_lang.clone(),
            target_lang: self.target_lang.clone(),
            tts_enabled: self.tts_enabled,
            speaker_recognition_enabled: self.speaker_recognition_enabled,
            mute_self_pauses_translation: self.mute_self_pauses_translation.load(Ordering::Relaxed),
            ui_language: self.ui_language,
            first_run: self.first_run,
            server_url: self.server_url.clone(),
            osc_settings: self.osc_draft.clone(),
            active_page: self.navigation.page,
            sidebar_collapsed: self.navigation.collapsed,
            floating_subtitles_enabled: self.floating_subtitles_enabled,
            floating_subtitles_max_count: self.floating_subtitles_max_count,
            floating_subtitles_font_size: self.floating_subtitles_font_size,
        };
        if let Err(e) = settings.save(&self.project_root()) {
            log::error!("Failed to save client settings: {e}");
        }
    }

    pub fn finish_onboarding(&mut self) {
        self.first_run = false;
        self.save_settings();
    }

    pub fn set_ui_language(&mut self, language: UiLanguage) {
        self.ui_language = language;
        self.save_settings();
    }

    pub fn app_update_state(&self) -> &app_update::AppUpdateState {
        self.app_update_manager.state()
    }

    pub fn check_for_updates(&mut self) {
        if let Err(error) = self.app_update_manager.check() {
            self.last_error = Some(error);
        }
    }

    pub fn download_update(&mut self) {
        if let Err(error) = self.app_update_manager.download(self.project_root()) {
            self.last_error = Some(error);
        }
    }

    pub fn install_update_and_restart(&mut self) {
        let install = match self.app_update_manager.begin_install() {
            Ok(install) => install,
            Err(error) => {
                self.last_error = Some(error);
                return;
            }
        };
        self.stop();
        self.backend_start_deadline = None;
        self.backend_manager.shutdown();
        if let Ok(mut overlay) = self.overlay_manager.lock() {
            overlay.stop();
        }
        match app_update::spawn_updater(install) {
            Ok(()) => std::process::exit(0),
            Err(error) => self.last_error = Some(error),
        }
    }

    fn set_connection_status(&mut self, status: impl Into<String>) {
        let status = status.into();
        self.connection_status.clone_from(&status);
        if let Ok(mut state) = self.shared_session_state.lock() {
            state.connection_status = status;
        }
    }

    fn set_startup_error(&mut self, status: &str, error: String) {
        self.set_connection_status(status);
        self.last_error = Some(error.clone());
        if let Ok(mut state) = self.shared_session_state.lock() {
            state.last_error = Some(error);
            state.is_translating = false;
        }
    }

    pub fn start(&mut self, ctx: Option<eframe::egui::Context>) {
        if self.backend_start_deadline.is_some() {
            return;
        }
        match self.backend_manager.prepare(&self.server_url) {
            Ok(backend::BackendStart::Ready) => self.start_session(ctx),
            Ok(backend::BackendStart::Starting(stage)) => {
                self.backend_start_deadline =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(180));
                self.set_connection_status(stage.message());
                self.last_error = None;
                if let Ok(mut state) = self.shared_session_state.lock() {
                    state.last_error = None;
                }
            }
            Err(error) => self.set_startup_error("Startup failed", error),
        }
    }

    fn start_session(&mut self, ctx: Option<eframe::egui::Context>) {
        let (audio_tx, audio_rx) = unbounded();
        match self.start_selected_capture(audio_tx.clone()) {
            Ok(()) => {
                let tts_handle = crate::feature_access::is_available(
                    crate::feature_access::Feature::TtsPlayback,
                )
                .then(|| self.audio_system.tts_handle())
                .flatten();
                let session = start_session(
                    audio_rx,
                    self.event_tx.clone(),
                    self.server_url.clone(),
                    self.source_lang.clone(),
                    self.target_lang.clone(),
                    self.speaker_recognition_enabled,
                    self.osc_manager.muted_state(),
                    Arc::clone(&self.mute_self_pauses_translation),
                    self.osc_manager.handle(),
                    tts_handle,
                    ctx,
                );
                if crate::feature_access::is_available(crate::feature_access::Feature::TtsPlayback)
                {
                    session.set_tts_enabled(self.tts_enabled);
                }
                self.session = Some(session);
                self.audio_tx = Some(audio_tx);
                self.is_translating = true;
                self.connection_status = "Connecting...".into();
                self.last_error = None;
                self.partial_text.clear();
                self.recognition_history.clear();
                self.translations.clear();

                if let Ok(mut state) = self.shared_session_state.lock() {
                    state.connection_status = "Connecting...".into();
                    state.partial_text.clear();
                    state.pending_final_asr.clear();
                    state.recognition_history.clear();
                    state.translations.clear();
                    state.last_error = None;
                    state.is_translating = true;
                }
            }
            Err(error) => self.set_startup_error("Audio input failed", error),
        }
    }

    fn poll_backend_startup(&mut self, ctx: Option<eframe::egui::Context>) {
        let Some(deadline) = self.backend_start_deadline else {
            return;
        };
        match self.backend_manager.status(&self.server_url) {
            backend::BackendStatus::Ready => {
                self.backend_start_deadline = None;
                self.start_session(ctx);
            }
            backend::BackendStatus::Starting(stage) if std::time::Instant::now() < deadline => {
                self.set_connection_status(stage.message());
            }
            backend::BackendStatus::Starting(_) => {
                self.backend_start_deadline = None;
                self.backend_manager.shutdown();
                self.set_startup_error(
                    "Startup timed out",
                    "Local services did not become ready within 180 seconds".into(),
                );
            }
            backend::BackendStatus::Failed(error) => {
                self.backend_start_deadline = None;
                self.set_startup_error("Startup failed", error.clone());
                self.modal_dialog = ui::modal::ModalDialog::error(
                    "Backend Startup Failure",
                    "The native backend process failed to initialize or exited prematurely.",
                    Some(&error),
                );
            }
        }
    }

    fn refresh_selected_input_config(&mut self) {
        let result = match self.capture_source {
            CaptureSource::Microphone => self.audio_system.input_config(&self.selected_device_id),
            CaptureSource::SystemAudio => self
                .audio_system
                .loopback_config(&self.selected_loopback_device_id),
        };
        match result {
            Ok(config) => {
                self.selected_input_config = Some(config);
                self.last_error = None;
            }
            Err(error) => {
                self.selected_input_config = None;
                self.last_error = Some(error);
            }
        }
    }

    fn start_selected_capture(&mut self, audio_tx: Sender<Vec<f32>>) -> Result<(), String> {
        self.input_level.store(0.0_f32.to_bits(), Ordering::Relaxed);
        match self.capture_source {
            CaptureSource::Microphone => self.audio_system.start_capture(
                &self.selected_device_id,
                audio_tx,
                Arc::clone(&self.input_level),
            ),
            CaptureSource::SystemAudio => self.audio_system.start_loopback_capture(
                &self.selected_loopback_device_id,
                audio_tx,
                Arc::clone(&self.input_level),
            ),
        }
    }

    fn switch_capture_device(&mut self, previous_device_id: String) {
        self.refresh_selected_input_config();
        self.save_settings();
        if !self.is_translating {
            return;
        }

        let Some(audio_tx) = self.audio_tx.clone() else {
            self.last_error = Some("Active audio channel is unavailable".into());
            return;
        };
        match self.start_selected_capture(audio_tx.clone()) {
            Ok(()) => {
                self.connection_status = "Connected - microphone switched".into();
                self.last_error = None;
            }
            Err(error) => {
                match self.capture_source {
                    CaptureSource::Microphone => self.selected_device_id = previous_device_id,
                    CaptureSource::SystemAudio => {
                        self.selected_loopback_device_id = previous_device_id
                    }
                }
                self.refresh_selected_input_config();
                self.save_settings();
                let rollback_error = self.start_selected_capture(audio_tx).err();
                self.last_error = Some(match rollback_error {
                    Some(rollback_error) => format!(
                        "Could not switch audio device: {error}; could not restore previous device: {rollback_error}"
                    ),
                    None => {
                        format!("Could not switch audio device: {error}; previous device restored")
                    }
                });
            }
        }
    }

    fn switch_capture_source(&mut self, previous_source: CaptureSource) {
        self.refresh_selected_input_config();
        self.save_settings();
        if !self.is_translating {
            return;
        }
        let Some(audio_tx) = self.audio_tx.clone() else {
            self.last_error = Some("Active audio channel is unavailable".into());
            return;
        };
        if let Err(error) = self.start_selected_capture(audio_tx) {
            self.capture_source = previous_source;
            self.refresh_selected_input_config();
            self.save_settings();
            self.last_error = Some(format!("Could not switch audio source: {error}"));
        } else {
            self.connection_status = "Connected - audio source switched".into();
            self.last_error = None;
            if let Some(session) = &self.session {
                session.reset_audio_pipeline(self.source_lang.clone(), self.target_lang.clone());
            }
        }
    }

    fn apply_language_route(&mut self) {
        if self.source_lang == "auto" && !self.target_lang.contains(',') {
            self.target_lang = "zh,en".into();
        } else if self.source_lang != "auto" && self.target_lang.contains(',') {
            self.target_lang = "en".into();
        }
        self.save_settings();
        if let Some(session) = &self.session {
            session.update_language_route(self.source_lang.clone(), self.target_lang.clone());
        }
    }

    fn set_tts_enabled(&mut self, enabled: bool) {
        self.tts_enabled = enabled
            && crate::feature_access::is_available(crate::feature_access::Feature::TtsPlayback);
        self.save_settings();
        if !self.tts_enabled {
            self.audio_system.clear_tts_playback();
        }
        if let Some(session) = &self.session {
            session.set_tts_enabled(self.tts_enabled);
        }
    }

    /// Updates the unified speaker-recognition setting.
    fn set_osc_speaker_number_enabled(&mut self, enabled: bool) {
        let enabled = enabled
            && crate::feature_access::is_available(crate::feature_access::Feature::SpeakerNumbers);
        self.speaker_recognition_enabled = enabled;
        self.osc_draft.show_speaker_number = enabled;
        if let Some(session) = &self.session {
            session.set_speaker_recognition_enabled(enabled);
        }
        match self.osc_manager.update_settings(self.osc_draft.clone()) {
            Ok(()) => self.last_error = None,
            Err(error) => self.last_error = Some(error),
        }
        self.save_settings();
    }

    fn set_mute_self_pauses_translation(&mut self, enabled: bool) {
        let enabled = enabled
            && crate::feature_access::is_available(crate::feature_access::Feature::MuteSync);
        self.mute_self_pauses_translation
            .store(enabled, Ordering::Release);
        self.save_settings();
    }

    fn set_floating_subtitles_enabled(&mut self, enabled: bool) {
        self.floating_subtitles_enabled = enabled
            && crate::feature_access::is_available(
                crate::feature_access::Feature::FloatingSubtitles,
            );
        self.save_settings();
    }

    pub(crate) fn clear_history(&mut self) {
        self.translations.clear();
        self.recognition_history.clear();
        self.partial_text.clear();
        if let Ok(mut state) = self.shared_session_state.lock() {
            state.translations.clear();
            state.recognition_history.clear();
            state.partial_text.clear();
            state.pending_final_asr.clear();
        }
        self.osc_manager.clear_chatbox();
    }

    fn stop(&mut self) {
        if let Some(session) = &self.session {
            session.stop();
        }
        self.session = None;
        self.audio_system.stop();
        self.audio_tx = None;
        self.input_level.store(0.0_f32.to_bits(), Ordering::Relaxed);
        self.osc_manager.clear_chatbox();
        self.is_translating = false;
        self.connection_status = "Stopped".into();

        if let Ok(mut state) = self.shared_session_state.lock() {
            state.connection_status = "Stopped".into();
            state.is_translating = false;
        }
    }

    fn poll_session_events(&mut self) {
        if let Some(error) = self.osc_manager.take_error() {
            self.last_error = Some(error);
        }

        // Sync atomic settings to background pump thread
        self.overlay_enabled_atomic
            .store(self.floating_subtitles_enabled, Ordering::Relaxed);
        self.overlay_max_count_atomic
            .store(self.floating_subtitles_max_count, Ordering::Relaxed);
        self.overlay_font_size_atomic
            .store(self.floating_subtitles_font_size as u32, Ordering::Relaxed);

        if self.floating_subtitles_enabled {
            if let Ok(mut mgr) = self.overlay_manager.lock() {
                mgr.start();
                for event in mgr.poll_events() {
                    match event {
                        overlay_ipc::OverlayEvent::CloseRequested => {
                            self.floating_subtitles_enabled = false;
                            self.overlay_enabled_atomic.store(false, Ordering::Relaxed);
                            mgr.stop();
                        }
                        overlay_ipc::OverlayEvent::MaxCountChanged(new_max) => {
                            let clamped = new_max.clamp(1, 10);
                            self.floating_subtitles_max_count = clamped;
                            self.overlay_max_count_atomic
                                .store(clamped, Ordering::Relaxed);
                        }
                    }
                }
            }
        } else {
            if let Ok(mut mgr) = self.overlay_manager.lock() {
                mgr.stop();
            }
        }

        // Copy latest shared state into self for local rendering when main UI is visible
        if let Ok(state) = self.shared_session_state.lock() {
            self.connection_status = state.connection_status.clone();
            self.partial_text = state.partial_text.clone();
            self.recognition_history = state.recognition_history.clone();
            self.translations = state.translations.clone();
            self.is_translating = state.is_translating;
            if let Some(err) = &state.last_error {
                self.last_error = Some(err.clone());
            }
        }
    }
}

fn compact_speaker_label(speaker_id: &str) -> Option<String> {
    let value = speaker_id.trim();
    if value.is_empty() {
        return None;
    }
    let suffix = value.strip_prefix("speaker-").unwrap_or(value);
    if suffix.eq_ignore_ascii_case("unknown") {
        return Some("S?".into());
    }
    let sequence = suffix.trim_start_matches('0');
    Some(format!(
        "S{}",
        if sequence.is_empty() { "0" } else { sequence }
    ))
}

impl eframe::App for XRTranslateApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.model_task_manager.poll();
        self.runtime_installer.poll();
        self.app_update_manager.poll();
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
        if self.first_run {
            ui::render_onboarding_fullscreen(self, ui);
            return;
        }

        self.poll_backend_startup(Some(ui.ctx().clone()));
        self.poll_session_events();

        let expand_target = if self.navigation.collapsed { 0.0 } else { 1.0 };
        let expand_factor = ui::animation::AnimationSystem::animate_value(
            ui.ctx(),
            egui::Id::new("sidebar_expand_anim"),
            expand_target,
            0.20,
        );
        let eased_expand = ui::animation::AnimationSystem::ease_out_cubic(expand_factor);
        let sidebar_width = egui::lerp(54.0..=200.0, eased_expand);
        let margin_x = egui::lerp(8.0..=12.0, eased_expand);

        let prev_collapsed = self.navigation.collapsed;
        let prev_page = self.navigation.page;

        // 1. Native Sidebar Panel (Animated width, full height)
        egui::Panel::left("sidebar_panel")
            .resizable(false)
            .exact_size(sidebar_width)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(226, 232, 240),
                    ))
                    .inner_margin(egui::Margin::symmetric(margin_x.round() as i8, 14)),
            )
            .show(ui, |ui| {
                ui::render_sidebar(
                    ui,
                    &mut self.navigation,
                    &mut self.modal_dialog,
                    self.ui_language,
                    eased_expand,
                );
            });

        if self.navigation.collapsed != prev_collapsed || self.navigation.page != prev_page {
            self.save_settings();
        }

        // 2. Native Central Content Panel (Takes 100% of remaining width and height)
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(248, 250, 252))
                    .inner_margin(egui::Margin::symmetric(24, 20)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("main_scroll_area")
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.navigation.page {
                        Page::Translation => {
                            ui::animation::AnimationSystem::render_animated_page(
                                ui,
                                Page::Translation,
                                |ui| ui::pages::translation::render(self, ui),
                            );
                        }
                        Page::Osc => {
                            ui::animation::AnimationSystem::render_animated_page(
                                ui,
                                Page::Osc,
                                |ui| ui::pages::osc::render(self, ui),
                            );
                        }
                        Page::Settings => {
                            ui::animation::AnimationSystem::render_animated_page(
                                ui,
                                Page::Settings,
                                |ui| ui::pages::settings::render(self, ui),
                            );
                        }
                    });
            });

        self.modal_dialog.render(ui.ctx(), self.ui_language);
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    if std::env::args().any(|a| a == "--overlay") {
        #[cfg(windows)]
        overlay_native::run_native_overlay();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 680.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../resources/branding/xrtranslate-logo.png"
                ))
                .expect("embedded application icon must be valid PNG"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "XRTranslate",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            configure_cjk_fonts(&cc.egui_ctx);
            ui::theme::apply_theme(&cc.egui_ctx);
            Ok(Box::new(XRTranslateApp::default()))
        }),
    )
}

fn configure_cjk_fonts(ctx: &egui::Context) {
    let mut definitions = egui::FontDefinitions::default();

    // 1. Primary CJK Font (Microsoft YaHei)
    let yahei_path = std::path::Path::new(r"C:\Windows\Fonts\msyh.ttc");
    let has_yahei = if let Ok(font_bytes) = std::fs::read(yahei_path) {
        definitions.font_data.insert(
            "microsoft_yahei".into(),
            std::sync::Arc::new(egui::FontData::from_owned(font_bytes)),
        );
        true
    } else {
        log::warn!("Chinese UI font not found: {}", yahei_path.display());
        false
    };

    // 2. Korean Font (Malgun Gothic)
    let malgun_path = std::path::Path::new(r"C:\Windows\Fonts\malgun.ttf");
    let has_malgun = if let Ok(font_bytes) = std::fs::read(malgun_path) {
        definitions.font_data.insert(
            "malgun_gothic".into(),
            std::sync::Arc::new(egui::FontData::from_owned(font_bytes)),
        );
        true
    } else {
        log::warn!("Korean UI font not found: {}", malgun_path.display());
        false
    };

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = definitions.families.entry(family).or_default();
        if has_yahei {
            list.insert(0, "microsoft_yahei".into());
        }
        if has_malgun {
            let pos = if has_yahei { 1 } else { 0 };
            list.insert(pos, "malgun_gothic".into());
        }
    }
    ctx.set_fonts(definitions);
}

#[cfg(test)]
mod tests {
    use super::compact_speaker_label;

    #[test]
    fn compact_speaker_labels_are_stable_and_human_readable() {
        assert_eq!(compact_speaker_label("speaker-01").as_deref(), Some("S1"));
        assert_eq!(compact_speaker_label("speaker-12").as_deref(), Some("S12"));
        assert_eq!(
            compact_speaker_label("speaker-unknown").as_deref(),
            Some("S?")
        );
        assert_eq!(compact_speaker_label(""), None);
    }
}
