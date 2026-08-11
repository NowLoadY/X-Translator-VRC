use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use parking_lot::Mutex;
use rosc::{OscBundle, OscMessage, OscPacket, OscType, decoder, encoder};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MUTE_PATH: &str = "/avatar/parameters/MuteSelf";
const COOLDOWN: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OscFormatMode {
    BilingualSourceFirst, // Source \n Target
    BilingualTargetFirst, // Target \n Source
    Inline,               // Source | Target
    TargetOnly,           // Target only
}

impl OscFormatMode {
    pub fn label(&self, language: crate::i18n::UiLanguage) -> &'static str {
        if matches!(self, Self::BilingualSourceFirst) {
            return crate::i18n::tr(language, "Bilingual (Source → Target)");
        }
        if matches!(self, Self::BilingualTargetFirst) {
            return crate::i18n::tr(language, "Bilingual (Target → Source)");
        }
        match self {
            Self::BilingualSourceFirst => "Bilingual (Source → Target)",
            Self::BilingualTargetFirst => "Bilingual (Target → Source)",
            Self::Inline => crate::i18n::tr(language, "Single Line (Source | Target)"),
            Self::TargetOnly => crate::i18n::tr(language, "Target Only"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BannerContentType {
    None,
    CustomText,
    SystemTime,
    CpuStatus,
    GpuStatus,
}

impl BannerContentType {
    pub fn label(&self, language: crate::i18n::UiLanguage) -> &'static str {
        match self {
            Self::None => crate::i18n::tr(language, "None (Disabled)"),
            Self::CustomText => crate::i18n::tr(language, "Custom Text"),
            Self::SystemTime => crate::i18n::tr(language, "System Time"),
            Self::CpuStatus => crate::i18n::tr(language, "CPU Usage"),
            Self::GpuStatus => crate::i18n::tr(language, "GPU Usage"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannerConfig {
    pub content_type: BannerContentType,
    pub custom_text: String,
    pub show_device_name: bool,
}

impl Default for BannerConfig {
    fn default() -> Self {
        Self {
            content_type: BannerContentType::None,
            custom_text: String::new(),
            show_device_name: false,
        }
    }
}

impl BannerConfig {
    pub fn render_text(&self, metrics: &crate::sys_info::SystemMetrics) -> String {
        match self.content_type {
            BannerContentType::None => String::new(),
            BannerContentType::CustomText => self.custom_text.trim().to_string(),
            BannerContentType::SystemTime => {
                if metrics.time_str.is_empty() {
                    String::new()
                } else {
                    format!("[{}]", metrics.time_str)
                }
            }
            BannerContentType::CpuStatus => {
                if self.show_device_name && !metrics.cpu_name.is_empty() {
                    format!("[{} {}%]", metrics.cpu_name, metrics.cpu_usage)
                } else {
                    format!("[CPU {}%]", metrics.cpu_usage)
                }
            }
            BannerContentType::GpuStatus => {
                if self.show_device_name && !metrics.gpu_name.is_empty() {
                    format!("[{} {}%]", metrics.gpu_name, metrics.gpu_usage)
                } else {
                    format!("[GPU {}%]", metrics.gpu_usage)
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OscSettings {
    pub enabled: bool,
    pub ip: String,
    pub send_port: u16,
    pub listen_port: u16,
    pub max_text_length: usize,
    pub history_ttl_seconds: f64,
    pub header_config: BannerConfig,
    pub footer_config: BannerConfig,
    pub format_mode: OscFormatMode,
    pub show_speaker_number: bool,
}

impl Default for OscSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            ip: "127.0.0.1".into(),
            send_port: 9000,
            listen_port: 9001,
            max_text_length: 144,
            history_ttl_seconds: 15.0,
            header_config: BannerConfig::default(),
            footer_config: BannerConfig::default(),
            format_mode: OscFormatMode::BilingualSourceFirst,
            show_speaker_number: false,
        }
    }
}

#[derive(Default, Deserialize)]
struct ProjectConfig {
    osc: Option<OscConfig>,
    frontend: Option<FrontendConfig>,
}
#[derive(Default, Deserialize)]
struct FrontendConfig {
    show_speaker: Option<bool>,
}
#[derive(Default, Deserialize)]
struct OscConfig {
    enabled: Option<bool>,
    ip: Option<String>,
    send_port: Option<u16>,
    listen_port: Option<u16>,
    max_text_length: Option<usize>,
    history_ttl_seconds: Option<f64>,
}

impl OscSettings {
    /// Retains the established config.json OSC section, but only the native client reads it.
    pub fn from_project_config() -> Self {
        let mut settings = Self::default();
        for path in project_config_candidates() {
            let Ok(contents) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(config) = serde_json::from_str::<ProjectConfig>(&contents) else {
                continue;
            };
            if let Some(value) = config.frontend.and_then(|frontend| frontend.show_speaker) {
                settings.show_speaker_number = value;
            }
            let Some(config) = config.osc else { continue };
            if let Some(value) = config.enabled {
                settings.enabled = value;
            }
            if let Some(value) = config.ip.filter(|v| !v.trim().is_empty()) {
                settings.ip = value;
            }
            if let Some(value) = config.send_port {
                settings.send_port = value;
            }
            if let Some(value) = config.listen_port {
                settings.listen_port = value;
            }
            if let Some(value) = config.max_text_length {
                settings.max_text_length = value;
            }
            if let Some(value) = config.history_ttl_seconds {
                settings.history_ttl_seconds = value;
            }
            break;
        }
        settings
    }

    fn validate(&self) -> Result<(), String> {
        if self.ip.trim().is_empty() {
            return Err("OSC IP address cannot be empty".into());
        }
        if self.max_text_length == 0 {
            return Err("OSC maximum text length must be at least 1".into());
        }
        if !self.history_ttl_seconds.is_finite() || self.history_ttl_seconds < 0.0 {
            return Err("OSC history TTL must be non-negative".into());
        }
        Ok(())
    }
}

fn project_config_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for start in [std::env::current_dir().ok(), std::env::current_exe().ok()] {
        let Some(start) = start else { continue };
        let directory = if start.is_dir() {
            start
        } else {
            start.parent().map(PathBuf::from).unwrap_or(start)
        };
        for ancestor in directory.ancestors() {
            let path = ancestor.join("config.json");
            if path.exists() && !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

#[allow(dead_code)]
pub fn format_chatbox_preview(
    settings: &OscSettings,
    source: &str,
    translated: &str,
    metrics: &crate::sys_info::SystemMetrics,
) -> String {
    format_chatbox_history_preview(settings, &[(source.into(), translated.into())], metrics)
}

pub fn format_chatbox_history_preview(
    settings: &OscSettings,
    history_items: &[(String, String)],
    metrics: &crate::sys_info::SystemMetrics,
) -> String {
    let now = Instant::now();
    let history = history_items
        .iter()
        .map(|(s, t)| HistoryMessage {
            source: s.trim().into(),
            translated: t.trim().into(),
            speaker_id: String::new(),
            expires_at: now + Duration::from_secs_f64(settings.history_ttl_seconds),
        })
        .collect::<Vec<_>>();
    build_chatbox_text(&history, None, settings, metrics)
}

#[derive(Clone)]
struct HistoryMessage {
    source: String,
    translated: String,
    speaker_id: String,
    expires_at: Instant,
}
struct QueuedMessage {
    text: String,
    typing: bool,
    notify: bool,
    final_priority: bool,
    expires_at: Option<Instant>,
}
enum Command {
    Message {
        source: String,
        translated: String,
        speaker_id: String,
        ongoing: bool,
        /// Optional lifetime for this message.
        ttl: Option<Duration>,
    },
    Clear,
    Update(OscSettings),
    Shutdown,
}

#[derive(Clone)]
pub struct OscHandle {
    tx: Sender<Command>,
}

impl OscHandle {
    pub fn add_message_and_send(
        &self,
        source: &str,
        translated: &str,
        speaker_id: &str,
        ongoing: bool,
    ) {
        let _ = self.tx.send(Command::Message {
            source: source.trim().into(),
            translated: translated.trim().into(),
            speaker_id: speaker_id.trim().into(),
            ongoing,
            ttl: None,
        });
    }
    #[allow(dead_code)]
    pub fn add_message_with_ttl(
        &self,
        source: &str,
        translated: &str,
        speaker_id: &str,
        ongoing: bool,
        ttl: Duration,
    ) {
        let _ = self.tx.send(Command::Message {
            source: source.trim().into(),
            translated: translated.trim().into(),
            speaker_id: speaker_id.trim().into(),
            ongoing,
            ttl: Some(ttl),
        });
    }
    pub fn clear_chatbox(&self) {
        let _ = self.tx.send(Command::Clear);
    }
}

#[derive(Default)]
struct RuntimeStatus {
    listener: String,
    last_error: Option<String>,
    chatbox_text: String,
    chatbox_typing: bool,
    next_message_expires_at: Option<Instant>,
}

#[derive(Clone, Debug, Default)]
pub struct ChatboxPreview {
    pub text: String,
    pub typing: bool,
    pub next_message_expires_in: Option<Duration>,
}

/// Owns the VRChat OSC UDP listener and a single coalescing chatbox writer.
/// This keeps VRChat mute-state handling off the ASR path.
pub struct OscManager {
    settings: OscSettings,
    muted: Arc<AtomicBool>,
    status: Arc<Mutex<RuntimeStatus>>,
    tx: Sender<Command>,
    worker: Option<JoinHandle<()>>,
    listener_stop: Option<Sender<()>>,
    listener: Option<JoinHandle<()>>,
}

impl OscManager {
    pub fn new(settings: OscSettings) -> Self {
        let (tx, rx) = unbounded();
        let status = Arc::new(Mutex::new(RuntimeStatus::default()));
        let dispatch_settings = settings.clone();
        let dispatch_status = Arc::clone(&status);
        let worker = thread::Builder::new()
            .name("osc-chatbox-dispatch".into())
            .spawn(move || dispatch_loop(rx, dispatch_settings, dispatch_status))
            .expect("failed to start OSC dispatcher");
        let mut manager = Self {
            settings,
            muted: Arc::new(AtomicBool::new(false)),
            status,
            tx,
            worker: Some(worker),
            listener_stop: None,
            listener: None,
        };
        if let Err(error) = manager.restart_listener() {
            manager.status.lock().last_error = Some(error);
        }
        manager
    }

    pub fn muted_state(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.muted)
    }
    #[allow(dead_code)]
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }
    pub fn listener_status(&self) -> String {
        self.status.lock().listener.clone()
    }
    pub fn take_error(&self) -> Option<String> {
        self.status.lock().last_error.take()
    }
    pub fn chatbox_preview(&self) -> ChatboxPreview {
        let status = self.status.lock();
        ChatboxPreview {
            text: status.chatbox_text.clone(),
            typing: status.chatbox_typing,
            next_message_expires_in: status
                .next_message_expires_at
                .map(|expires_at| expires_at.saturating_duration_since(Instant::now())),
        }
    }
    pub fn clear_chatbox(&self) {
        let _ = self.tx.send(Command::Clear);
    }

    pub fn handle(&self) -> OscHandle {
        OscHandle {
            tx: self.tx.clone(),
        }
    }
    pub fn update_settings(&mut self, settings: OscSettings) -> Result<(), String> {
        settings.validate()?;
        let listener_changed = listener_config_changed(&self.settings, &settings);
        self.settings = settings.clone();
        let _ = self.tx.send(Command::Update(settings));
        if listener_changed {
            self.restart_listener()
        } else {
            Ok(())
        }
    }
    pub fn shutdown(&mut self) {
        self.clear_chatbox();
        self.stop_listener();
        let _ = self.tx.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
    fn restart_listener(&mut self) -> Result<(), String> {
        self.stop_listener();
        self.muted.store(false, Ordering::Release);
        if !self.settings.enabled {
            self.status.lock().listener = "Disabled".into();
            return Ok(());
        }
        let address = format!("{}:{}", self.settings.ip, self.settings.listen_port);
        let socket = UdpSocket::bind(&address)
            .map_err(|e| format!("Cannot listen for VRChat OSC on {address}: {e}"))?;
        socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|e| format!("Cannot configure VRChat OSC listener: {e}"))?;
        let (stop_tx, stop_rx) = bounded(1);
        let muted = Arc::clone(&self.muted);
        let status = Arc::clone(&self.status);
        let listener = thread::Builder::new()
            .name("osc-vrchat-listener".into())
            .spawn(move || listen_loop(socket, stop_rx, muted, status))
            .map_err(|e| format!("Cannot start VRChat OSC listener: {e}"))?;
        self.listener_stop = Some(stop_tx);
        self.listener = Some(listener);
        self.status.lock().listener = format!("Listening on {address}");
        Ok(())
    }
    fn stop_listener(&mut self) {
        if let Some(tx) = self.listener_stop.take() {
            let _ = tx.send(());
        }
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

impl Drop for OscManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn listen_loop(
    socket: UdpSocket,
    stop: Receiver<()>,
    muted: Arc<AtomicBool>,
    status: Arc<Mutex<RuntimeStatus>>,
) {
    let mut buffer = [0; 65_535];
    loop {
        if stop.try_recv().is_ok() {
            return;
        }
        match socket.recv_from(&mut buffer) {
            Ok((size, _)) => match decoder::decode_udp(&buffer[..size]) {
                Ok((_, packet)) => apply_mute_packet(packet, &muted),
                Err(error) => {
                    status.lock().last_error = Some(format!("Invalid OSC packet: {error}"))
                }
            },
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => {
                status.lock().last_error = Some(format!("VRChat OSC listener failed: {error}"));
                return;
            }
        }
    }
}

fn apply_mute_packet(packet: OscPacket, muted: &AtomicBool) {
    match packet {
        OscPacket::Message(message) if message.addr == MUTE_PATH => {
            if let Some(value) = message.args.first().and_then(parse_mute_value) {
                muted.store(value, Ordering::Release);
            }
        }
        OscPacket::Bundle(OscBundle { content, .. }) => {
            for packet in content {
                apply_mute_packet(packet, muted);
            }
        }
        _ => {}
    }
}

fn parse_mute_value(value: &OscType) -> Option<bool> {
    match value {
        OscType::Bool(value) => Some(*value),
        OscType::Int(value) => Some(*value != 0),
        OscType::Long(value) => Some(*value != 0),
        OscType::Float(value) => Some(*value != 0.0),
        OscType::Double(value) => Some(*value != 0.0),
        OscType::String(value) => match value.trim().to_lowercase().as_str() {
            "true" | "1" | "on" | "yes" | "mute" | "muted" => Some(true),
            "false" | "0" | "off" | "no" | "unmute" | "unmuted" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn dispatch_loop(
    rx: Receiver<Command>,
    mut settings: OscSettings,
    status: Arc<Mutex<RuntimeStatus>>,
) {
    let monitor = crate::sys_info::SystemMonitor::new();
    let mut history = Vec::new();
    let mut live = None;
    let mut pending = None;
    let mut last_send = Instant::now() - COOLDOWN;
    loop {
        let wait = dispatch_wait(
            pending.as_ref(),
            last_send,
            next_content_expiry(&history, live.as_ref()),
        );
        match rx.recv_timeout(wait) {
            Ok(Command::Message {
                source,
                translated,
                speaker_id,
                ongoing,
                ttl,
            }) if settings.enabled => {
                let now = Instant::now();
                expire_chatbox_entries(&mut history, &mut live, now);
                let entry = HistoryMessage {
                    source,
                    translated,
                    speaker_id,
                    expires_at: now
                        + ttl.unwrap_or_else(|| {
                            Duration::from_secs_f64(settings.history_ttl_seconds)
                        }),
                };
                if render_entry(&entry, &settings).is_empty() {
                    continue;
                }
                if ongoing {
                    live = Some(entry);
                } else {
                    live = None;
                    history.push(entry);
                }
                let metrics = monitor.snapshot();
                queue_message(
                    &mut pending,
                    build_queued_message(
                        &history,
                        live.as_ref(),
                        &settings,
                        &metrics,
                        ongoing,
                        !ongoing,
                        !ongoing,
                    ),
                );
            }
            Ok(Command::Message { .. }) => {}
            Ok(Command::Clear) => {
                history.clear();
                live = None;
                if settings.enabled {
                    queue_message(&mut pending, clear_message());
                } else {
                    clear_runtime_preview(&status);
                }
            }
            Ok(Command::Update(updated)) => {
                if settings.enabled && !updated.enabled {
                    send_message(&settings, &clear_message(), &status);
                    pending = None;
                    last_send = Instant::now();
                }
                settings = updated;
                expire_chatbox_entries(&mut history, &mut live, Instant::now());
                if settings.enabled {
                    let metrics = monitor.snapshot();
                    queue_message(
                        &mut pending,
                        build_queued_message(
                            &history,
                            live.as_ref(),
                            &settings,
                            &metrics,
                            live.is_some(),
                            false,
                            true,
                        ),
                    );
                }
            }
            Ok(Command::Shutdown) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                if let Some(message) = pending.take()
                    && settings.enabled
                {
                    send_message(&settings, &message, &status);
                }
                return;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                if expire_chatbox_entries(&mut history, &mut live, now) {
                    let metrics = monitor.snapshot();
                    queue_message(
                        &mut pending,
                        build_queued_message(
                            &history,
                            live.as_ref(),
                            &settings,
                            &metrics,
                            false,
                            false,
                            true,
                        ),
                    );
                }
            }
        }
        if pending.is_some() && last_send.elapsed() >= COOLDOWN {
            let message = pending.take().expect("pending message checked above");
            send_message(&settings, &message, &status);
            last_send = Instant::now();
        }
    }
}

fn queue_message(pending: &mut Option<QueuedMessage>, message: QueuedMessage) {
    if !message.final_priority
        && pending
            .as_ref()
            .is_some_and(|pending| pending.final_priority)
    {
        return;
    }
    *pending = Some(message);
}

fn clear_message() -> QueuedMessage {
    QueuedMessage {
        text: String::new(),
        typing: false,
        notify: false,
        final_priority: true,
        expires_at: None,
    }
}

fn send_message(settings: &OscSettings, message: &QueuedMessage, status: &Mutex<RuntimeStatus>) {
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(socket) => socket,
        Err(error) => {
            status.lock().last_error = Some(format!("Cannot create OSC sender: {error}"));
            return;
        }
    };
    let target = format!("{}:{}", settings.ip, settings.send_port);
    for packet in [
        OscPacket::Message(OscMessage {
            addr: "/chatbox/typing".into(),
            args: vec![OscType::Bool(message.typing)],
        }),
        OscPacket::Message(OscMessage {
            addr: "/chatbox/input".into(),
            args: vec![
                OscType::String(message.text.clone()),
                OscType::Bool(true),
                OscType::Bool(message.notify),
            ],
        }),
    ] {
        match encoder::encode(&packet) {
            Ok(bytes) => {
                if let Err(error) = socket.send_to(&bytes, &target) {
                    status.lock().last_error = Some(format!("Cannot send VRChat OSC: {error}"));
                }
            }
            Err(error) => {
                status.lock().last_error = Some(format!("Cannot encode VRChat OSC: {error}"));
            }
        }
    }
    let mut runtime = status.lock();
    runtime.chatbox_text.clone_from(&message.text);
    runtime.chatbox_typing = message.typing;
    runtime.next_message_expires_at = message.expires_at;
}

fn build_queued_message(
    history: &[HistoryMessage],
    live: Option<&HistoryMessage>,
    settings: &OscSettings,
    metrics: &crate::sys_info::SystemMetrics,
    typing: bool,
    notify: bool,
    final_priority: bool,
) -> QueuedMessage {
    QueuedMessage {
        text: build_chatbox_text(history, live, settings, metrics),
        typing,
        notify,
        final_priority,
        expires_at: next_content_expiry(history, live),
    }
}

fn dispatch_wait(
    pending: Option<&QueuedMessage>,
    last_send: Instant,
    content_expiry: Option<Instant>,
) -> Duration {
    let now = Instant::now();
    let send_wait = pending.map(|_| COOLDOWN.saturating_sub(now.duration_since(last_send)));
    let expiry_wait = content_expiry.map(|expires_at| expires_at.saturating_duration_since(now));
    match (send_wait, expiry_wait) {
        (Some(send), Some(expiry)) => send.min(expiry),
        (Some(send), None) => send,
        (None, Some(expiry)) => expiry,
        (None, None) => Duration::from_secs(3600),
    }
}

fn next_content_expiry(
    history: &[HistoryMessage],
    live: Option<&HistoryMessage>,
) -> Option<Instant> {
    history
        .iter()
        .chain(live)
        .map(|entry| entry.expires_at)
        .min()
}

fn listener_config_changed(previous: &OscSettings, updated: &OscSettings) -> bool {
    previous.enabled != updated.enabled
        || previous.ip != updated.ip
        || previous.listen_port != updated.listen_port
}

fn expire_chatbox_entries(
    history: &mut Vec<HistoryMessage>,
    live: &mut Option<HistoryMessage>,
    now: Instant,
) -> bool {
    let previous_len = history.len();
    history.retain(|entry| now < entry.expires_at);
    let history_changed = history.len() != previous_len;
    let live_expired = live.as_ref().is_some_and(|entry| now >= entry.expires_at);
    if live_expired {
        *live = None;
    }
    history_changed || live_expired
}

fn clear_runtime_preview(status: &Mutex<RuntimeStatus>) {
    let mut runtime = status.lock();
    runtime.chatbox_text.clear();
    runtime.chatbox_typing = false;
    runtime.next_message_expires_at = None;
}

fn build_chatbox_text(
    history: &[HistoryMessage],
    live: Option<&HistoryMessage>,
    settings: &OscSettings,
    metrics: &crate::sys_info::SystemMetrics,
) -> String {
    let mut entries = history.iter().chain(live).cloned().collect::<VecDeque<_>>();
    while entries.len() > 9 {
        entries.pop_front();
    }

    // Header and footer exist only while live messages remain.
    if entries.is_empty() {
        return String::new();
    }

    let prefix = settings.header_config.render_text(metrics);
    let suffix = settings.footer_config.render_text(metrics);

    while let Some(first) = entries.front() {
        let rendered_entries = entries
            .iter()
            .map(|e| render_entry(e, settings))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        let combined = compose_chatbox(&prefix, &rendered_entries.join("\n"), &suffix);

        if combined.chars().count() <= settings.max_text_length {
            return combined;
        }
        if entries.len() > 1 {
            entries.pop_front();
        } else {
            return fit_single_entry(first, &prefix, &suffix, settings);
        }
    }

    String::new()
}

fn compose_chatbox(prefix: &str, content: &str, suffix: &str) -> String {
    [prefix, content, suffix]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_entry(entry: &HistoryMessage, settings: &OscSettings) -> String {
    let source = entry.source.trim();
    let translated = entry.translated.trim();

    let core_text = match settings.format_mode {
        OscFormatMode::TargetOnly => {
            if !translated.is_empty() {
                translated.to_string()
            } else {
                source.to_string()
            }
        }
        OscFormatMode::BilingualTargetFirst => {
            if !source.is_empty() && !translated.is_empty() && source != translated {
                format!("{}\n{}", translated, source)
            } else if !translated.is_empty() {
                translated.to_string()
            } else {
                source.to_string()
            }
        }
        OscFormatMode::Inline => {
            if !source.is_empty() && !translated.is_empty() && source != translated {
                format!("{} | {}", source, translated)
            } else if !translated.is_empty() {
                translated.to_string()
            } else {
                source.to_string()
            }
        }
        OscFormatMode::BilingualSourceFirst => {
            if !source.is_empty() && !translated.is_empty() && source != translated {
                format!("{}\n{}", source, translated)
            } else if !translated.is_empty() {
                translated.to_string()
            } else {
                source.to_string()
            }
        }
    };

    if core_text.is_empty() {
        return String::new();
    }

    if settings.show_speaker_number
        && let Some(label) = crate::compact_speaker_label(&entry.speaker_id)
    {
        format!("[{label}] {core_text}")
    } else {
        core_text
    }
}

fn fit_single_entry(
    entry: &HistoryMessage,
    prefix: &str,
    suffix: &str,
    settings: &OscSettings,
) -> String {
    let rendered = render_entry(entry, settings);
    let limit = settings.max_text_length;
    let mut prefix = prefix;
    let mut suffix = suffix;

    // Preserve speech before decorations when space is limited.
    while decoration_length(prefix, suffix) >= limit {
        if !suffix.is_empty() {
            suffix = "";
        } else if !prefix.is_empty() {
            prefix = "";
        } else {
            break;
        }
    }
    let content_limit = limit.saturating_sub(decoration_length(prefix, suffix));
    let content = trim_text(&rendered, content_limit);
    compose_chatbox(prefix, &content, suffix)
}

fn decoration_length(prefix: &str, suffix: &str) -> usize {
    let text = prefix.chars().count() + suffix.chars().count();
    let separators = usize::from(!prefix.is_empty()) + usize::from(!suffix.is_empty());
    text + separators
}

fn trim_text(text: &str, limit: usize) -> String {
    let value = text.trim();
    if limit == 0 {
        return String::new();
    }
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= limit {
        return value.into();
    }
    let tail = chars[chars.len() - limit..].iter().collect::<String>();
    for marker in [
        "。", "！", "？", ".", "!", "?", ";", ":", "；", "，", ",", " ",
    ] {
        if let Some(index) = tail.find(marker) {
            let next = index + marker.len();
            if next < tail.len() {
                return tail[next..].trim_start().into();
            }
        }
    }
    tail.trim_start().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_only_updates_do_not_restart_the_osc_listener() {
        let previous = OscSettings::default();
        let mut display_update = previous.clone();
        display_update.show_speaker_number = true;
        display_update.history_ttl_seconds = 20.0;
        assert!(!listener_config_changed(&previous, &display_update));

        let mut network_update = previous.clone();
        network_update.listen_port += 1;
        assert!(listener_config_changed(&previous, &network_update));
    }

    fn history_message(expires_at: Instant, text: &str) -> HistoryMessage {
        HistoryMessage {
            source: text.into(),
            translated: String::new(),
            speaker_id: String::new(),
            expires_at,
        }
    }

    fn receive_chatbox_input(socket: &UdpSocket) -> (String, bool) {
        let mut buffer = [0; 4096];
        loop {
            let (size, _) = socket.recv_from(&mut buffer).unwrap();
            let (_, packet) = decoder::decode_udp(&buffer[..size]).unwrap();
            let OscPacket::Message(message) = packet else {
                continue;
            };
            if message.addr != "/chatbox/input" {
                continue;
            }
            let text = match &message.args[0] {
                OscType::String(text) => text.clone(),
                other => panic!("unexpected chatbox text argument: {other:?}"),
            };
            let notify = match message.args[2] {
                OscType::Bool(notify) => notify,
                ref other => panic!("unexpected chatbox notification argument: {other:?}"),
            };
            return (text, notify);
        }
    }

    #[test]
    fn mute_values_match_the_wire_contract() {
        assert_eq!(
            parse_mute_value(&OscType::String("muted".into())),
            Some(true)
        );
        assert_eq!(parse_mute_value(&OscType::Int(0)), Some(false));
    }

    #[test]
    fn ttl_expiration_removes_final_history_and_abandoned_live_text() {
        let started = Instant::now();
        let expires_at = started + Duration::from_secs(1);
        let mut history = vec![history_message(expires_at, "final")];
        let mut live = Some(history_message(expires_at, "partial"));

        assert!(!expire_chatbox_entries(
            &mut history,
            &mut live,
            started + Duration::from_millis(999),
        ));
        assert!(expire_chatbox_entries(
            &mut history,
            &mut live,
            started + Duration::from_secs(1),
        ));
        assert!(history.is_empty());
        assert!(live.is_none());
    }

    #[test]
    fn next_expiry_tracks_the_oldest_visible_entry() {
        let started = Instant::now();
        let history = vec![
            history_message(started + Duration::from_secs(10), "oldest"),
            history_message(started + Duration::from_secs(12), "newer"),
        ];
        let live = history_message(started + Duration::from_secs(13), "live");

        assert_eq!(
            next_content_expiry(&history, Some(&live)),
            Some(started + Duration::from_secs(10)),
        );
    }

    #[test]
    fn entries_expire_independently_and_banners_clear_with_the_last_message() {
        let started = Instant::now();
        let mut history = vec![
            history_message(started + Duration::from_secs(1), "old"),
            history_message(started + Duration::from_secs(3), "new"),
        ];
        let mut live = None;
        let mut settings = OscSettings::default();
        settings.header_config = BannerConfig {
            content_type: BannerContentType::CustomText,
            custom_text: "header".into(),
            show_device_name: false,
        };
        let metrics = crate::sys_info::SystemMetrics::default();

        assert!(expire_chatbox_entries(
            &mut history,
            &mut live,
            started + Duration::from_secs(1),
        ));
        assert_eq!(history.len(), 1);
        assert!(build_chatbox_text(&history, None, &settings, &metrics).contains("new"));

        assert!(expire_chatbox_entries(
            &mut history,
            &mut live,
            started + Duration::from_secs(3),
        ));
        assert!(build_chatbox_text(&history, None, &settings, &metrics).is_empty());
    }

    #[test]
    fn long_single_message_is_trimmed_without_silently_dropping_banners() {
        let now = Instant::now();
        let history = vec![history_message(now + Duration::from_secs(10), "0123456789")];
        let mut settings = OscSettings::default();
        settings.max_text_length = 12;
        settings.header_config = BannerConfig {
            content_type: BannerContentType::CustomText,
            custom_text: "H".into(),
            show_device_name: false,
        };
        settings.footer_config = BannerConfig {
            content_type: BannerContentType::CustomText,
            custom_text: "F".into(),
            show_device_name: false,
        };

        let text = build_chatbox_text(
            &history,
            None,
            &settings,
            &crate::sys_info::SystemMetrics::default(),
        );
        assert_eq!(text, "H\n23456789\nF");
        assert_eq!(text.chars().count(), settings.max_text_length);
    }

    #[test]
    fn clear_messages_do_not_trigger_a_vrchat_notification() {
        let message = clear_message();

        assert!(!message.typing);
        assert!(!message.notify);
        assert!(message.final_priority);
    }

    #[test]
    fn speaker_number_prefix_uses_the_assigned_voiceprint_id_and_can_be_disabled() {
        let mut entry = history_message(Instant::now(), "hello");
        entry.speaker_id = "speaker-02".into();
        let mut settings = OscSettings::default();

        assert_eq!(render_entry(&entry, &settings), "hello");
        settings.show_speaker_number = true;
        assert_eq!(render_entry(&entry, &settings), "[S2] hello");

        entry.speaker_id = "speaker-unknown".into();
        assert_eq!(render_entry(&entry, &settings), "[S?] hello");
    }

    #[test]
    fn dispatcher_actively_clears_vrchat_after_ttl_without_another_message() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut settings = OscSettings::default();
        settings.ip = "127.0.0.1".into();
        settings.send_port = receiver.local_addr().unwrap().port();
        settings.history_ttl_seconds = 0.05;
        let status = Arc::new(Mutex::new(RuntimeStatus::default()));
        let worker_status = Arc::clone(&status);
        let (tx, rx) = unbounded();
        let worker = thread::spawn(move || dispatch_loop(rx, settings, worker_status));

        tx.send(Command::Message {
            source: "hello".into(),
            translated: "你好".into(),
            speaker_id: "speaker-01".into(),
            ongoing: false,
            ttl: None,
        })
        .unwrap();
        assert_eq!(
            receive_chatbox_input(&receiver),
            ("hello\n你好".into(), true)
        );
        assert_eq!(receive_chatbox_input(&receiver), (String::new(), false));
        assert!(status.lock().chatbox_text.is_empty());

        tx.send(Command::Shutdown).unwrap();
        worker.join().unwrap();
    }
}
