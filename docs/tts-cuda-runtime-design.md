# TTS CUDA runtime design

## Goal

Audio8 TTS uses CUDA whenever a compatible NVIDIA GPU and driver are present,
and otherwise uses CPU. Vulkan and DirectML are not selectable backends. The
application supplies its own matched native runtime, so users do not need to
install a CUDA Toolkit or modify the system `PATH`.

The same host detection and package-selection policy is shared with the
managed llama.cpp runtime. Model files remain independent downloads under
`models/` and are never bundled into an application release.

## Ownership

- `xrtranslate-config` declares immutable runtime archives and their target,
  role, CUDA ABI, required files, size, and checksum. It does not probe the
  host or perform downloads.
- The desktop host probes NVIDIA capabilities and converts the configured
  ASR, translation, and TTS providers into neutral runtime requirements.
- `runtime_install` selects declared assets, downloads them through
  `xrtranslate-download`, verifies them, and activates an app-local runtime.
- The backend receives a resolved runtime plan. ONNX and llama.cpp adapters do
  not download files, inspect the operating system, or guess package names.
- UI code consumes a diagnostic snapshot and emits typed install, repair, or
  retry actions. It does not own probing or installation.

This preserves the dependency direction in `platform-architecture.md` and
`refactoring-contract.md`.

## Neutral runtime plan

The host-level contract needs to represent requirements rather than concrete
vendor archive names:

```text
RuntimeRequirements
├─ llama_cpp: bool
├─ onnx_tts: bool
└─ onnx_cuda: bool

DetectedAccelerator
└─ Nvidia { compute_capability, driver_cuda }

ResolvedRuntimePlan
├─ backend: Cuda { abi } | Cpu
├─ assets: [declared runtime asset IDs]
└─ diagnostic: Ready | DownloadRequired | RepairRequired | Unsupported
```

`onnx_tts` expresses that Audio8 is selected; `onnx_cuda` is false only for an
explicit CPU provider configuration. The union plan records the requirements
it was built from. `plan_matches` lets the host apply provider changes with
last-write-wins semantics: an in-flight verified download finishes, then the
host recomputes the plan and reuses any newly installed matching assets.

Selection uses the highest declared CUDA ABI that is supported by both the
driver and GPU compute capability. CUDA 13 is selected only when both are
compatible; otherwise a compatible CUDA 12 package is selected. Blackwell
(50-Series, CC 12.0+) requires CUDA 12.8 or newer precompiled assets and cannot
run the declared CUDA 12.4 package. Drivers reporting CUDA 13.1/13.2 select the
declared CUDA 13.1 llama.cpp bundle; CUDA 13.3-capable drivers prefer the newer
13.3 bundle. The UI retains an NVIDIA App upgrade prompt while using 13.1.
An absent NVIDIA GPU selects CPU without downloading CUDA files. A detected but
incompatible or incomplete NVIDIA installation produces an actionable diagnostic instead
of silently claiming that CUDA is active.

The selected backend and exact ABI are runtime facts, not preferences. They
must be reported by the backend after provider initialization so the UI can
distinguish `CUDA 13`, `CUDA 12`, and `CPU` from a requested `Auto` setting.

## Package layout and size policy

Runtime resources remain split by role:

1. llama.cpp executable and backend-specific libraries;
2. a compact ONNX Runtime CPU core included in the native application (`runtime/onnxruntime/cpu/`);
3. CUDA-version-specific ONNX cores and execution-provider pairs under dedicated directories (`runtime/onnxruntime/cuda-13/`, `runtime/onnxruntime/cuda-12/`);
4. CUDA redistributable dependencies shared by compatible consumers
   (`runtime/cuda/13.3/`, `runtime/cuda/13.1/`, `runtime/cuda/12.4/`).

Only the roles required by active local providers are installed. A CPU-only
host never downloads CUDA, CUDA providers, or cuDNN. Installing TTS does not duplicate CUDA
libraries already selected for llama.cpp when their ABI and file digests are
identical. Files that differ by ABI live in separate versioned directories;
the installer must never mix CUDA 12 and CUDA 13 DLLs in one load directory.

Shared files are selected from declarative metadata and verified by SHA-256.
Sharing is based on an asset/file identity, not a coincidental filename. The
llama.cpp server is a separate process, while ONNX TTS runs in the backend, so
each launch receives an explicit, minimal dependency directory rather than a
global `PATH` mutation.

The activated selection is persisted at `<runtime_root>/native-runtime.json`. It
records separate llama.cpp and ONNX backends, the CUDA version, the exact ONNX
core, and the ordered CUDA dependencies to preload. The ONNX core,
`onnxruntime_providers_shared`, and `onnxruntime_providers_cuda` are extracted
from one official archive into one directory. This is an indivisible ABI
closure: a statically linked or differently sourced core cannot discover the
managed provider reliably on Windows. Paths are stored relative to the movable
runtime layout.

## Inference policy

`Auto` is the production default:

1. initialize the resolved CUDA execution provider;
2. verify that model nodes are assigned to CUDA and report the active ABI;
3. use CPU only when host detection selected CPU or CUDA initialization fails;
4. record the fallback reason in logs and the diagnostic snapshot.

For autoregressive TTS, model sessions are warmed before the backend reports
ready and then reused. Voice registration releases the online sessions while
the CPU encoder runs, then reloads Slow/Fast before publishing the new voice;
the first spoken translation therefore does not pay the cold-start cost.
Slow-to-Fast single-token hidden state remains device-resident through ONNX
Runtime I/O binding. The exported graphs currently accept and return growing
KV tensors, so the reusable KV buffers use CUDA-pinned host memory; replacing
that transfer requires a fused cache-update export or a safe CUDA scatter
operation, not an unsafe partial copy. Fixed-shape buffers may use CUDA Graphs
after correctness parity is established. Voice registration and codec stages may
remain on CPU when GPU placement does not improve end-to-end latency or changes
validated output.

## User experience & Onboarding Wizard

The application integrates startup prerequisite verification with a 4-step fullscreen onboarding wizard:

- **Step 1: Welcome**: Core feature introduction cards.
- **Step 2: Install models**: ASR and Translation model package selection, levels, and integrity downloads.
- **Step 3: Optional TTS**: Audio8 voice cloning setup or Skip.
- **Step 4: Inference Runtime**: llama.cpp and ONNX native acceleration installation (Option A automatic, Option B custom directory).

When mandatory prerequisites are missing on app launch, `XRTranslateApp::new` sets `first_run = true` and routes to Step 1 (Welcome). Once all prerequisites are satisfied, the user proceeds directly into the live translation session.

The Settings TTS provider card exposes only `Auto`, `CUDA`, and `CPU`:

- `Auto · CUDA 13 ready` or `Auto · CUDA 12 ready`: acceleration is active.
- `Auto · CPU`: no compatible NVIDIA GPU was detected; no repair is needed.
- `CUDA runtime required`: show download size and a Download action using the
  common progress manager.
- `CUDA runtime incomplete`: show the missing component and a Repair action.
- `CUDA unavailable`: when the user explicitly selected CUDA, explain the
  driver/GPU incompatibility and offer switching to Auto or CPU.

Before a backend session exists, the cards label the resolved choice as
`Planned · CUDA 13`, `Planned · CUDA 12`, or `Planned · CPU`. After ONNX
sessions warm successfully, the backward-compatible optional fields on the
`session_ready` event replace that label with `Active · …`. Disconnecting
clears the live diagnostic so a stale CUDA result is never presented as the
current backend.

## Verification

Required automated coverage:

- no NVIDIA device selects CPU and no CUDA assets;
- CUDA 12-only driver selects the compatible CUDA 12 plan;
- CUDA 13-capable Turing-or-newer hardware selects the CUDA 13 plan;
- Blackwell (50-Series) selects CUDA 13.1 for drivers reporting 13.1/13.2,
  prefers CUDA 13.3 when supported, never selects CUDA 12.4, and falls back to
  CPU with an upgrade prompt when no complete compatible bundle exists;
- unsupported compute capability cannot select an incompatible package;
- llama.cpp and TTS requirements reuse identical runtime assets;
- missing required DLLs produce `RepairRequired`;
- legacy runtime layouts are automatically migrated on startup;
- provider UI never offers Vulkan or DirectML;
- backend diagnostics report the actual backend and ABI after fallback.

The managed CUDA 13 closure was additionally exercised on an RTX 5070 Ti with
driver CUDA 13.3: the dynamically loaded ORT 1.28 core created both Audio8
Slow/Fast FP16 sessions with the CUDA EP in 2.28 seconds in an unoptimized test
build. This test fails if either provider is missing or silently falls back.

Performance validation must report cold initialization, warm first audio, and
steady-state real-time factor separately for CPU, CUDA 12, and CUDA 13. CUDA
results are accepted only when synthesized audio matches the CPU quality
baseline.
