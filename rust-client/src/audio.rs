use audioadapter_buffers::direct::InterleavedSlice;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, Stream};
use crossbeam_channel::{Receiver, Sender, bounded};
use parking_lot::Mutex;
use rubato::{Fft, FixedSync, Resampler};
use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use wasapi::{
    AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat, initialize_mta,
};

pub struct InputDevice {
    /// Stable endpoint ID. Do not use the display name as an identifier.
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct InputConfigInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
}

pub struct AudioSystem {
    host: cpal::Host,
    active_captures: Vec<ActiveCapture>,
    tts_player: Option<TtsPlayer>,
}

#[derive(Clone)]
pub struct TtsPlayerHandle {
    queue: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
}

impl TtsPlayerHandle {
    pub fn play_pcm(&self, pcm: &[u8]) {
        if pcm.len() < 2 {
            return;
        }
        let samples = pcm
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0)
            .collect::<Vec<_>>();
        let samples = resample_mono_linear(samples, 48_000, self.sample_rate);
        self.queue.lock().extend(samples);
    }
}

struct TtsPlayer {
    queue: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
    _stream: Stream,
}

enum ActiveCapture {
    Microphone(Stream),
    #[cfg(windows)]
    Loopback(LoopbackCapture),
}

impl ActiveCapture {
    fn stop(self) {
        match self {
            Self::Microphone(stream) => {
                let _ = thread::Builder::new()
                    .name("audio-stream-reaper".into())
                    .spawn(move || drop(stream));
            }
            #[cfg(windows)]
            Self::Loopback(capture) => capture.stop(),
        }
    }
}

#[cfg(windows)]
struct LoopbackCapture {
    stop_requested: Arc<AtomicBool>,
    worker: thread::JoinHandle<()>,
}

#[cfg(windows)]
impl LoopbackCapture {
    fn stop(self) {
        self.stop_requested.store(true, Ordering::Release);
        reap_worker(self.worker);
    }
}

#[cfg(windows)]
fn reap_worker(worker: thread::JoinHandle<()>) {
    let _ = thread::Builder::new()
        .name("wasapi-worker-reaper".into())
        .spawn(move || {
            let _ = worker.join();
        });
}

impl AudioSystem {
    pub fn new() -> Self {
        Self {
            host: cpal::default_host(),
            active_captures: Vec::new(),
            tts_player: None,
        }
    }

    /// List all available input devices
    pub fn available_devices(&self) -> Vec<InputDevice> {
        let mut devices = Vec::new();
        if let Ok(input_devices) = self.host.input_devices() {
            for device in input_devices {
                match (device.id(), device.description()) {
                    (Ok(id), Ok(description)) => devices.push(InputDevice {
                        id: id.to_string(),
                        name: description.name().to_owned(),
                    }),
                    (Err(error), _) | (_, Err(error)) => {
                        log::warn!("Skipping input device that cannot be described: {error}");
                    }
                }
            }
        }
        devices
    }

    /// Read the current default capture format for the selected device.
    pub fn input_config(&self, device_id: &str) -> Result<InputConfigInfo, String> {
        let device = if device_id.is_empty() {
            self.host
                .default_input_device()
                .ok_or("No default input device available.")?
        } else {
            let parsed_id = device_id
                .parse()
                .map_err(|error| format!("Invalid microphone ID '{device_id}': {error}"))?;
            self.host
                .device_by_id(&parsed_id)
                .ok_or_else(|| format!("Microphone '{device_id}' is no longer available"))?
        };
        let config = device
            .default_input_config()
            .map_err(|error| format!("Failed to read microphone format: {error}"))?;
        Ok(InputConfigInfo {
            sample_rate: config.sample_rate(),
            channels: config.channels(),
            sample_format: config.sample_format().to_string(),
        })
    }

    /// Stop the currently active audio stream
    pub fn stop(&mut self) {
        for capture in self.active_captures.drain(..) {
            capture.stop();
        }
        self.clear_tts_playback();
    }

    pub fn clear_tts_playback(&mut self) {
        if let Some(player) = &self.tts_player {
            player.queue.lock().clear();
        }
    }

    /// Get a handle to the TTS player that can be safely sent to other threads.
    pub fn tts_handle(&mut self) -> Option<TtsPlayerHandle> {
        if let Err(e) = self.ensure_tts_player() {
            log::error!("Failed to initialize TTS player: {}", e);
            return None;
        }
        self.tts_player.as_ref().map(|p| TtsPlayerHandle {
            queue: Arc::clone(&p.queue),
            sample_rate: p.sample_rate,
        })
    }

    /// Start capturing from a device by name.
    /// If name is empty, uses the system default input device.
    pub fn start_capture(
        &mut self,
        device_id: &str,
        tx: Sender<Vec<f32>>,
        level: Arc<AtomicU32>,
    ) -> Result<(), String> {
        // Build and start the replacement before replacing `active_stream`.
        // If this fails (for example, a device was unplugged), the current microphone
        // remains active and the translation session keeps receiving audio.
        let device = if device_id.is_empty() {
            self.host
                .default_input_device()
                .ok_or("No default input device available.")?
        } else {
            let parsed_id = device_id
                .parse()
                .map_err(|error| format!("Invalid microphone ID '{device_id}': {error}"))?;
            self.host
                .device_by_id(&parsed_id)
                .ok_or_else(|| format!("Microphone '{device_id}' is no longer available"))?
        };

        let config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get default input config: {}", e))?;

        let sample_rate = config.sample_rate();
        let channels = config.channels() as usize;
        let sample_format = config.sample_format();

        log::info!("Starting capture on device: '{}'", device);
        log::info!(
            "Format: {:?}, Rate: {}, Channels: {}",
            sample_format,
            sample_rate,
            channels
        );

        // We target 16kHz for the ASR model
        let target_rate = 16000;

        let stream_config: cpal::StreamConfig = config.into();

        macro_rules! build_stream {
            ($sample:ty) => {
                self.build_stream::<$sample>(
                    &device,
                    (stream_config, channels, sample_rate, target_rate),
                    tx,
                    level,
                )
            };
        }
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream!(f32),
            cpal::SampleFormat::F64 => build_stream!(f64),
            cpal::SampleFormat::I8 => build_stream!(i8),
            cpal::SampleFormat::I16 => build_stream!(i16),
            cpal::SampleFormat::I24 => build_stream!(cpal::I24),
            cpal::SampleFormat::I32 => build_stream!(i32),
            cpal::SampleFormat::I64 => build_stream!(i64),
            cpal::SampleFormat::U8 => build_stream!(u8),
            cpal::SampleFormat::U16 => build_stream!(u16),
            cpal::SampleFormat::U24 => build_stream!(cpal::U24),
            cpal::SampleFormat::U32 => build_stream!(u32),
            cpal::SampleFormat::U64 => build_stream!(u64),
            sample_format => Err(format!(
                "Unsupported microphone sample format: {sample_format}"
            )),
        }?;

        stream
            .play()
            .map_err(|e| format!("Failed to play stream: {}", e))?;
        self.add_active_capture(ActiveCapture::Microphone(stream));

        Ok(())
    }

    fn build_stream<T: Sample + cpal::SizedSample>(
        &self,
        device: &cpal::Device,
        input: (cpal::StreamConfig, usize, u32, u32),
        tx: Sender<Vec<f32>>,
        level: Arc<AtomicU32>,
    ) -> Result<Stream, String>
    where
        f32: cpal::FromSample<T>,
    {
        let (config, channels, src_rate, target_rate) = input;
        // 1. Resampler setup (if rates don't match)
        let (raw_tx, raw_rx) = bounded::<Vec<f32>>(32);
        Self::spawn_processing_worker(raw_rx, src_rate, target_rate, tx)?;

        let err_fn = |err| log::error!("An error occurred on the input audio stream: {}", err);

        let stream = device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    // Keep the high-priority CPAL callback small: format conversion and mixdown only.
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| {
                            if frame.len() >= 6 {
                                // 5.1 / 7.1 Multi-channel Dialogue Isolation (Physical Noise Cancellation):
                                // Layout: [0: Left, 1: Right, 2: Center, 3: LFE(Subwoofer), 4: Surround L, 5: Surround R]
                                // - Center (frame[2]) contains 95%+ of actor dialogue.
                                // - LFE (frame[3]) is pure low-frequency rumble/explosions (0% dialogue), completely discarded.
                                // - Surround (frame[4], frame[5]) contains ambient/reverb (0% dialogue), heavily attenuated.
                                // - Left & Right contain music & panning sound effects, kept at low ratio for rare off-center lines.
                                let l = f32::from_sample(frame[0]);
                                let r = f32::from_sample(frame[1]);
                                let c = f32::from_sample(frame[2]);
                                let ls = f32::from_sample(frame[4]);
                                let rs = f32::from_sample(frame[5]);
                                c * 0.85 + (l + r) * 0.12 + (ls + rs) * 0.03
                            } else if frame.len() == 2 {
                                let l = f32::from_sample(frame[0]);
                                let r = f32::from_sample(frame[1]);
                                (l + r) * 0.5
                            } else {
                                frame
                                    .iter()
                                    .map(|sample| f32::from_sample(*sample))
                                    .sum::<f32>()
                                    / frame.len() as f32
                            }
                        })
                        .collect();
                    update_input_level(&mono, &level);
                    let _ = raw_tx.try_send(mono);
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {}", e))?;

        Ok(stream)
    }

    fn spawn_processing_worker(
        raw_rx: Receiver<Vec<f32>>,
        src_rate: u32,
        target_rate: u32,
        output_tx: Sender<Vec<f32>>,
    ) -> Result<(), String> {
        let mut resampler = if src_rate != target_rate {
            Some(
                Fft::<f32>::new(
                    src_rate as usize,
                    target_rate as usize,
                    1024,
                    1,
                    FixedSync::Input,
                )
                .map_err(|error| format!("Failed to create resampler: {error}"))?,
            )
        } else {
            None
        };

        thread::Builder::new()
            .name("audio-resampler".into())
            .spawn(move || {
                let mut pending = VecDeque::new();
                while let Ok(samples) = raw_rx.recv() {
                    if let Some(resampler) = &mut resampler {
                        pending.extend(samples);
                        while pending.len() >= resampler.input_frames_next() {
                            let input_len = resampler.input_frames_next();
                            let input: Vec<f32> = pending.drain(..input_len).collect();
                            let output_capacity = resampler.output_frames_max();
                            let input_adapter = InterleavedSlice::new(&input, 1, input_len)
                                .expect("valid mono input");
                            let mut output = vec![0.0; output_capacity];
                            let mut output_adapter =
                                InterleavedSlice::new_mut(&mut output, 1, output_capacity)
                                    .expect("valid mono output");
                            if let Ok((_, frames_written)) = resampler.process_into_buffer(
                                &input_adapter,
                                &mut output_adapter,
                                None,
                            ) {
                                output.truncate(frames_written);
                                let _ = output_tx.try_send(output);
                            }
                        }
                    } else {
                        let _ = output_tx.try_send(samples);
                    }
                }
            })
            .map(|_| ())
            .map_err(|error| format!("Failed to start audio processing thread: {error}"))
    }

    fn add_active_capture(&mut self, capture: ActiveCapture) {
        self.active_captures.push(capture);
    }

    fn ensure_tts_player(&mut self) -> Result<(), String> {
        if self.tts_player.is_some() {
            return Ok(());
        }
        let device = self
            .host
            .default_output_device()
            .ok_or("No default audio output device available for TTS playback")?;
        let config = device
            .default_output_config()
            .map_err(|error| format!("Cannot read TTS output format: {error}"))?;
        let sample_rate = config.sample_rate();
        let channels = config.channels() as usize;
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let stream_config: cpal::StreamConfig = config.into();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                build_tts_output_stream::<f32>(&device, stream_config, channels, Arc::clone(&queue))
            }
            cpal::SampleFormat::F64 => {
                build_tts_output_stream::<f64>(&device, stream_config, channels, Arc::clone(&queue))
            }
            cpal::SampleFormat::I8 => {
                build_tts_output_stream::<i8>(&device, stream_config, channels, Arc::clone(&queue))
            }
            cpal::SampleFormat::I16 => {
                build_tts_output_stream::<i16>(&device, stream_config, channels, Arc::clone(&queue))
            }
            cpal::SampleFormat::I24 => build_tts_output_stream::<cpal::I24>(
                &device,
                stream_config,
                channels,
                Arc::clone(&queue),
            ),
            cpal::SampleFormat::I32 => {
                build_tts_output_stream::<i32>(&device, stream_config, channels, Arc::clone(&queue))
            }
            cpal::SampleFormat::I64 => {
                build_tts_output_stream::<i64>(&device, stream_config, channels, Arc::clone(&queue))
            }
            cpal::SampleFormat::U8 => {
                build_tts_output_stream::<u8>(&device, stream_config, channels, Arc::clone(&queue))
            }
            cpal::SampleFormat::U16 => {
                build_tts_output_stream::<u16>(&device, stream_config, channels, Arc::clone(&queue))
            }
            cpal::SampleFormat::U24 => build_tts_output_stream::<cpal::U24>(
                &device,
                stream_config,
                channels,
                Arc::clone(&queue),
            ),
            cpal::SampleFormat::U32 => {
                build_tts_output_stream::<u32>(&device, stream_config, channels, Arc::clone(&queue))
            }
            cpal::SampleFormat::U64 => {
                build_tts_output_stream::<u64>(&device, stream_config, channels, Arc::clone(&queue))
            }
            format => Err(format!("Unsupported TTS output sample format: {format}")),
        }?;
        stream
            .play()
            .map_err(|error| format!("Cannot start TTS output stream: {error}"))?;
        self.tts_player = Some(TtsPlayer {
            queue,
            sample_rate,
            _stream: stream,
        });
        Ok(())
    }
}

fn build_tts_output_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    queue: Arc<Mutex<VecDeque<f32>>>,
) -> Result<Stream, String>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
{
    let error_callback = |error| log::error!("TTS output stream error: {error}");
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
                let mut pending = queue.lock();
                for frame in output.chunks_mut(channels) {
                    let sample = pending.pop_front().unwrap_or(0.0);
                    for channel in frame {
                        *channel = T::from_sample(sample);
                    }
                }
            },
            error_callback,
            None,
        )
        .map_err(|error| format!("Cannot create TTS output stream: {error}"))
}

fn resample_mono_linear(samples: Vec<f32>, source_rate: u32, target_rate: u32) -> Vec<f32> {
    if source_rate == target_rate || samples.len() < 2 {
        return samples;
    }
    let output_len = samples.len() * target_rate as usize / source_rate as usize;
    (0..output_len)
        .map(|index| {
            let position = index as f64 * source_rate as f64 / target_rate as f64;
            let lower = position.floor() as usize;
            let upper = (lower + 1).min(samples.len() - 1);
            let fraction = (position - lower as f64) as f32;
            samples[lower] + (samples[upper] - samples[lower]) * fraction
        })
        .collect()
}

#[cfg(windows)]
impl AudioSystem {
    /// List playback endpoints that can be captured with WASAPI loopback.
    pub fn available_loopback_devices(&self) -> Vec<InputDevice> {
        let Ok(enumerator) = DeviceEnumerator::new() else {
            return Vec::new();
        };
        let Ok(devices) = enumerator.get_device_collection(&Direction::Render) else {
            return Vec::new();
        };
        devices
            .into_iter()
            .filter_map(Result::ok)
            .filter_map(|device| {
                Some(InputDevice {
                    id: device.get_id().ok()?,
                    name: device.get_friendlyname().ok()?,
                })
            })
            .collect()
    }

    /// Read the Windows shared-mode format used by an output endpoint.
    pub fn loopback_config(&self, device_id: &str) -> Result<InputConfigInfo, String> {
        let enumerator = DeviceEnumerator::new()
            .map_err(|error| format!("Cannot enumerate playback devices: {error}"))?;
        let device = if device_id.is_empty() {
            enumerator
                .get_default_device(&Direction::Render)
                .map_err(|error| format!("No default playback device available: {error}"))?
        } else {
            enumerator
                .get_device(device_id)
                .map_err(|error| format!("Playback device is no longer available: {error}"))?
        };
        let format = device
            .get_device_format()
            .map_err(|error| format!("Cannot read playback format: {error}"))?;
        Ok(InputConfigInfo {
            sample_rate: format.get_samplespersec(),
            channels: format.get_nchannels(),
            sample_format: format
                .get_subformat()
                .map(|sample_type| sample_type.to_string())
                .unwrap_or_else(|_| "Unknown".into()),
        })
    }

    /// Capture the selected output endpoint through Windows WASAPI loopback.
    pub fn start_loopback_capture(
        &mut self,
        device_id: &str,
        output_tx: Sender<Vec<f32>>,
        level: Arc<AtomicU32>,
    ) -> Result<(), String> {
        self.start_loopback(device_id, Some(output_tx), level, "wasapi-loopback")
    }

    fn start_loopback(
        &mut self,
        device_id: &str,
        output_tx: Option<Sender<Vec<f32>>>,
        level: Arc<AtomicU32>,
        worker_name: &str,
    ) -> Result<(), String> {
        let device_id = device_id.to_owned();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop_requested);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name(worker_name.into())
            .spawn(move || {
                if let Err(error) =
                    run_loopback_capture(&device_id, output_tx, level, worker_stop, &ready_tx)
                {
                    let _ = ready_tx.send(Err(error.clone()));
                    log::error!("WASAPI loopback capture stopped: {error}");
                }
            })
            .map_err(|error| format!("Failed to start WASAPI loopback worker: {error}"))?;

        match ready_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => {
                self.add_active_capture(ActiveCapture::Loopback(LoopbackCapture {
                    stop_requested,
                    worker,
                }));
                Ok(())
            }
            Ok(Err(error)) => {
                stop_requested.store(true, Ordering::Release);
                reap_worker(worker);
                Err(error)
            }
            Err(_) => {
                stop_requested.store(true, Ordering::Release);
                reap_worker(worker);
                Err("Timed out while opening the WASAPI loopback device".into())
            }
        }
    }
}

#[cfg(windows)]
fn run_loopback_capture(
    device_id: &str,
    output_tx: Option<Sender<Vec<f32>>>,
    level: Arc<AtomicU32>,
    stop_requested: Arc<AtomicBool>,
    ready_tx: &std::sync::mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    initialize_mta()
        .ok()
        .map_err(|error| format!("Cannot initialize WASAPI: {error}"))?;
    let (mut client, name) = open_loopback_client(device_id)?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 200_000,
    };
    client
        .initialize_client(&format, &Direction::Capture, &mode)
        .map_err(|error| format!("Cannot initialize WASAPI loopback capture: {error}"))?;
    let event = client
        .set_get_eventhandle()
        .map_err(|error| format!("Cannot create WASAPI loopback event: {error}"))?;
    let capture = client
        .get_audiocaptureclient()
        .map_err(|error| format!("Cannot create WASAPI capture client: {error}"))?;
    let raw_tx = if let Some(output_tx) = output_tx {
        let (raw_tx, raw_rx) = bounded::<Vec<f32>>(32);
        AudioSystem::spawn_processing_worker(raw_rx, 48_000, 16_000, output_tx)?;
        Some(raw_tx)
    } else {
        None
    };
    client
        .start_stream()
        .map_err(|error| format!("Cannot start WASAPI loopback capture: {error}"))?;
    log::info!("Started WASAPI loopback capture on '{name}'");
    let _ = ready_tx.send(Ok(()));

    let bytes_per_frame = format.get_blockalign() as usize;
    let mut pending = VecDeque::new();
    let mut last_audio_received = std::time::Instant::now();
    let mut last_waiting_log = std::time::Instant::now();
    while !stop_requested.load(Ordering::Acquire) {
        let pending_before_read = pending.len();
        capture
            .read_from_device_to_deque(&mut pending)
            .map_err(|error| format!("WASAPI loopback read failed: {error}"))?;
        if pending.len() > pending_before_read {
            last_audio_received = std::time::Instant::now();
        } else if last_audio_received.elapsed() >= Duration::from_secs(3)
            && last_waiting_log.elapsed() >= Duration::from_secs(3)
        {
            log::info!(
                "WASAPI loopback is running but has not received audio samples; verify that this is the active Windows playback device"
            );
            last_waiting_log = std::time::Instant::now();
        }
        while pending.len() >= bytes_per_frame * 960 {
            let samples = take_loopback_mono(&mut pending, 960);
            update_input_level(&samples, &level);
            if let Some(raw_tx) = &raw_tx {
                let _ = raw_tx.try_send(samples);
            }
        }
        let _ = event.wait_for_event(100_000);
    }
    let _ = client.stop_stream();
    Ok(())
}

fn update_input_level(samples: &[f32], level: &AtomicU32) {
    if samples.is_empty() {
        return;
    }

    update_input_level_from_energy(
        samples.iter().map(|sample| sample * sample).sum(),
        samples.len(),
        level,
    );
}

fn update_input_level_from_energy(sum: f32, sample_count: usize, level: &AtomicU32) {
    if sample_count == 0 {
        return;
    }
    let rms = (sum / sample_count as f32).sqrt().clamp(0.0, 1.0);
    let previous = f32::from_bits(level.load(Ordering::Relaxed));
    // A quick rise and gentle fall makes speech activity readable without the
    // meter flickering at the audio callback rate.
    let smoothed = if rms > previous {
        previous * 0.35 + rms * 0.65
    } else {
        previous * 0.8 + rms * 0.2
    };
    level.store(smoothed.to_bits(), Ordering::Relaxed);
}

#[cfg(windows)]
fn open_loopback_client(device_id: &str) -> Result<(AudioClient, String), String> {
    // Endpoint IDs can briefly remain enumerable while Windows is re-registering a
    // USB, Bluetooth, VR, or virtual-audio device.  In that window Activate can
    // return ERROR_FILE_NOT_FOUND (0x80070002).  Re-resolve the endpoint before
    // each attempt so the default selection also follows an endpoint change.
    const MAX_ATTEMPTS: u8 = 3;
    let target = if device_id.is_empty() {
        "the default playback device"
    } else {
        "the selected playback device"
    };
    let mut last_error = None;

    for attempt in 1..=MAX_ATTEMPTS {
        let result = (|| -> Result<(AudioClient, String), String> {
            let enumerator = DeviceEnumerator::new()
                .map_err(|error| format!("Cannot enumerate playback devices: {error}"))?;
            let device = if device_id.is_empty() {
                enumerator
                    .get_default_device(&Direction::Render)
                    .map_err(|error| format!("No default playback device available: {error}"))?
            } else {
                enumerator
                    .get_device(device_id)
                    .map_err(|error| format!("Playback device is no longer available: {error}"))?
            };
            let name = device
                .get_friendlyname()
                .unwrap_or_else(|_| "selected playback device".into());
            let client = device
                .get_iaudioclient()
                .map_err(|error| format!("Cannot open WASAPI loopback device: {error}"))?;
            Ok((client, name))
        })();

        match result {
            Ok(client) => return Ok(client),
            Err(error) => {
                last_error = Some(error);
                if attempt < MAX_ATTEMPTS {
                    log::warn!(
                        "WASAPI loopback activation failed for {target} (attempt {attempt}/{MAX_ATTEMPTS}); retrying"
                    );
                    thread::sleep(Duration::from_millis(250));
                }
            }
        }
    }

    Err(format!(
        "Cannot open WASAPI loopback device after {MAX_ATTEMPTS} attempts ({target}): {}",
        last_error.unwrap_or_else(|| "unknown error".into())
    ))
}

#[cfg(windows)]
fn take_loopback_mono(pending: &mut VecDeque<u8>, frames: usize) -> Vec<f32> {
    let mut mono = Vec::with_capacity(frames);
    for _ in 0..frames {
        let left = f32::from_le_bytes([
            pending.pop_front().unwrap_or_default(),
            pending.pop_front().unwrap_or_default(),
            pending.pop_front().unwrap_or_default(),
            pending.pop_front().unwrap_or_default(),
        ]);
        let right = f32::from_le_bytes([
            pending.pop_front().unwrap_or_default(),
            pending.pop_front().unwrap_or_default(),
            pending.pop_front().unwrap_or_default(),
            pending.pop_front().unwrap_or_default(),
        ]);
        mono.push((left + right) * 0.5);
    }
    mono
}
