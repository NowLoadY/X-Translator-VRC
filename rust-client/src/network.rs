use crossbeam_channel::{Receiver, Sender};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
};
use xrtranslate_protocol::CorpusTermMatch;

#[derive(Debug, Clone)]
pub enum SessionEvent {
    Connected,
    Disconnected(String),
    Status(String),
    Asr {
        kind: String,
        text: String,
        turn_id: String,
    },
    SourceSegment {
        text: String,
        activation_matches: Vec<CorpusTermMatch>,
        context_matches: Vec<CorpusTermMatch>,
        turn_id: String,
        speaker_id: String,
        source_start_ms: f64,
        source_end_ms: f64,
        segment_index: u32,
    },
    Translation {
        source: String,
        translated: String,
        speaker_id: String,
        source_start_ms: f64,
        source_end_ms: f64,
        term_matches: Vec<CorpusTermMatch>,
    },
    TtsAudio(Vec<u8>),
    BackendError(String),
    Error(String),
}

pub struct SessionHandle {
    stop_requested: Arc<AtomicBool>,
    command_tx: mpsc::Sender<SessionCommand>,
}

enum SessionCommand {
    UpdateLanguageRoute {
        source_lang: String,
        target_lang: String,
    },
    ResetAudioPipeline {
        source_lang: String,
        target_lang: String,
    },
    SetTtsEnabled(bool),
    SetSpeakerRecognitionEnabled(bool),
}

impl SessionHandle {
    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
    }

    pub fn update_language_route(&self, source_lang: String, target_lang: String) {
        let _ = self
            .command_tx
            .try_send(SessionCommand::UpdateLanguageRoute {
                source_lang,
                target_lang,
            });
    }

    pub fn set_tts_enabled(&self, enabled: bool) {
        let _ = self
            .command_tx
            .try_send(SessionCommand::SetTtsEnabled(enabled));
    }

    pub fn set_speaker_recognition_enabled(&self, enabled: bool) {
        let _ = self
            .command_tx
            .try_send(SessionCommand::SetSpeakerRecognitionEnabled(enabled));
    }

    /// Reconfigure the backend audio stream after replacing the local capture
    /// source. This clears any partially accumulated VAD utterance.
    pub fn reset_audio_pipeline(&self, source_lang: String, target_lang: String) {
        let _ = self
            .command_tx
            .try_send(SessionCommand::ResetAudioPipeline {
                source_lang,
                target_lang,
            });
    }
}

pub fn start_session(
    audio_rx: Receiver<Vec<f32>>,
    event_tx: Sender<SessionEvent>,
    server_url: String,
    source_lang: String,
    target_lang: String,
    speaker_recognition_enabled: bool,
    muted: Arc<AtomicBool>,
    mute_gate_enabled: Arc<AtomicBool>,
    osc_manager: crate::osc::OscHandle,
    tts_handle: Option<crate::audio::TtsPlayerHandle>,
    egui_ctx: Option<eframe::egui::Context>,
) -> SessionHandle {
    let stop_requested = Arc::new(AtomicBool::new(false));
    let runtime_stop = Arc::clone(&stop_requested);
    // Bound pending configuration updates.
    let (command_tx, command_rx) = mpsc::channel(16);
    thread::Builder::new()
        .name("translation-session".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = event_tx.send(SessionEvent::Error(format!(
                        "Failed to start network runtime: {error}"
                    )));
                    return;
                }
            };
            runtime.block_on(run_session(
                audio_rx,
                event_tx,
                server_url,
                source_lang,
                target_lang,
                speaker_recognition_enabled,
                runtime_stop,
                muted,
                mute_gate_enabled,
                command_rx,
                osc_manager,
                tts_handle,
                egui_ctx,
            ));
        })
        .expect("failed to start translation session thread");

    SessionHandle {
        stop_requested,
        command_tx,
    }
}

async fn run_session(
    audio_rx: Receiver<Vec<f32>>,
    event_tx: Sender<SessionEvent>,
    server_url: String,
    source_lang: String,
    target_lang: String,
    speaker_recognition_enabled: bool,
    stop_requested: Arc<AtomicBool>,
    muted: Arc<AtomicBool>,
    mute_gate_enabled: Arc<AtomicBool>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
    osc_manager: crate::osc::OscHandle,
    tts_handle: Option<crate::audio::TtsPlayerHandle>,
    egui_ctx: Option<eframe::egui::Context>,
) {
    let _ = event_tx.send(SessionEvent::Status("Connecting to backend…".into()));
    let (stream, _) = match connect_async(&server_url).await {
        Ok(connection) => connection,
        Err(error) => {
            let _ = event_tx.send(SessionEvent::Error(format!(
                "Cannot connect to {server_url}: {error}"
            )));
            return;
        }
    };
    let (mut write, mut read) = stream.split();
    if let Err(error) = send_json(
        &mut write,
        json!({
            "action": "session_config",
            "source_lang": source_lang,
            "target_lang": target_lang,
            "sample_rate": 16_000,
        }),
    )
    .await
    {
        let _ = event_tx.send(SessionEvent::Error(format!(
            "Cannot configure session: {error}"
        )));
        return;
    }
    if let Err(error) = send_json(
        &mut write,
        json!({
            "action": "toggle_feature",
            "feature": "speaker_recognition",
            "enabled": speaker_recognition_enabled,
        }),
    )
    .await
    {
        let _ = event_tx.send(SessionEvent::Error(format!(
            "Cannot configure speaker recognition: {error}"
        )));
        return;
    }
    if let Err(error) = send_json(
        &mut write,
        json!({
            "event": "config_audio",
            "sample_rate": 16_000,
            "source_lang": source_lang,
            "target_lang": target_lang,
        }),
    )
    .await
    {
        let _ = event_tx.send(SessionEvent::Error(format!(
            "Cannot configure audio: {error}"
        )));
        return;
    }
    let _ = event_tx.send(SessionEvent::Connected);

    let (pcm_tx, mut pcm_rx) = mpsc::channel::<Vec<u8>>(32);
    let producer_stop = Arc::clone(&stop_requested);
    thread::spawn(move || {
        while !producer_stop.load(Ordering::Acquire) {
            match audio_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(samples) => {
                    let pcm = f32_to_pcm16le(samples);
                    let _ = pcm_tx.try_send(pcm);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    let mut turn_started = false;
    let mut ticker = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if stop_requested.load(Ordering::Acquire) {
                    let _ = send_json(&mut write, json!({"event": "stop"})).await;
                    let _ = write.close().await;
                    let _ = event_tx.send(SessionEvent::Disconnected("Stopped".into()));
                    break;
                }
            }
            Some(command) = command_rx.recv() => {
                if let Err(error) = send_session_command(&mut write, command).await {
                    let _ = event_tx.send(SessionEvent::Error(format!("Failed to update session: {error}")));
                    return;
                }
            }
            Some(pcm) = pcm_rx.recv() => {
                if mute_gate_enabled.load(Ordering::Acquire) && muted.load(Ordering::Acquire) {
                    continue;
                }
                if !turn_started {
                    turn_started = true;
                    if let Err(error) = send_json(&mut write, json!({"event": "turn_started", "turn_id": "native-1"})).await {
                        let _ = event_tx.send(SessionEvent::Error(format!("Failed to begin audio turn: {error}")));
                        break;
                    }
                }
                if let Err(error) = write.send(Message::Binary(pcm.into())).await {
                    let _ = event_tx.send(SessionEvent::Error(format!("Failed to send microphone audio: {error}")));
                    break;
                }
            }
            message = read.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => forward_server_event(&event_tx, &text, &osc_manager),
                    Some(Ok(Message::Binary(audio))) => {
                        if let Some(tts) = &tts_handle {
                            tts.play_pcm(&audio);
                        }
                        let _ = event_tx.send(SessionEvent::TtsAudio(audio.to_vec()));
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        osc_manager.clear_chatbox();
                        let _ = event_tx.send(SessionEvent::Disconnected("Backend closed the connection".into()));
                        if let Some(ctx) = &egui_ctx { ctx.request_repaint(); }
                        break;
                    }
                    Some(Err(error)) => {
                        osc_manager.clear_chatbox();
                        let _ = event_tx.send(SessionEvent::Error(format!("Backend connection failed: {error}")));
                        if let Some(ctx) = &egui_ctx { ctx.request_repaint(); }
                        break;
                    }
                    _ => {}
                }
                if let Some(ctx) = &egui_ctx {
                    ctx.request_repaint();
                }
            }
        }
    }
}

async fn send_session_command<S>(
    write: &mut S,
    command: SessionCommand,
) -> Result<(), tungstenite::Error>
where
    S: futures::Sink<Message, Error = tungstenite::Error> + Unpin,
{
    match command {
        SessionCommand::UpdateLanguageRoute {
            source_lang,
            target_lang,
        } => {
            send_json(
                write,
                json!({
                    "action": "session_config",
                    "source_lang": source_lang,
                    "target_lang": target_lang,
                }),
            )
            .await
        }
        SessionCommand::SetTtsEnabled(enabled) => {
            send_json(
                write,
                json!({
                    "action": "toggle_feature",
                    "feature": "tts",
                    "enabled": enabled,
                }),
            )
            .await
        }
        SessionCommand::SetSpeakerRecognitionEnabled(enabled) => {
            send_json(
                write,
                json!({
                    "action": "toggle_feature",
                    "feature": "speaker_recognition",
                    "enabled": enabled,
                }),
            )
            .await
        }
        SessionCommand::ResetAudioPipeline {
            source_lang,
            target_lang,
        } => {
            log::info!("Resetting backend audio pipeline after capture-source switch");
            send_json(
                write,
                json!({
                    "event": "config_audio",
                    "sample_rate": 16_000,
                    "source_lang": source_lang,
                    "target_lang": target_lang,
                }),
            )
            .await
        }
    }
}

async fn send_json<S>(write: &mut S, value: Value) -> Result<(), tungstenite::Error>
where
    S: futures::Sink<Message, Error = tungstenite::Error> + Unpin,
{
    write.send(Message::Text(value.to_string().into())).await
}

fn forward_server_event(
    event_tx: &Sender<SessionEvent>,
    text: &str,
    osc_manager: &crate::osc::OscHandle,
) {
    let Ok(payload) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let data = payload.get("data").and_then(Value::as_object);
    match payload.get("action").and_then(Value::as_str) {
        Some("session_ready") => {
            let _ = event_tx.send(SessionEvent::Status("Connected — listening".into()));
        }
        Some("asr_result") => {
            let kind: String = data
                .and_then(|d| d.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("partial")
                .into();
            let text_val: String = data
                .and_then(|d| d.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into();

            if kind == "partial" && !text_val.is_empty() {
                osc_manager.add_message_and_send(&text_val, "", "", true);
            }

            let _ = event_tx.send(SessionEvent::Asr {
                kind,
                text: text_val,
                turn_id: data
                    .and_then(|d| d.get("turn_id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            });
        }
        Some("source_segment_ready") => {
            let _ = event_tx.send(SessionEvent::SourceSegment {
                text: data
                    .and_then(|d| d.get("source_text"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                activation_matches: data
                    .and_then(|d| d.get("activation_matches"))
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default(),
                context_matches: data
                    .and_then(|d| d.get("context_matches"))
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default(),
                turn_id: data
                    .and_then(|d| d.get("turn_id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                speaker_id: data
                    .and_then(|d| d.get("speaker_id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                source_start_ms: data
                    .and_then(|d| d.get("source_start_ms"))
                    .and_then(Value::as_f64)
                    .unwrap_or_default(),
                source_end_ms: data
                    .and_then(|d| d.get("source_end_ms"))
                    .and_then(Value::as_f64)
                    .unwrap_or_default(),
                segment_index: data
                    .and_then(|d| d.get("segment_index"))
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or(1),
            });
        }
        Some("translation_ready") => {
            let source: String = data
                .and_then(|d| d.get("source_text"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into();
            let translated: String = data
                .and_then(|d| d.get("translated_text"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into();
            let speaker_id: String = data
                .and_then(|d| d.get("speaker_id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into();

            osc_manager.add_message_and_send(&source, &translated, &speaker_id, false);

            let _ = event_tx.send(SessionEvent::Translation {
                source,
                translated,
                speaker_id,
                source_start_ms: data
                    .and_then(|d| d.get("source_start_ms"))
                    .and_then(Value::as_f64)
                    .unwrap_or_default(),
                source_end_ms: data
                    .and_then(|d| d.get("source_end_ms"))
                    .and_then(Value::as_f64)
                    .unwrap_or_default(),
                term_matches: data
                    .and_then(|d| d.get("term_matches"))
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default(),
            });
        }
        Some("error") => {
            let _ = event_tx.send(SessionEvent::BackendError(
                data.and_then(|d| d.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown backend error")
                    .into(),
            ));
        }
        _ => {}
    }
}

fn f32_to_pcm16le(samples: Vec<f32>) -> Vec<u8> {
    samples
        .into_iter()
        .flat_map(|sample| {
            let sample = sample.clamp(-1.0, 1.0);
            let pcm = if sample < 0.0 {
                (sample * 32768.0) as i16
            } else {
                (sample * 32767.0) as i16
            };
            pcm.to_le_bytes()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_segment_event_retains_speaker_and_timeline_metadata() {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let mock_osc = crate::osc::OscManager::new(crate::osc::OscSettings::default()).handle();
        forward_server_event(
            &sender,
            r#"{"action":"source_segment_ready","data":{"source_text":"hello","speaker_id":"speaker-03","source_start_ms":125.0,"source_end_ms":875.0,"segment_index":2}}"#,
            &mock_osc,
        );

        let event = receiver.try_recv().unwrap();
        let SessionEvent::SourceSegment {
            text,
            activation_matches,
            speaker_id,
            source_start_ms,
            source_end_ms,
            segment_index,
            ..
        } = event
        else {
            panic!("expected source-segment event");
        };
        assert_eq!(text, "hello");
        assert!(activation_matches.is_empty());
        assert_eq!(speaker_id, "speaker-03");
        assert_eq!(source_start_ms, 125.0);
        assert_eq!(source_end_ms, 875.0);
        assert_eq!(segment_index, 2);
    }

    #[test]
    fn backend_feature_errors_are_nonfatal_session_events() {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let mock_osc = crate::osc::OscManager::new(crate::osc::OscSettings::default()).handle();
        forward_server_event(
            &sender,
            r#"{"action":"error","data":{"message":"speaker recognition is unavailable"}}"#,
            &mock_osc,
        );

        assert!(matches!(
            receiver.try_recv().unwrap(),
            SessionEvent::BackendError(message) if message == "speaker recognition is unavailable"
        ));
    }

    #[test]
    fn translation_event_retains_term_provenance() {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let mock_osc = crate::osc::OscManager::new(crate::osc::OscSettings::default()).handle();
        forward_server_event(
            &sender,
            r#"{"action":"translation_ready","data":{"source_text":"I love Mercy.","translated_text":"我喜欢天使。","speaker_id":"","term_matches":[{"start_byte":9,"end_byte":15,"text":"天使","sources":[{"corpus_id":"games.overwatch.heroes","domain":"games","subdomain":"overwatch","title":"Overwatch Heroes"}]}]}}"#,
            &mock_osc,
        );

        let SessionEvent::Translation { term_matches, .. } = receiver.try_recv().unwrap() else {
            panic!("expected translation event");
        };
        assert_eq!(term_matches[0].text, "天使");
        assert_eq!(
            term_matches[0].sources[0].corpus_id,
            "games.overwatch.heroes"
        );
    }

    #[test]
    fn source_segment_event_retains_activation_provenance() {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let mock_osc = crate::osc::OscManager::new(crate::osc::OscSettings::default()).handle();
        forward_server_event(
            &sender,
            r#"{"action":"source_segment_ready","data":{"source_text":"论文写没？","activation_matches":[{"start_byte":0,"end_byte":6,"text":"论文","sources":[{"corpus_id":"education-and-science.research.common","domain":"education-and-science","subdomain":"research","title":"研究与学术交流"}]}]}}"#,
            &mock_osc,
        );

        let SessionEvent::SourceSegment {
            activation_matches, ..
        } = receiver.try_recv().unwrap()
        else {
            panic!("expected source-segment event");
        };
        assert_eq!(activation_matches[0].text, "论文");
        assert_eq!(activation_matches[0].sources[0].subdomain, "research");
    }
}
