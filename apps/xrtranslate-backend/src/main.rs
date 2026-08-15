//! Axum entrypoint for the native XRTranslate backend.
//!
//! This bootstrap intentionally exposes the legacy health and WebSocket
//! contract before model execution is connected.  Keeping transport separate
//! lets the remaining engine work land without another client protocol change.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
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
use xrtranslate_config::{AppConfig, LocalModelRuntimeConfig};
use xrtranslate_engine::RouteEpoch;
use xrtranslate_inference::{AsyncHttpClient, HttpRequest, ReqwestClient};
use xrtranslate_protocol::{
    ActionControl, ClientControl, DrainReason, ErrorEvent, EventControl, Feature, LatencyMetrics,
    PcmFormat, PcmFrame, PipelineDrained, RecognitionStreamEnded, RouteChanged, ServerEvent,
    SessionReady, VadActivity,
};
use xrtranslate_supervisor::{
    LlamaServerEndpoint, LlamaServerLauncher, LlamaServerProcess, LlamaServerProcessHandle,
    LlamaServerRole, LlamaServerSpec, StdLlamaServerLauncher,
};
use xrtranslate_vad::{FRAME_SAMPLES, SAMPLE_RATE_HZ, Utterance};

use crate::{
    language::AdaptiveLanguageRoute,
    pipeline::{
        NativeInference, NativePipeline, PipelineEvent, RecognizedOutput, TimedUtterance,
        TranslationOutput, resolved_model_assets, validate_input_chunk_size,
        validate_input_sample_rate,
    },
    session::{SegmentContext, SessionAdapter, WireOutput},
    terminology::{rewrite_recognition_terms, rewrite_translation_terms},
};

mod language;
mod pipeline;
mod session;
mod terminology;
use xr_corpus_client::CorpusClient;
use xr_corpus_protocol::{
    ContextBudgets, PrepareAsrRequest, PrepareTranslationRequest, RecordTranslationRequest,
    SegmentContext as CorpusSegmentContext,
};

const INFERENCE_QUEUE_CAPACITY: usize = 64;
/// Results awaiting the only WebSocket writer. This protects backend memory
/// when a client socket stops consuming messages.
const INFERENCE_RESULT_CAPACITY: usize = 32;
/// The socket writer owns this bounded queue. Keeping it separate from model
/// results makes the WebSocket write path explicit and preserves event order.
const OUTBOUND_MESSAGE_CAPACITY: usize = 64;
/// A managed Hy-MT2 server exposes four slots. One session uses at most two so
/// multi-sentence turns overlap without monopolizing all capacity.
const TRANSLATION_CONCURRENCY_PER_SESSION: usize = 2;

#[derive(Clone, Copy)]
struct StreamWindowContext {
    start_ms: f64,
    end_ms: f64,
    revisable: bool,
    overlap_ratio: f32,
}

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

struct UtteranceJob {
    utterance: Utterance,
    source_start_ms: f64,
    source_end_ms: f64,
    revisable: bool,
    generation: PipelineGeneration,
    turn_id: String,
    topic_turn_id: String,
    source_language: String,
    target_language: String,
    speaker_id: Option<String>,
}

enum InferenceJob {
    Utterance(UtteranceJob),
    StreamEnded {
        generation: PipelineGeneration,
        turn_id: String,
    },
    /// An ordered fence. The worker emits the matching event only after every
    /// inference job queued before it has completed.
    Drain {
        generation: PipelineGeneration,
        reason: DrainReason,
    },
}

enum InferenceEvent {
    WindowObserved {
        generation: PipelineGeneration,
        text_units: usize,
    },
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
    StreamEnded {
        generation: PipelineGeneration,
        turn_id: String,
    },
    Drained {
        generation: PipelineGeneration,
        reason: DrainReason,
    },
    Error {
        generation: PipelineGeneration,
        message: String,
    },
}

impl InferenceEvent {
    const fn generation(&self) -> PipelineGeneration {
        match self {
            Self::WindowObserved { generation, .. }
            | Self::Recognized { generation, .. }
            | Self::Translation { generation, .. }
            | Self::StreamEnded { generation, .. }
            | Self::Drained { generation, .. }
            | Self::Error { generation, .. } => *generation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionInputState {
    Running,
    Paused,
    Draining,
}

impl SessionInputState {
    const fn accepts_audio(self) -> bool {
        matches!(self, Self::Running)
    }

    const fn accepts_controls(self) -> bool {
        !matches!(self, Self::Draining)
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
    #[arg(long, default_value = "http://127.0.0.1:7766")]
    corpus_url: String,
}

#[derive(Clone)]
struct BackendState {
    config: AppConfig,
    corpus_client: CorpusClient,
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
async fn main() {
    if let Err(error) = run_backend().await {
        eprintln!("[XRTRANSLATE_STARTUP_ERROR] {error}");
        std::process::exit(1);
    }
}

async fn run_backend() -> Result<(), Box<dyn std::error::Error>> {
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
    let corpus_client = CorpusClient::new(&args.corpus_url)?;
    let corpus_health = corpus_client.ensure_compatible().await?;
    info!(
        count = corpus_health.corpus_count,
        api_version = corpus_health.api_version,
        "connected to XR Corpus"
    );
    let _model_processes = if args.manage_llama_servers {
        let mut processes = start_llama_servers(&config, &project_root)?;
        wait_for_model_servers(&config, args.model_start_timeout_seconds, &mut processes).await?;
        info!("managed llama.cpp model servers are ready");
        Some(processes)
    } else {
        None
    };
    let address = format!("{}:{}", config.server.host, config.server.port);
    let state = BackendState {
        config,
        corpus_client,
        project_root,
        next_session_id: Arc::new(AtomicU64::new(1)),
    };
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/ws", get(websocket))
        .route("/integrations/vrcx/status", get(vrcx_status))
        .with_state(state);

    let listener = TcpListener::bind(&address).await?;
    info!(%address, "native backend transport is listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
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

async fn vrcx_status(State(state): State<BackendState>) -> impl IntoResponse {
    match state.corpus_client.vrcx_status().await {
        Ok(status) => (axum::http::StatusCode::OK, Json(status)).into_response(),
        Err(error) => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
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
    let speaker_available = pipeline.inference().speaker_is_available();
    let speaker_recognition_enabled = Arc::new(AtomicBool::new(false));
    let speaker_state_revision = Arc::new(AtomicU64::new(0));

    let (job_sender, job_receiver) = mpsc::channel(INFERENCE_QUEUE_CAPACITY);
    let (result_sender, mut result_receiver) = mpsc::channel(INFERENCE_RESULT_CAPACITY);
    let worker = tokio::spawn(run_inference_worker(
        pipeline.inference(),
        job_receiver,
        result_sender,
        generation_receiver,
        state.corpus_client.clone(),
        Arc::clone(&speaker_recognition_enabled),
        Arc::clone(&speaker_state_revision),
    ));
    let mut job_sender = Some(job_sender);
    let mut input_state = SessionInputState::Running;
    let mut graceful_shutdown = false;
    let mut next_utterance_sequence = 1_u64;

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
                let drained = match &result {
                    InferenceEvent::Drained { reason, .. } => Some(*reason),
                    _ => None,
                };
                if let InferenceEvent::WindowObserved { text_units, .. } = &result {
                    pipeline.observe_text_density(*text_units);
                }
                if handle_inference_event(&outbound_sender, &mut session, generation, result).await.is_err() {
                    break;
                }
                if let Some(reason) = drained {
                    if send_event(
                        &outbound_sender,
                        Some(generation),
                        ServerEvent::PipelineDrained(PipelineDrained { reason }),
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                    if reason != DrainReason::Paused {
                        graceful_shutdown = true;
                        break;
                    }
                }
            }
            frame = reader.next(), if input_state.accepts_controls() => {
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
                            audio_source,
                            vad_threshold,
                            vad_silence_ms,
                            continuous_recognition,
                        })) => {
                            if let Err(error) = validate_input_sample_rate(sample_rate) {
                                if send_error(&outbound_sender, error).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                            input_format = PcmFormat::mono_s16le(sample_rate);
                            if let Err(error) = pipeline.configure_segmentation(
                                vad_threshold,
                                vad_silence_ms,
                                continuous_recognition,
                                audio_source,
                            ) {
                                if send_error(&outbound_sender, error).await.is_err() {
                                    break;
                                }
                                continue;
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
                            info!(%session_id, source_lang = session.source_lang(), target_lang = session.target_lang(), sample_rate, "audio configured");
                        }
                        Ok(ClientControl::Event(EventControl::Pause)) => {
                            if input_state == SessionInputState::Running {
                                let Some(sender) = job_sender.as_ref() else { break };
                                if let Err(error) = queue_pipeline_drain(
                                    &mut pipeline,
                                    sender,
                                    &session,
                                    generation,
                                    DrainReason::Paused,
                                    &mut next_utterance_sequence,
                                )
                                .await
                                {
                                    if send_error(&outbound_sender, error).await.is_err() {
                                        break;
                                    }
                                }
                                let mut send_failed = false;
                                for active in pipeline.take_vad_transitions() {
                                    if send_event(
                                        &outbound_sender,
                                        Some(generation),
                                        ServerEvent::VadActivity(VadActivity { active }),
                                    )
                                    .await
                                    .is_err()
                                    {
                                        send_failed = true;
                                        break;
                                    }
                                }
                                if send_failed {
                                    break;
                                }
                                input_state = SessionInputState::Paused;
                            }
                        }
                        Ok(ClientControl::Event(EventControl::Resume)) => {
                            if input_state == SessionInputState::Paused {
                                input_state = SessionInputState::Running;
                            }
                        }
                        Ok(ClientControl::Event(control @ (EventControl::Finish | EventControl::InputEnded | EventControl::Stop))) => {
                            let reason = match control {
                                EventControl::Finish => DrainReason::Finished,
                                EventControl::InputEnded => DrainReason::InputEnded,
                                EventControl::Stop => DrainReason::Stopped,
                                _ => unreachable!(),
                            };
                            let Some(sender) = job_sender.as_ref() else { break };
                            if let Err(error) = queue_pipeline_drain(
                                &mut pipeline,
                                sender,
                                &session,
                                generation,
                                reason,
                                &mut next_utterance_sequence,
                            )
                            .await
                            {
                                if send_error(&outbound_sender, error).await.is_err() {
                                    break;
                                }
                            }
                            let mut send_failed = false;
                            for active in pipeline.take_vad_transitions() {
                                if send_event(
                                    &outbound_sender,
                                    Some(generation),
                                    ServerEvent::VadActivity(VadActivity { active }),
                                )
                                .await
                                .is_err()
                                {
                                    send_failed = true;
                                    break;
                                }
                            }
                            if send_failed {
                                break;
                            }
                            job_sender.take();
                            input_state = SessionInputState::Draining;
                        }
                        Ok(ClientControl::Action(ActionControl::ToggleFeature { feature, enabled })) => {
                            match feature {
                                Feature::Tts => session.set_tts_enabled(enabled),
                                Feature::SpeakerRecognition if enabled && !speaker_available => {
                                    if send_error(
                                        &outbound_sender,
                                        "speaker recognition is unavailable; enable speaker.enabled and install its model".into(),
                                    )
                                    .await
                                    .is_err()
                                    {
                                        break;
                                    }
                                    continue;
                                }
                                Feature::SpeakerRecognition => {
                                    if speaker_recognition_enabled.swap(enabled, Ordering::AcqRel)
                                        != enabled
                                    {
                                        speaker_state_revision.fetch_add(1, Ordering::AcqRel);
                                    }
                                }
                            }
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
                        if !input_state.accepts_audio() {
                            continue;
                        }
                        if let Err(error) = validate_input_chunk_size(audio.len()) {
                            if send_error(&outbound_sender, error).await.is_err() {
                                break;
                            }
                            continue;
                        }
                        match PcmFrame::new(audio.to_vec(), input_format) {
                            Ok(frame) => match pipeline.push_pcm(frame.as_bytes()) {
                                Ok(utterances) => {
                                    let mut vad_send_failed = false;
                                    for active in pipeline.take_vad_transitions() {
                                        if send_event(
                                            &outbound_sender,
                                            Some(generation),
                                            ServerEvent::VadActivity(VadActivity { active }),
                                        )
                                        .await
                                        .is_err()
                                        {
                                            vad_send_failed = true;
                                            break;
                                        }
                                    }
                                    if vad_send_failed {
                                        break;
                                    }
                                    if let Some(sender) = job_sender.as_ref()
                                        && let Err(error) = enqueue_utterances(
                                            sender,
                                            &session,
                                            generation,
                                            utterances,
                                            &mut next_utterance_sequence,
                                        )
                                        .await
                                        && send_error(&outbound_sender, error).await.is_err() {
                                            break;
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
    if !graceful_shutdown {
        worker.abort();
    }
    let _ = worker.await;
    drop(outbound_sender);
    if graceful_shutdown {
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
    let mut asr_spec = LlamaServerSpec::qwen3_asr_gguf(
        &native.llama_server_path,
        paths.qwen3_asr_model,
        paths.qwen3_asr_mmproj,
    )
    .with_endpoint(bind(asr_port));
    apply_model_runtime(&mut asr_spec, native.asr_runtime)?;
    let mut translation_spec =
        LlamaServerSpec::hunyuan_mt_gguf(&native.llama_server_path, paths.hunyuan_mt_model)
            .with_endpoint(bind(translation_port));
    apply_model_runtime(&mut translation_spec, native.translation_runtime)?;

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

fn apply_model_runtime(
    spec: &mut LlamaServerSpec,
    runtime: LocalModelRuntimeConfig,
) -> Result<(), String> {
    spec.context_size = runtime
        .context_window_tokens
        .checked_mul(u32::from(runtime.parallel_slots))
        .ok_or("model context_window_tokens × parallel_slots exceeds u32")?;
    spec.parallel_slots = (runtime.parallel_slots > 1).then_some(runtime.parallel_slots);
    Ok(())
}

async fn wait_for_model_servers(
    config: &AppConfig,
    timeout_seconds: u64,
    processes: &mut [LlamaServerProcess],
) -> Result<(), String> {
    let native = config.default_gguf().map_err(|error| error.to_string())?;
    let asr_health = health_url(&native.asr_url)?;
    let translation_health = health_url(&native.translation_url)?;
    let asr_models = models_url(&native.asr_url)?;
    let translation_models = models_url(&native.translation_url)?;
    let client =
        ReqwestClient::new_direct(Duration::from_secs(2)).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);

    loop {
        for process in processes.iter_mut() {
            let role = process.role().model_alias();
            if let Some(status) = process
                .try_wait()
                .map_err(|error| format!("cannot inspect managed {role} process: {error}"))?
            {
                return Err(format!(
                    "managed {role} llama-server exited during startup ({status}); check whether its port is already in use and inspect the lines above this error"
                ));
            }
        }
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

async fn enqueue_utterances(
    sender: &mpsc::Sender<InferenceJob>,
    session: &SessionAdapter,
    generation: PipelineGeneration,
    utterances: Vec<PipelineEvent>,
    next_utterance_sequence: &mut u64,
) -> Result<(), String> {
    debug_assert_eq!(generation.route_epoch, session.route_epoch());
    let jobs = inference_jobs(session, generation, utterances, next_utterance_sequence)?;
    for job in jobs {
        sender
            .send(job)
            .await
            .map_err(|_| "native inference worker has stopped".to_owned())?;
    }
    Ok(())
}

fn inference_jobs(
    session: &SessionAdapter,
    generation: PipelineGeneration,
    utterances: Vec<PipelineEvent>,
    next_utterance_sequence: &mut u64,
) -> Result<Vec<InferenceJob>, String> {
    let turn_id_prefix = session.turn_id();
    let source_language = session.source_lang().to_owned();
    let target_language = session.target_lang().to_owned();
    let mut jobs = Vec::with_capacity(utterances.len());
    for event in utterances {
        if matches!(event, PipelineEvent::StreamEnded) {
            jobs.push(InferenceJob::StreamEnded {
                generation,
                turn_id: turn_id_prefix.clone(),
            });
            continue;
        }
        let PipelineEvent::Utterance(timed) = event else {
            unreachable!()
        };
        let TimedUtterance {
            utterance,
            source_start_ms,
            source_end_ms,
            revisable,
            topic_turn_sequence,
            speaker_id,
        } = timed;
        let turn_id = format!("{turn_id_prefix}:utterance-{next_utterance_sequence}");
        *next_utterance_sequence = (*next_utterance_sequence)
            .checked_add(1)
            .ok_or_else(|| "utterance identity counter exhausted".to_owned())?;
        let topic_turn_id = topic_turn_sequence.map_or_else(
            || turn_id.clone(),
            |sequence| format!("{turn_id_prefix}:generation-{generation:?}:speech-{sequence}"),
        );
        let duration_ms = utterance.samples.len().saturating_mul(1_000) / SAMPLE_RATE_HZ as usize;
        info!(
            duration_ms,
            overlap_frames = utterance.overlap_frames,
            end_reason = ?utterance.end_reason,
            "VAD finalized an utterance; queuing it for ASR"
        );
        jobs.push(InferenceJob::Utterance(UtteranceJob {
            utterance,
            source_start_ms,
            source_end_ms,
            revisable,
            generation,
            turn_id,
            topic_turn_id,
            source_language: source_language.clone(),
            target_language: target_language.clone(),
            speaker_id,
        }));
    }
    Ok(jobs)
}

/// Flushes the current VAD turn and places an ordered fence behind all queued
/// model work. Unlike live ingestion, this waits for bounded queue capacity so
/// a pause or EOF cannot silently discard its final utterance.
async fn queue_pipeline_drain(
    pipeline: &mut NativePipeline,
    sender: &mpsc::Sender<InferenceJob>,
    session: &SessionAdapter,
    generation: PipelineGeneration,
    reason: DrainReason,
    next_utterance_sequence: &mut u64,
) -> Result<(), String> {
    let flush_error = match pipeline.flush() {
        Ok(Some(utterance)) => {
            for job in inference_jobs(
                session,
                generation,
                vec![utterance],
                next_utterance_sequence,
            )? {
                sender
                    .send(job)
                    .await
                    .map_err(|_| "native inference worker has stopped".to_owned())?;
            }
            None
        }
        Ok(None) => None,
        Err(error) => Some(error),
    };
    sender
        .send(InferenceJob::Drain { generation, reason })
        .await
        .map_err(|_| "native inference worker has stopped".to_owned())?;
    flush_error.map_or(Ok(()), Err)
}

async fn run_inference_worker(
    inference: NativeInference,
    mut jobs: mpsc::Receiver<InferenceJob>,
    events: mpsc::Sender<InferenceEvent>,
    generation: watch::Receiver<PipelineGeneration>,
    corpus_client: CorpusClient,
    speaker_recognition_enabled: Arc<AtomicBool>,
    speaker_state_revision: Arc<AtomicU64>,
) {
    let mut previous_transcript: Option<(PipelineGeneration, String)> = None;
    let mut adaptive_route = AdaptiveLanguageRoute::default();
    let mut diarizer: Option<xrtranslate_speaker::OnlineSpeakerDiarizer> = None;
    let speaker_min_utterance_ms = inference.speaker_min_utterance_ms().unwrap_or(0);
    let mut diarizer_generation: Option<PipelineGeneration> = Some(*generation.borrow());
    let mut speaker_revision_seen = 0;
    let mut speaker_load_failed = false;
    let mut corpus_session = match corpus_client.create_session().await {
        Ok(session) => session,
        Err(message) => {
            let current_generation = *generation.borrow();
            let _ = events
                .send(InferenceEvent::Error {
                    generation: current_generation,
                    message: message.to_string(),
                })
                .await;
            return;
        }
    };
    'jobs: while let Some(job) = jobs.recv().await {
        let job = match job {
            InferenceJob::Utterance(job) => job,
            InferenceJob::StreamEnded {
                generation: event_generation,
                turn_id,
            } => {
                if *generation.borrow() == event_generation
                    && events
                        .send(InferenceEvent::StreamEnded {
                            generation: event_generation,
                            turn_id,
                        })
                        .await
                        .is_err()
                {
                    break;
                }
                continue;
            }
            InferenceJob::Drain {
                generation: event_generation,
                reason,
            } => {
                if *generation.borrow() == event_generation
                    && events
                        .send(InferenceEvent::Drained {
                            generation: event_generation,
                            reason,
                        })
                        .await
                        .is_err()
                {
                    break;
                }
                continue;
            }
        };
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
            adaptive_route = AdaptiveLanguageRoute::default();
            if let Some(diarizer) = &mut diarizer {
                diarizer.reset();
            }
            match corpus_client.create_session().await {
                Ok(session) => {
                    let old_session = std::mem::replace(&mut corpus_session, session);
                    tokio::spawn(async move {
                        let _ = old_session.close().await;
                    });
                }
                Err(message) => {
                    let _ = events
                        .send(InferenceEvent::Error {
                            generation: job.generation,
                            message: message.to_string(),
                        })
                        .await;
                    continue;
                }
            }
            diarizer_generation = Some(job.generation);
        }
        let overlap_frames = job.utterance.overlap_frames;
        let (asr_tokens, translation_tokens) = inference.prompt_context_token_budgets();
        let context_budgets = ContextBudgets {
            asr_tokens,
            translation_tokens,
        };
        adaptive_route.configure(&job.source_language, &job.target_language);
        let active_target_language = adaptive_route.active_targets(&job.target_language);
        let asr_context = match corpus_session
            .prepare_asr(&PrepareAsrRequest {
                source_language: job.source_language.clone(),
                target_language: active_target_language,
                budgets: context_budgets,
            })
            .await
        {
            Ok(prompt) => prompt,
            Err(message) => {
                if events
                    .send(InferenceEvent::Error {
                        generation: job.generation,
                        message: message.to_string(),
                    })
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };
        let mut recognized = match inference
            .transcribe(
                &job.utterance.samples,
                &job.source_language,
                &job.target_language,
                &mut adaptive_route,
                asr_context.prompt,
                &asr_context.echo_guard,
            )
            .await
        {
            Ok(Some(recognized)) => recognized,
            Ok(None) => {
                if events
                    .send(InferenceEvent::WindowObserved {
                        generation: job.generation,
                        text_units: 0,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
            Err(message) => {
                if events
                    .send(InferenceEvent::Error {
                        generation: job.generation,
                        message: message.to_string(),
                    })
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };
        if !job.revisable
            && overlap_frames > 0
            && let Some((_, previous)) = &previous_transcript
            && !recognized.remove_overlap_with(previous)
        {
            info!(
                overlap_frames,
                "ASR overlap contained no new text; suppressing duplicate result"
            );
            if events
                .send(InferenceEvent::WindowObserved {
                    generation: job.generation,
                    text_units: 0,
                })
                .await
                .is_err()
            {
                break;
            }
            continue;
        }
        if job.revisable {
            recognized.prepare_revisable_snapshot();
        }
        if *generation.borrow() != job.generation {
            continue;
        }
        if events
            .send(InferenceEvent::WindowObserved {
                generation: job.generation,
                text_units: text_density_units(&recognized.source_text),
            })
            .await
            .is_err()
        {
            break;
        }
        let translation_context = match corpus_session
            .prepare_translation(&PrepareTranslationRequest {
                asr_context_id: asr_context.context_id,
                turn_id: Some(job.topic_turn_id.clone()),
                source_language: recognized.source_language.clone(),
                target_language: recognized.target_language.clone(),
                recognized_text: recognized.source_text.clone(),
                segments: recognized
                    .segments
                    .iter()
                    .map(|segment| segment.translation_text.clone())
                    .collect(),
                budgets: context_budgets,
            })
            .await
        {
            Ok(context) => context,
            Err(message) => {
                if events
                    .send(InferenceEvent::Error {
                        generation: job.generation,
                        message: message.to_string(),
                    })
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };
        let source_rewrite = rewrite_recognition_terms(
            &recognized.source_text,
            &translation_context.source_corrections,
        );
        if source_rewrite.corrected_text != recognized.source_text {
            info!(
                before = %recognized.source_text,
                after = %source_rewrite.corrected_text,
                "applied XR Corpus ASR terminology correction"
            );
            recognized.apply_source_correction(source_rewrite.corrected_text.clone());
        }
        if job.revisable {
            recognized.prepare_revisable_snapshot();
        }
        for (segment, context) in recognized
            .segments
            .iter_mut()
            .zip(&translation_context.segments)
        {
            let rewrite =
                rewrite_recognition_terms(&segment.translation_text, &context.source_corrections);
            segment.translation_text = rewrite.corrected_text;
            segment.source_text.clone_from(&segment.translation_text);
        }
        previous_transcript = Some((job.generation, recognized.source_text.clone()));
        info!(
            asr_ms = recognized.asr_elapsed.as_millis(),
            segments = recognized.segments.len(),
            "ASR completed an utterance"
        );
        let speaker_enabled = speaker_recognition_enabled.load(Ordering::Acquire);
        let speaker_revision = speaker_state_revision.load(Ordering::Acquire);
        if speaker_revision != speaker_revision_seen {
            if let Some(diarizer) = &mut diarizer {
                diarizer.reset();
            }
            speaker_revision_seen = speaker_revision;
            speaker_load_failed = false;
        }
        let duration_ms = job.source_end_ms - job.source_start_ms;
        let speaker_id = if let Some(assigned) = job.speaker_id {
            assigned
        } else if speaker_enabled && duration_ms >= f64::from(speaker_min_utterance_ms) {
            if diarizer.is_none() && !speaker_load_failed {
                match tokio::task::block_in_place(|| inference.speaker_diarizer()) {
                    Ok(loaded) => diarizer = loaded,
                    Err(error) => {
                        speaker_load_failed = true;
                        warn!(%error, "speaker model failed to initialize; preserving ASR");
                    }
                }
            }
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
                None => "speaker-unknown".into(),
            }
        } else if speaker_enabled {
            "speaker-unknown".into()
        } else {
            String::new()
        };
        let asr_elapsed = recognized.asr_elapsed;
        let source_language = recognized.source_language.clone();
        let target_language = recognized.target_language.clone();
        let segments = recognized.segments.clone();
        let overlap_ms =
            overlap_frames as f64 * FRAME_SAMPLES as f64 * 1_000.0 / f64::from(SAMPLE_RATE_HZ);
        let non_overlapping_start_ms = if job.revisable {
            job.source_start_ms
        } else {
            (job.source_start_ms + overlap_ms).min(job.source_end_ms)
        };
        let window_ms = (job.source_end_ms - job.source_start_ms).max(1.0);
        let segment_contexts = segment_contexts(
            &segments,
            &translation_context.segments,
            job.turn_id.clone(),
            speaker_id,
            StreamWindowContext {
                start_ms: non_overlapping_start_ms,
                end_ms: job.source_end_ms,
                revisable: job.revisable,
                overlap_ratio: (overlap_ms / window_ms) as f32,
            },
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
        let translations = futures_util::stream::iter(
            segments
                .into_iter()
                .zip(segment_contexts)
                .zip(translation_context.segments),
        )
        .map(|((segment, segment_context), corpus_context)| {
            let inference = inference.clone();
            let source_language = source_language.clone();
            let target_language = target_language.clone();
            let source_for_terms = segment.translation_text.clone();
            let prompt_terms = corpus_context.prompt_terms.clone();
            async move {
                let output = inference
                    .translate_segment(
                        &segment,
                        &source_language,
                        &target_language,
                        corpus_context.prompt,
                    )
                    .await;
                (segment_context, source_for_terms, prompt_terms, output)
            }
        })
        .buffered(TRANSLATION_CONCURRENCY_PER_SESSION);
        tokio::pin!(translations);
        while let Some((segment_context, source_for_terms, prompt_terms, mut output)) =
            translations.next().await
        {
            if *generation.borrow() != job.generation {
                continue 'jobs;
            }
            if let Ok(translated) = &mut output {
                let rewrite = rewrite_translation_terms(
                    &source_for_terms,
                    &translated.translated_text,
                    &translated.target_language,
                    &prompt_terms,
                );
                translated.translated_text = rewrite.translated_text;
                translated.term_matches = rewrite.term_matches;
                match corpus_session
                    .record_translation(&RecordTranslationRequest {
                        context_id: translation_context.context_id,
                        source_language: translated.source_language.clone(),
                        target_language: translated.target_language.clone(),
                        source_text: translated.source_text.clone(),
                        translated_text: translated.translated_text.clone(),
                    })
                    .await
                {
                    Ok(_) => {}
                    Err(message) => {
                        warn!(%message, "could not record XR Corpus translation context")
                    }
                }
            }
            if events
                .send(InferenceEvent::Translation {
                    generation: job.generation,
                    asr_elapsed,
                    context: segment_context,
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
        InferenceEvent::WindowObserved { .. } => {}
        InferenceEvent::Recognized {
            generation,
            recognized,
            segments,
        } => {
            if let Some(target_lang) = &recognized.route_switched {
                send_event(
                    writer,
                    Some(generation),
                    ServerEvent::RouteChanged(RouteChanged {
                        source_lang: "auto".to_string(),
                        target_lang: target_lang.clone(),
                    }),
                )
                .await?;
            }
            let turn_id = segments
                .first()
                .map(|context| context.turn_id.clone())
                .unwrap_or_else(|| session.turn_id());
            if !session
                .submit_recognized_for_route_and_turn(
                    generation.route_epoch,
                    recognized.source_text,
                    true,
                    turn_id,
                )
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
                    output.term_matches,
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
        InferenceEvent::StreamEnded { turn_id, .. } => {
            send_event(
                writer,
                Some(current_generation),
                ServerEvent::RecognitionStreamEnded(RecognitionStreamEnded { turn_id }),
            )
            .await?;
        }
        InferenceEvent::Drained { .. } => {}
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

fn text_density_units(text: &str) -> usize {
    let mut units = 0;
    let mut in_word = false;
    for character in text.chars() {
        if matches!(
            character as u32,
            0x3040..=0x30FF | 0x3400..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF
        ) {
            units += 1;
            in_word = false;
        } else if character.is_alphanumeric() || character == '\'' {
            if !in_word {
                units += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    units
}

fn segment_contexts(
    segments: &[xrtranslate_engine::TranslationSegmentPair],
    corpus_contexts: &[CorpusSegmentContext],
    turn_id: String,
    speaker_id: String,
    window: StreamWindowContext,
) -> Vec<SegmentContext> {
    let weights = segments
        .iter()
        .map(|segment| segment.source_text.chars().count().max(1))
        .collect::<Vec<_>>();
    let total_weight = weights.iter().sum::<usize>().max(1) as f64;
    let duration = (window.end_ms - window.start_ms).max(0.0);
    let mut consumed = 0usize;
    weights
        .into_iter()
        .enumerate()
        .map(|(index, weight)| {
            let start = window.start_ms + duration * consumed as f64 / total_weight;
            consumed += weight;
            let end = if index + 1 == segments.len() {
                window.end_ms
            } else {
                window.start_ms + duration * consumed as f64 / total_weight
            };
            let corpus_context = corpus_contexts.get(index);
            SegmentContext {
                turn_id: turn_id.clone(),
                segment_index: u32::try_from(index + 1).unwrap_or(u32::MAX),
                segment_count: u32::try_from(segments.len()).unwrap_or(u32::MAX),
                speaker_id: speaker_id.clone(),
                source_start_ms: start,
                source_end_ms: end,
                revisable: window.revisable,
                overlap_ratio: window.overlap_ratio,
                activation_matches: corpus_context
                    .map(|context| context.activation_matches.clone())
                    .unwrap_or_default(),
                context_matches: corpus_context.map_or_else(Vec::new, |context| {
                    let mut matches = context.context_matches.clone();
                    let rewrite = rewrite_recognition_terms(
                        &context.corrected_text,
                        &context.source_corrections,
                    );
                    matches.extend(rewrite.term_matches);
                    matches
                }),
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
        AudioEpoch, PipelineGeneration, SessionInputState, StreamWindowContext,
        apply_model_runtime, health_url, local_endpoint_port, model_alias_is_advertised,
        models_url, outbound_is_current, segment_contexts,
    };
    use xrtranslate_config::LocalModelRuntimeConfig;
    use xrtranslate_engine::{
        EngineConfig, Language, LanguageRoute, SessionEngine, TranslationSegmentPair,
    };
    use xrtranslate_supervisor::LlamaServerSpec;

    #[test]
    fn paused_sessions_keep_controls_but_reject_binary_audio() {
        assert!(SessionInputState::Running.accepts_audio());
        assert!(SessionInputState::Running.accepts_controls());
        assert!(!SessionInputState::Paused.accepts_audio());
        assert!(SessionInputState::Paused.accepts_controls());
        assert!(!SessionInputState::Draining.accepts_audio());
        assert!(!SessionInputState::Draining.accepts_controls());
    }

    #[test]
    fn per_request_context_is_multiplied_by_parallel_slots() {
        let mut spec = LlamaServerSpec::hunyuan_mt_gguf("llama-server", "hy-mt2.gguf");
        apply_model_runtime(
            &mut spec,
            LocalModelRuntimeConfig {
                context_window_tokens: 2_048,
                max_tokens: 256,
                parallel_slots: 2,
            },
        )
        .unwrap();
        assert_eq!(spec.context_size, 4_096);
        assert_eq!(spec.parallel_slots, Some(2));
    }

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
            &[],
            "turn-7".into(),
            "speaker-02".into(),
            StreamWindowContext {
                start_ms: 1_000.0,
                end_ms: 3_000.0,
                revisable: false,
                overlap_ratio: 0.0,
            },
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
