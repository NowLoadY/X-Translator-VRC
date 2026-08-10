//! Axum entrypoint for the native XRTranslate backend.
//!
//! This bootstrap intentionally exposes the legacy health and WebSocket
//! contract before model execution is connected.  Keeping transport separate
//! lets the remaining engine work land without another client protocol change.

use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::get,
};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::time::{Instant, sleep};
use tokio::{
    net::TcpListener,
    sync::{mpsc, watch},
};
use tracing::{info, warn};
use xrtranslate_config::AppConfig;
use xrtranslate_engine::RouteEpoch;
use xrtranslate_inference::{AsyncHttpClient, HttpRequest, ReqwestClient};
use xrtranslate_protocol::{
    ActionControl, ClientControl, ErrorEvent, EventControl, LatencyMetrics, PcmFormat, PcmFrame,
    ServerEvent, SessionReady,
};
use xrtranslate_supervisor::{
    LlamaServerEndpoint, LlamaServerLauncher, LlamaServerProcess, LlamaServerRole, LlamaServerSpec,
    StdLlamaServerLauncher,
};
use xrtranslate_vad::{FRAME_SAMPLES, SAMPLE_RATE_HZ, Utterance};

use crate::{
    pipeline::{
        NativeInference, NativePipeline, RecognizedOutput, TimedUtterance, TranslationOutput,
        resolved_model_assets, validate_input_chunk_size, validate_input_sample_rate,
    },
    session::{SegmentContext, SessionAdapter, WireOutput},
};

mod pipeline;
mod session;

/// At most four VAD-complete turns may await local model inference per
/// WebSocket session. This bounds retained audio and prevents hidden latency
/// growth when a model server slows down.
const INFERENCE_QUEUE_CAPACITY: usize = 4;
/// Results awaiting the only WebSocket writer. This protects backend memory
/// when a client socket stops consuming messages.
const INFERENCE_RESULT_CAPACITY: usize = 32;
/// The socket writer owns this bounded queue. Keeping it separate from model
/// results makes the WebSocket write path explicit and preserves event order.
const OUTBOUND_MESSAGE_CAPACITY: usize = 64;
/// A managed Hy-MT2 server exposes four slots. One session uses at most two so
/// multi-sentence turns overlap without monopolizing all capacity.
const TRANSLATION_CONCURRENCY_PER_SESSION: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AudioEpoch(u64);

impl AudioEpoch {
    const INITIAL: Self = Self(0);

    fn advance(&mut self) {
        self.0 = self.0.checked_add(1).expect("audio epoch overflow");
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PipelineGeneration {
    route_epoch: RouteEpoch,
    audio_epoch: AudioEpoch,
}

struct InferenceJob {
    utterance: Utterance,
    source_start_ms: f64,
    source_end_ms: f64,
    generation: PipelineGeneration,
    turn_id: String,
    source_language: String,
    target_language: String,
}

enum InferenceEvent {
    Recognized {
        generation: PipelineGeneration,
        recognized: RecognizedOutput,
        segments: Vec<SegmentContext>,
    },
    Translation {
        generation: PipelineGeneration,
        asr_elapsed: Duration,
        context: SegmentContext,
        output: Result<TranslationOutput, String>,
    },
    Error {
        generation: PipelineGeneration,
        message: String,
    },
}

impl InferenceEvent {
    const fn generation(&self) -> PipelineGeneration {
        match self {
            Self::Recognized { generation, .. }
            | Self::Translation { generation, .. }
            | Self::Error { generation, .. } => *generation,
        }
    }
}

struct OutboundMessage {
    generation: Option<PipelineGeneration>,
    message: Message,
}

impl OutboundMessage {
    fn current(generation: PipelineGeneration, message: Message) -> Self {
        Self {
            generation: Some(generation),
            message,
        }
    }

    fn independent(message: Message) -> Self {
        Self {
            generation: None,
            message,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "xrtranslate-backend",
    version,
    about = "Native XRTranslate backend"
)]
struct Arguments {
    /// Path to the compatibility config.json file.
    #[arg(long, default_value = "config.json")]
    config: std::path::PathBuf,
    /// Start and own the two local llama.cpp model servers for this backend.
    ///
    /// Leave this off only when the Qwen3-ASR and Hy-MT2 endpoints are already
    /// managed by another native process.
    #[arg(long)]
    manage_llama_servers: bool,
    /// Maximum time to wait for managed llama-server instances to report ready.
    #[arg(long, default_value_t = 120)]
    model_start_timeout_seconds: u64,
}

#[derive(Clone)]
struct BackendState {
    config: AppConfig,
    project_root: PathBuf,
    next_session_id: Arc<AtomicU64>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    runtime: &'static str,
    protocol_version: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let args = Arguments::parse();
    let config = AppConfig::from_path(&args.config)?;
    let project_root = args
        .config
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    validate_native_route(&config, &project_root)?;
    let _model_processes = if args.manage_llama_servers {
        let processes = start_llama_servers(&config, &project_root)?;
        wait_for_model_servers(&config, args.model_start_timeout_seconds).await?;
        info!("managed llama.cpp model servers are ready");
        Some(processes)
    } else {
        None
    };
    let address = format!("{}:{}", config.server.host, config.server.port);
    let state = BackendState {
        config,
        project_root,
        next_session_id: Arc::new(AtomicU64::new(1)),
    };
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/ws", get(websocket))
        .with_state(state);

    let listener = TcpListener::bind(&address).await?;
    info!(%address, "native backend transport is listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        runtime: "native-gguf",
        protocol_version: xrtranslate_protocol::PROTOCOL_VERSION,
    })
}

async fn websocket(
    State(state): State<BackendState>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| serve_session(socket, state))
}

async fn serve_session(socket: WebSocket, state: BackendState) {
    let session_id = state.next_session_id.fetch_add(1, Ordering::Relaxed);
    let mut session = match SessionAdapter::new(
        &state.config.translation.source_lang,
        &state.config.translation.target_lang,
    ) {
        Ok(session) => session,
        Err(error) => {
            warn!(%session_id, %error, "invalid session route");
            return;
        }
    };
    let mut generation = PipelineGeneration {
        route_epoch: session.route_epoch(),
        audio_epoch: AudioEpoch::INITIAL,
    };
    let (generation_sender, generation_receiver) = watch::channel(generation);
    let (socket_writer, mut reader) = socket.split();
    let (outbound_sender, outbound_receiver) = mpsc::channel(OUTBOUND_MESSAGE_CAPACITY);
    let mut writer_task = tokio::spawn(run_websocket_writer(
        socket_writer,
        outbound_receiver,
        generation_receiver.clone(),
    ));
    let mut pipeline = match NativePipeline::new(&state.config, &state.project_root) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            warn!(%session_id, %error, "native pipeline initialization failed");
            let _ = send_error(&outbound_sender, error).await;
            return;
        }
    };
    let mut input_format = PcmFormat::mono_s16le(state.config.audio.sample_rate);

    let (job_sender, job_receiver) = mpsc::channel(INFERENCE_QUEUE_CAPACITY);
    let (result_sender, mut result_receiver) = mpsc::channel(INFERENCE_RESULT_CAPACITY);
    let worker = tokio::spawn(run_inference_worker(
        pipeline.inference(),
        job_receiver,
        result_sender,
        generation_receiver,
    ));
    let mut job_sender = Some(job_sender);
    let mut stopping = false;

    if send_event(
        &outbound_sender,
        None,
        ServerEvent::SessionReady(SessionReady {
            session_id: format!("native-{session_id}"),
            source_lang: session.source_lang().into(),
            target_lang: session.target_lang().into(),
        }),
    )
    .await
    .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            result = result_receiver.recv() => {
                let Some(result) = result else {
                    break;
                };
                if handle_inference_event(&outbound_sender, &mut session, generation, result).await.is_err() {
                    break;
                }
            }
            frame = reader.next(), if !stopping => {
                let Some(frame) = frame else {
                    break;
                };
                let Ok(frame) = frame else {
                    break;
                };
                match frame {
                    Message::Text(text) => match serde_json::from_str::<ClientControl>(&text) {
                        Ok(ClientControl::Action(ActionControl::SessionConfig {
                            source_lang: source,
                            target_lang: target,
                            sample_rate,
                        })) => {
                            if let Some(sample_rate) = sample_rate {
                                if let Err(error) = validate_input_sample_rate(sample_rate) {
                                    if send_error(&outbound_sender, error).await.is_err() {
                                        break;
                                    }
                                    continue;
                                }
                                input_format = PcmFormat::mono_s16le(sample_rate);
                            }
                            if let Err(error) = session.set_route(&source, &target) {
                                if send_error(&outbound_sender, error).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                            pipeline.reset();
                            generation.route_epoch = session.route_epoch();
                            generation.audio_epoch.advance();
                            generation_sender.send_replace(generation);
                            info!(%session_id, source_lang = session.source_lang(), target_lang = session.target_lang(), "session route configured");
                        }
                        Ok(ClientControl::Event(EventControl::ConfigAudio {
                            sample_rate,
                            source_lang: source,
                            target_lang: target,
                        })) => {
                            if let Err(error) = validate_input_sample_rate(sample_rate) {
                                if send_error(&outbound_sender, error).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                            input_format = PcmFormat::mono_s16le(sample_rate);
                            if let Err(error) = session.set_route(&source, &target) {
                                if send_error(&outbound_sender, error).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                            pipeline.reset();
                            generation.route_epoch = session.route_epoch();
                            generation.audio_epoch.advance();
                            generation_sender.send_replace(generation);
                            info!(%session_id, source_lang = session.source_lang(), target_lang = session.target_lang(), sample_rate, "audio configured");
                        }
                        Ok(ClientControl::Event(EventControl::Stop)) => {
                            match pipeline.flush() {
                                Ok(Some(utterance)) => {
                                    if let Some(sender) = job_sender.as_ref()
                                        && let Err(error) = enqueue_utterances(sender, &session, generation, vec![utterance])
                                    {
                                        if send_error(&outbound_sender, error).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    if send_error(&outbound_sender, error).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            job_sender.take();
                            stopping = true;
                        }
                        Ok(ClientControl::Action(ActionControl::ToggleFeature { feature, enabled })) => {
                            session.set_tts_enabled(enabled);
                            info!(%session_id, ?feature, enabled, "session feature configured");
                        }
                        Ok(ClientControl::Event(EventControl::TurnStarted { turn_id })) => {
                            session.set_turn_id(turn_id);
                        }
                        Err(error) => {
                            if send_error(&outbound_sender, format!("Invalid client control: {error}"))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    },
                    Message::Binary(audio) => {
                        if let Err(error) = validate_input_chunk_size(audio.len()) {
                            if send_error(&outbound_sender, error).await.is_err() {
                                break;
                            }
                            continue;
                        }
                        match PcmFrame::new(audio.to_vec(), input_format) {
                            Ok(frame) => match pipeline.push_pcm(frame.as_bytes()) {
                                Ok(utterances) => {
                                    if let Some(sender) = job_sender.as_ref()
                                        && let Err(error) = enqueue_utterances(sender, &session, generation, utterances)
                                    {
                                        if send_error(&outbound_sender, error).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                Err(error) => {
                                    if send_error(&outbound_sender, error).await.is_err() {
                                        break;
                                    }
                                }
                            },
                            Err(error) => {
                                if send_error(&outbound_sender, error.to_string()).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(payload) => {
                        if outbound_sender
                            .send(OutboundMessage::independent(Message::Pong(payload)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Message::Pong(_) => {}
                }
            }
        }
    }

    job_sender.take();
    if !stopping {
        worker.abort();
    }
    let _ = worker.await;
    drop(outbound_sender);
    if stopping {
        if tokio::time::timeout(Duration::from_secs(2), &mut writer_task)
            .await
            .is_err()
        {
            writer_task.abort();
        }
    } else {
        writer_task.abort();
    }
}

fn validate_native_route(config: &AppConfig, project_root: &std::path::Path) -> Result<(), String> {
    config.default_gguf().map_err(|error| error.to_string())?;
    validate_input_sample_rate(config.audio.sample_rate)?;
    resolved_model_assets(config, project_root)
        .check()
        .into_result()
        .map_err(|error| error.to_string())?;
    let vad_path = project_root.join("models/silero-vad/src/silero_vad/data/silero_vad.onnx");
    if !vad_path.is_file() {
        return Err(format!(
            "native Silero VAD model is missing: {}",
            vad_path.display()
        ));
    }
    Ok(())
}

fn start_llama_servers(
    config: &AppConfig,
    project_root: &std::path::Path,
) -> Result<Vec<LlamaServerProcess>, String> {
    let native = config.default_gguf().map_err(|error| error.to_string())?;
    if !native.llama_server_path.is_file() {
        return Err(format!(
            "llama-server executable is missing: {}",
            native.llama_server_path.display()
        ));
    }

    let assets = resolved_model_assets(config, project_root);
    assets
        .check()
        .into_result()
        .map_err(|error| error.to_string())?;
    let paths = assets.llama_cpp_paths();
    let asr_port = local_endpoint_port(&native.asr_url)?;
    let translation_port = local_endpoint_port(&native.translation_url)?;
    if asr_port == translation_port {
        return Err(format!(
            "ASR and translation llama-server endpoints both use port {asr_port}"
        ));
    }

    // Model endpoints are an implementation detail of this local backend;
    // never expose their unauthenticated OpenAI-compatible APIs to the LAN.
    let bind = |port| LlamaServerEndpoint::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let asr_spec = LlamaServerSpec::qwen3_asr_gguf(
        &native.llama_server_path,
        paths.qwen3_asr_model,
        paths.qwen3_asr_mmproj,
    )
    .with_endpoint(bind(asr_port));
    let translation_spec =
        LlamaServerSpec::hunyuan_mt_gguf(&native.llama_server_path, paths.hunyuan_mt_model)
            .with_endpoint(bind(translation_port));

    let launcher = StdLlamaServerLauncher;
    let asr = launcher
        .launch(&asr_spec)
        .map_err(|error| format!("cannot start Qwen3-ASR llama-server: {error}"))?;
    let translation = launcher
        .launch(&translation_spec)
        .map_err(|error| format!("cannot start Hy-MT2 llama-server: {error}"))?;
    info!(
        asr_port,
        translation_port, "started managed llama.cpp model servers"
    );
    Ok(vec![asr, translation])
}

async fn wait_for_model_servers(config: &AppConfig, timeout_seconds: u64) -> Result<(), String> {
    let native = config.default_gguf().map_err(|error| error.to_string())?;
    let asr_health = health_url(&native.asr_url)?;
    let translation_health = health_url(&native.translation_url)?;
    let asr_models = models_url(&native.asr_url)?;
    let translation_models = models_url(&native.translation_url)?;
    let client =
        ReqwestClient::new_direct(Duration::from_secs(2)).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);

    loop {
        let asr = check_model_ready(
            &client,
            &asr_health,
            &asr_models,
            LlamaServerRole::Qwen3Asr.model_alias(),
        )
        .await;
        let translation = check_model_ready(
            &client,
            &translation_health,
            &translation_models,
            LlamaServerRole::HunyuanMt.model_alias(),
        )
        .await;
        if asr.is_ok() && translation.is_ok() {
            return Ok(());
        }
        let last_status = format!(
            "Qwen3-ASR: {}; Hy-MT2: {}",
            health_status(&asr),
            health_status(&translation)
        );
        if Instant::now() >= deadline {
            return Err(format!(
                "managed llama.cpp model servers did not become ready within {timeout_seconds} seconds ({last_status})"
            ));
        }
        sleep(Duration::from_millis(300)).await;
    }
}

/// Confirms both llama.cpp readiness and the OpenAI model alias advertised by
/// the actual inference endpoint.  A bare `/health` response can be true for
/// a server that loaded the wrong model or failed to expose the requested API.
async fn check_model_ready(
    client: &ReqwestClient,
    health_url: &str,
    models_url: &str,
    expected_model_alias: &str,
) -> Result<(), String> {
    let response = client
        .execute(HttpRequest {
            method: "GET".into(),
            url: health_url.into(),
            headers: Vec::new(),
            body: serde_json::Value::Null,
        })
        .await
        .map_err(|error| error.to_string())?;
    if !(200..300).contains(&response.status) {
        return Err(format!("health endpoint returned HTTP {}", response.status));
    }

    let response = client
        .execute(HttpRequest {
            method: "GET".into(),
            url: models_url.into(),
            headers: Vec::new(),
            body: serde_json::Value::Null,
        })
        .await
        .map_err(|error| error.to_string())?;
    if !(200..300).contains(&response.status) {
        return Err(format!("models endpoint returned HTTP {}", response.status));
    }
    model_alias_is_advertised(&response.body, expected_model_alias)
}

fn model_alias_is_advertised(
    response_body: &str,
    expected_model_alias: &str,
) -> Result<(), String> {
    let document: serde_json::Value = serde_json::from_str(response_body)
        .map_err(|error| format!("models endpoint returned invalid JSON: {error}"))?;
    let Some(models) = document.get("data").and_then(serde_json::Value::as_array) else {
        return Err("models endpoint response is missing its data array".into());
    };
    let found = models.iter().any(|model| {
        model
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| id == expected_model_alias)
    });
    if found {
        Ok(())
    } else {
        Err(format!(
            "models endpoint does not advertise expected model alias {expected_model_alias:?}"
        ))
    }
}

fn health_status(status: &Result<(), String>) -> String {
    match status {
        Ok(()) => "ready".into(),
        Err(error) => error.clone(),
    }
}

fn local_endpoint_port(url: &str) -> Result<u16, String> {
    let parsed =
        url::Url::parse(url).map_err(|error| format!("invalid model URL {url:?}: {error}"))?;
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if !matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "0.0.0.0") {
        return Err(format!(
            "--manage-llama-servers requires a local model URL, not {url:?}"
        ));
    }
    parsed
        .port()
        .ok_or_else(|| format!("managed local model URL must include an explicit port: {url:?}"))
}

fn health_url(chat_url: &str) -> Result<String, String> {
    local_model_url(chat_url, "/health")
}

fn models_url(chat_url: &str) -> Result<String, String> {
    local_model_url(chat_url, "/v1/models")
}

fn local_model_url(chat_url: &str, path: &str) -> Result<String, String> {
    let mut parsed = url::Url::parse(chat_url)
        .map_err(|error| format!("invalid model URL {chat_url:?}: {error}"))?;
    let _ = local_endpoint_port(chat_url)?;
    if parsed.host_str() == Some("0.0.0.0") {
        parsed
            .set_host(Some("127.0.0.1"))
            .map_err(|_| "cannot construct local llama-server health URL".to_owned())?;
    }
    parsed.set_path(path);
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed.into())
}

fn enqueue_utterances(
    sender: &mpsc::Sender<InferenceJob>,
    session: &SessionAdapter,
    generation: PipelineGeneration,
    utterances: Vec<TimedUtterance>,
) -> Result<(), String> {
    debug_assert_eq!(generation.route_epoch, session.route_epoch());
    let turn_id = session.turn_id();
    let source_language = session.source_lang().to_owned();
    let target_language = session.target_lang().to_owned();
    for timed in utterances {
        let TimedUtterance {
            utterance,
            source_start_ms,
            source_end_ms,
        } = timed;
        let duration_ms = utterance.samples.len().saturating_mul(1_000) / SAMPLE_RATE_HZ as usize;
        let queued = INFERENCE_QUEUE_CAPACITY.saturating_sub(sender.capacity());
        info!(
            duration_ms,
            overlap_frames = utterance.overlap_frames,
            end_reason = ?utterance.end_reason,
            queued,
            "VAD finalized an utterance; queuing it for ASR"
        );
        sender
            .try_send(InferenceJob {
                utterance,
                source_start_ms,
                source_end_ms,
                generation,
                turn_id: turn_id.clone(),
                source_language: source_language.clone(),
                target_language: target_language.clone(),
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => format!(
                    "native inference queue is full (capacity: {INFERENCE_QUEUE_CAPACITY}); finish the current speech before sending more audio"
                ),
                mpsc::error::TrySendError::Closed(_) => {
                    "native inference worker has stopped".to_owned()
                }
            })?;
    }
    Ok(())
}

async fn run_inference_worker(
    inference: NativeInference,
    mut jobs: mpsc::Receiver<InferenceJob>,
    events: mpsc::Sender<InferenceEvent>,
    generation: watch::Receiver<PipelineGeneration>,
) {
    let mut previous_transcript: Option<(PipelineGeneration, String)> = None;
    let mut diarizer = match tokio::task::block_in_place(|| inference.speaker_diarizer()) {
        Ok(diarizer) => diarizer,
        Err(message) => {
            let current_generation = *generation.borrow();
            let _ = events
                .send(InferenceEvent::Error {
                    generation: current_generation,
                    message,
                })
                .await;
            return;
        }
    };
    let speaker_min_utterance_ms = inference.speaker_min_utterance_ms().unwrap_or(0);
    let mut diarizer_generation: Option<PipelineGeneration> = None;
    'jobs: while let Some(job) = jobs.recv().await {
        if *generation.borrow() != job.generation {
            continue;
        }
        if previous_transcript
            .as_ref()
            .is_some_and(|(previous_generation, _)| *previous_generation != job.generation)
        {
            previous_transcript = None;
        }
        if diarizer_generation != Some(job.generation) {
            if let Some(diarizer) = &mut diarizer {
                diarizer.reset();
            }
            diarizer_generation = Some(job.generation);
        }
        let duration_ms = job.source_end_ms - job.source_start_ms;
        let speaker_id = if duration_ms >= f64::from(speaker_min_utterance_ms) {
            match &mut diarizer {
                Some(diarizer) => {
                    match tokio::task::block_in_place(|| diarizer.identify(&job.utterance.samples))
                    {
                        Ok(assignment) => {
                            info!(
                                speaker_id = assignment.speaker_id,
                                similarity = assignment.similarity,
                                is_new = assignment.is_new,
                                "speaker voiceprint assigned"
                            );
                            assignment.speaker_id
                        }
                        Err(error) => {
                            warn!(%error, "speaker embedding failed; preserving ASR with unknown speaker");
                            "speaker-unknown".into()
                        }
                    }
                }
                None => String::new(),
            }
        } else if diarizer.is_some() {
            "speaker-unknown".into()
        } else {
            String::new()
        };
        let overlap_frames = job.utterance.overlap_frames;
        let mut recognized = match inference
            .transcribe(job.utterance, &job.source_language, &job.target_language)
            .await
        {
            Ok(Some(recognized)) => recognized,
            Ok(None) => continue,
            Err(message) => {
                if events
                    .send(InferenceEvent::Error {
                        generation: job.generation,
                        message,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };
        if overlap_frames > 0
            && let Some((_, previous)) = &previous_transcript
            && !recognized.remove_overlap_with(previous, &job.source_language)
        {
            info!(
                overlap_frames,
                "ASR overlap contained no new text; suppressing duplicate result"
            );
            continue;
        }
        previous_transcript = Some((job.generation, recognized.source_text.clone()));
        info!(
            asr_ms = recognized.asr_elapsed.as_millis(),
            segments = recognized.segments.len(),
            "ASR completed an utterance"
        );
        if *generation.borrow() != job.generation {
            continue;
        }
        let asr_elapsed = recognized.asr_elapsed;
        let segments = recognized.segments.clone();
        let non_overlapping_start_ms = (job.source_start_ms
            + overlap_frames as f64 * FRAME_SAMPLES as f64 * 1_000.0 / f64::from(SAMPLE_RATE_HZ))
        .min(job.source_end_ms);
        let segment_contexts = segment_contexts(
            &segments,
            job.turn_id.clone(),
            speaker_id,
            non_overlapping_start_ms,
            job.source_end_ms,
        );
        if events
            .send(InferenceEvent::Recognized {
                generation: job.generation,
                recognized,
                segments: segment_contexts.clone(),
            })
            .await
            .is_err()
        {
            break;
        }
        let translations = futures_util::stream::iter(segments.into_iter().zip(segment_contexts))
            .map(|(segment, context)| {
                let inference = inference.clone();
                let source_language = job.source_language.clone();
                let target_language = job.target_language.clone();
                async move {
                    let output = inference
                        .translate_segment(&segment, &source_language, &target_language)
                        .await;
                    (context, output)
                }
            })
            // `buffered` executes requests concurrently but yields them in source
            // order, preserving the protocol timeline and OSC display order.
            .buffered(TRANSLATION_CONCURRENCY_PER_SESSION);
        tokio::pin!(translations);
        while let Some((context, output)) = translations.next().await {
            if *generation.borrow() != job.generation {
                continue 'jobs;
            }
            if events
                .send(InferenceEvent::Translation {
                    generation: job.generation,
                    asr_elapsed,
                    context,
                    output,
                })
                .await
                .is_err()
            {
                break 'jobs;
            }
        }
    }
}

async fn handle_inference_event(
    writer: &mpsc::Sender<OutboundMessage>,
    session: &mut SessionAdapter,
    current_generation: PipelineGeneration,
    event: InferenceEvent,
) -> Result<(), axum::Error> {
    if event.generation() != current_generation {
        return Ok(());
    }
    match event {
        InferenceEvent::Recognized {
            generation,
            recognized,
            segments,
        } => {
            if !session
                .submit_recognized_for_route(generation.route_epoch, recognized.source_text, true)
                .map_err(axum::Error::new)?
            {
                return Ok(());
            }
            send_session_output(writer, session, generation).await?;
            for (segment, context) in recognized.segments.into_iter().zip(segments) {
                send_event(
                    writer,
                    Some(generation),
                    session.source_segment_ready_for_turn(segment.source_text, context),
                )
                .await?;
            }
        }
        InferenceEvent::Translation {
            generation,
            asr_elapsed,
            context,
            output,
        } => {
            let output = match output {
                Ok(output) => output,
                Err(error) if generation.route_epoch == session.route_epoch() => {
                    send_scoped_error(writer, generation, error).await?;
                    return Ok(());
                }
                Err(_) => return Ok(()),
            };
            if session
                .submit_translation_segment_for_route_and_turn(
                    generation.route_epoch,
                    output.source_text,
                    output.translated_text,
                    LatencyMetrics {
                        asr_ms: millis(asr_elapsed),
                        mt_ms: millis(output.mt_elapsed),
                        tts_ms: 0,
                    },
                    context,
                )
                .map_err(axum::Error::new)?
            {
                send_session_output(writer, session, generation).await?;
            }
        }
        InferenceEvent::Error {
            generation,
            message,
        } if generation.route_epoch == session.route_epoch() => {
            send_scoped_error(writer, generation, message).await?
        }
        InferenceEvent::Error { .. } => {}
    }
    Ok(())
}

fn segment_contexts(
    segments: &[xrtranslate_engine::TranslationSegmentPair],
    turn_id: String,
    speaker_id: String,
    source_start_ms: f64,
    source_end_ms: f64,
) -> Vec<SegmentContext> {
    let weights = segments
        .iter()
        .map(|segment| segment.source_text.chars().count().max(1))
        .collect::<Vec<_>>();
    let total_weight = weights.iter().sum::<usize>().max(1) as f64;
    let duration = (source_end_ms - source_start_ms).max(0.0);
    let mut consumed = 0usize;
    weights
        .into_iter()
        .enumerate()
        .map(|(index, weight)| {
            let start = source_start_ms + duration * consumed as f64 / total_weight;
            consumed += weight;
            let end = if index + 1 == segments.len() {
                source_end_ms
            } else {
                source_start_ms + duration * consumed as f64 / total_weight
            };
            SegmentContext {
                turn_id: turn_id.clone(),
                segment_index: u32::try_from(index + 1).unwrap_or(u32::MAX),
                segment_count: u32::try_from(segments.len()).unwrap_or(u32::MAX),
                speaker_id: speaker_id.clone(),
                source_start_ms: start,
                source_end_ms: end,
            }
        })
        .collect()
}

fn millis(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

async fn send_session_output(
    writer: &mpsc::Sender<OutboundMessage>,
    session: &mut SessionAdapter,
    generation: PipelineGeneration,
) -> Result<(), axum::Error> {
    debug_assert_eq!(generation.route_epoch, session.route_epoch());
    for output in session.drain_wire_output() {
        match output {
            WireOutput::Event(event) => send_event(writer, Some(generation), event).await?,
            WireOutput::Pcm(pcm) => writer
                .send(OutboundMessage::current(
                    generation,
                    Message::Binary(pcm.into()),
                ))
                .await
                .map_err(axum::Error::new)?,
        }
    }
    Ok(())
}

async fn send_error(
    writer: &mpsc::Sender<OutboundMessage>,
    message: String,
) -> Result<(), axum::Error> {
    send_event(writer, None, ServerEvent::Error(ErrorEvent { message })).await
}

async fn send_scoped_error(
    writer: &mpsc::Sender<OutboundMessage>,
    generation: PipelineGeneration,
    message: String,
) -> Result<(), axum::Error> {
    send_event(
        writer,
        Some(generation),
        ServerEvent::Error(ErrorEvent { message }),
    )
    .await
}

async fn send_event(
    writer: &mpsc::Sender<OutboundMessage>,
    generation: Option<PipelineGeneration>,
    event: ServerEvent,
) -> Result<(), axum::Error> {
    let json = serde_json::to_string(&event).expect("protocol DTO must serialize");
    writer
        .send(OutboundMessage {
            generation,
            message: Message::Text(json.into()),
        })
        .await
        .map_err(axum::Error::new)
}

/// The only task allowed to write to this session's WebSocket sink.
async fn run_websocket_writer(
    mut writer: futures_util::stream::SplitSink<WebSocket, Message>,
    mut messages: mpsc::Receiver<OutboundMessage>,
    mut generation: watch::Receiver<PipelineGeneration>,
) {
    while let Some(outbound) = messages.recv().await {
        if !outbound_is_current(outbound.generation, *generation.borrow_and_update()) {
            continue;
        }
        if writer.send(outbound.message).await.is_err() {
            break;
        }
    }
}

fn outbound_is_current(
    event_generation: Option<PipelineGeneration>,
    current_generation: PipelineGeneration,
) -> bool {
    event_generation.is_none_or(|event_generation| event_generation == current_generation)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::{
        AudioEpoch, PipelineGeneration, health_url, local_endpoint_port, model_alias_is_advertised,
        models_url, outbound_is_current, segment_contexts,
    };
    use xrtranslate_engine::{
        EngineConfig, Language, LanguageRoute, SessionEngine, TranslationSegmentPair,
    };

    #[test]
    fn managed_model_urls_must_be_local_and_have_distinct_explicit_ports() {
        assert_eq!(
            local_endpoint_port("http://127.0.0.1:8001/v1/chat/completions").unwrap(),
            8001
        );
        assert!(local_endpoint_port("https://example.com:8001/v1/chat/completions").is_err());
        assert!(local_endpoint_port("http://localhost/v1/chat/completions").is_err());
    }

    #[test]
    fn health_url_keeps_the_local_port_and_replaces_unspecified_bind_address() {
        assert_eq!(
            health_url("http://0.0.0.0:8002/v1/chat/completions").unwrap(),
            "http://127.0.0.1:8002/health"
        );
    }

    #[test]
    fn models_url_uses_the_local_openai_models_endpoint() {
        assert_eq!(
            models_url("http://0.0.0.0:8001/v1/chat/completions").unwrap(),
            "http://127.0.0.1:8001/v1/models"
        );
    }

    #[test]
    fn readiness_requires_the_expected_model_alias_not_just_a_healthy_server() {
        let ready = r#"{"object":"list","data":[{"id":"hy-mt2"}]}"#;
        assert!(model_alias_is_advertised(ready, "hy-mt2").is_ok());
        assert!(model_alias_is_advertised(ready, "qwen3-asr").is_err());
        assert!(model_alias_is_advertised("{}", "hy-mt2").is_err());
    }

    #[test]
    fn writer_drops_queued_events_for_an_old_route_but_keeps_controls() {
        let mut engine = SessionEngine::new(
            LanguageRoute::new(Language::new("en").unwrap(), Language::new("zh").unwrap()),
            EngineConfig::default(),
        );
        let initial = PipelineGeneration {
            route_epoch: engine.route_epoch(),
            audio_epoch: AudioEpoch::INITIAL,
        };
        let current_route = engine.set_route(LanguageRoute::new(
            Language::new("ja").unwrap(),
            Language::new("en").unwrap(),
        ));
        let current = PipelineGeneration {
            route_epoch: current_route,
            audio_epoch: initial.audio_epoch,
        };
        assert!(outbound_is_current(Some(initial), initial));
        assert!(!outbound_is_current(Some(initial), current));
        assert!(outbound_is_current(None, current));
    }

    #[test]
    fn writer_drops_queued_events_after_audio_reset_with_the_same_route() {
        let route_epoch = SessionEngine::new(
            LanguageRoute::new(Language::new("en").unwrap(), Language::new("zh").unwrap()),
            EngineConfig::default(),
        )
        .route_epoch();
        let before_reset = PipelineGeneration {
            route_epoch,
            audio_epoch: AudioEpoch::INITIAL,
        };
        let mut audio_epoch = before_reset.audio_epoch;
        audio_epoch.advance();
        let after_reset = PipelineGeneration {
            route_epoch,
            audio_epoch,
        };

        assert!(!outbound_is_current(Some(before_reset), after_reset));
        assert!(outbound_is_current(Some(after_reset), after_reset));
        assert!(outbound_is_current(None, after_reset));
    }

    #[test]
    fn source_timeline_is_monotonic_and_shared_with_one_speaker() {
        let segments = vec![
            TranslationSegmentPair {
                source_text: "Hi.".into(),
                translation_text: "Hi.".into(),
            },
            TranslationSegmentPair {
                source_text: "This is longer.".into(),
                translation_text: "This is longer.".into(),
            },
        ];
        let metadata = segment_contexts(
            &segments,
            "turn-7".into(),
            "speaker-02".into(),
            1_000.0,
            3_000.0,
        );
        assert_eq!(metadata.len(), 2);
        assert_eq!(metadata[0].speaker_id, "speaker-02");
        assert_eq!(metadata[0].turn_id, "turn-7");
        assert_eq!(metadata[0].segment_index, 1);
        assert_eq!(metadata[1].segment_index, 2);
        assert_eq!(metadata[1].segment_count, 2);
        assert_eq!(metadata[0].source_start_ms, 1_000.0);
        assert_eq!(metadata[0].source_end_ms, metadata[1].source_start_ms);
        assert_eq!(metadata[1].source_end_ms, 3_000.0);
        assert!(metadata[0].source_end_ms < 2_000.0);
    }
}
