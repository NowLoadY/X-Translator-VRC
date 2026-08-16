# XRTranslate built-in plugin architecture

XRTranslate plugins are statically linked Rust modules. This avoids exposing a
Rust dynamic-library ABI while keeping feature ownership, host access, and
lifecycle explicit. A plugin has a stable string ID, contributes declarative UI
metadata, owns its domain runtime, and uses shared capabilities through neutral
typed contracts.

## Dependency rule

Recognition, translation, audio capture, and media import are shared
infrastructure. They must never import a concrete plugin or contain variants
named after one. Plugins configure and consume those capabilities; the host only
composes both sides.

```text
                         +--------------------+
                         |  network / audio   |
                         |   media_import     |
                         +---------^----------+
                                   |
                     neutral request/event contracts
                                   |
                 +-----------------+-----------------+
                 |                                   |
       +---------+----------+              +---------+----------+
       | host coordination  |<------------>| plugin controller  |
       | resource arbitration| typed action | and event adapter  |
       +--------------------+              +--------------------+
```

The neutral session contracts live under `session_coordinator`:

- `TranslationSessionPlugin` lets a plugin describe an active session through
  `PluginSessionBinding`; the binding carries an opaque owner, output policy,
  and lifecycle requirements rather than a Meeting/Player enum variant.
- `SessionEventSubscriber` receives the generic `SessionEvent` stream. A plugin
  adapter must enqueue blocking persistence work on its own worker.
- `HostOutputSubscriber` receives captions after host history merging. External
  presentation plugins do not need a branch inside the event pump.
- `TranslationSessionOwner::Plugin` stores opaque plugin metadata. Adding a new
  session-using plugin must not modify the owner enum or network protocol.

The dependency rule is intentionally stronger than “the network happens not to
call a plugin today”: concrete plugin imports are forbidden in shared
infrastructure.

### Recognition metadata is fact, not presentation policy

The shared recognition/translation path may publish neutral facts that several
consumers need, but it must not calculate a Meeting-, Player-, or OSC-specific
presentation. The current segment contract includes:

- stable turn and segment identity, segment order, and absolute source range;
- speaker identity, revisability, and continuous-window overlap;
- timing provenance (`utterance_window`, `estimated_text_partition`, or
  `merged_windows`) so a subtitle consumer knows whether a range was observed
  or inferred;
- the reason the recognition boundary was emitted (silence, adaptive silence,
  duration limit, speaker change, or input boundary).

A plugin decides how those facts become subtitle visibility, cue replacement,
export duration, meeting rows, or external captions. In particular, an
estimated text partition is not word alignment. Model-specific cosine distance
is also not exposed as speaker confidence: it is an internal clustering score,
not a calibrated probability. If a future recognizer provides genuine token or
word timestamps, add them as an optional neutral alignment contract rather than
embedding subtitle rules in the backend.

Speaker identity is part of the recognition result, not a plugin capability
toggle. Session plugins cannot enable or disable diarization. Presentation
plugins such as OSC may independently decide whether to render the supplied ID.

### Scheduling is a shared infrastructure policy

Plugins never choose model thread counts, queue sizes, or concrete scheduler
implementations. A neutral session is classified as `realtime` or `offline`
from its lifecycle contract: live capture is latency-sensitive, while finite
media input is throughput-oriented. The backend schedules both classes against
the configured ASR and translation slot counts, prioritizes realtime work, and
periodically admits offline work so it cannot starve.

Queueing remains bounded in every mode. Natural EOF and an explicit graceful
finish preserve ordered results and drain queued work; user cancellation or a
task switch closes the session and discards work that has not completed. Do not
turn an overload error into a larger hidden queue, and do not add a
plugin-specific model pool to make one importer faster. Extend the neutral
workload/lifecycle contract when a genuinely different scheduling requirement
appears.

## Ownership boundary

The host owns capabilities shared by features or requiring exclusive access:

- backend process and translation-session allocation;
- microphone and system-audio capture;
- streaming media-audio import and resampling;
- navigation, the persisted application-settings envelope, localization entry
  points, and the shared UI kit;
- generic recognition/translation event delivery and user-visible errors.

A plugin owns its domain state, schema, domain persistence, workers, UI, and
assets. The host may persist a plugin's settings value, but the plugin owns that
value's meaning and migration. Plugin UI may update plugin-owned controller or
draft state directly; effects requiring host capabilities must be returned as a
typed action. Plugin UI must never receive `&mut XRTranslateApp`.

```text
audio/backend session
        |
        v
typed SessionEvent -----> SessionEventSubscriber(s)
        |
        +---------------> host history / overlay
                                  |
                                  v
                         HostOutputSubscriber(s)

plugin UI --typed action--> host capability command
```

## Metadata and runtime contracts

`plugins::PluginDescriptor` is declarative metadata used by navigation and
settings. It contains the stable ID, translated label key, ordering, icon, page
scroll policy, settings contribution, and default enablement.

`plugins::PluginRegistry` is a catalogue plus persisted enablement preferences;
it is not a polymorphic runtime container. Concrete plugin instances remain in
their modules and the statically linked host adapter still registers page
rendering, settings rendering, session bindings, subscribers, and lifecycle
hooks explicitly. This explicit composition is intentional until all plugins
share a real behavior seam; metadata alone must not pretend to remove typed
runtime dispatch.

Current ownership is:

- `plugins::osc`: OSC settings, UDP listener/writer, caption formatting,
  preview/settings UI, mute-state capability, and a `HostOutputSubscriber`.
- `plugins::meeting`: meeting store, controller, recording, meeting UI, a
  `TranslationSessionPlugin` binding, and a non-blocking
  `SessionEventSubscriber`. It requests host-owned `media_import` for files.
- `plugins::player`: media tasks, playback, subtitles, player UI, and a
  `TranslationSessionPlugin` binding. It uses the same host-owned
  `media_import` capability for transcription.

Disabling always hides the plugin page and normalizes navigation. A plugin with
in-flight exclusive work rejects disablement until the work ends. Runtime
activation is capability-specific: OSC activates/deactivates its network
output, while idle Meeting/Player state remains constructed and performs no
active capture/translation work. Every plugin must document whether an idle
worker remains alive and how shutdown joins or drains it.

## Adding another built-in plugin

1. Create `rust-client/src/plugins/<id>/`. Keep its domain model, controller,
   persistence, workers, UI, tests, and assets beneath that boundary.
2. Add a descriptor and stable lowercase `PluginId`. IDs are persisted and must
   never be reused for a different feature.
3. Expose host-dependent UI effects as typed actions. Accept only a focused
   snapshot or capability handle; never accept `&mut XRTranslateApp`.
4. If it uses recognition/translation, implement `TranslationSessionPlugin` and
   return a `PluginSessionBinding`. Do not add a plugin-specific session-owner
   variant or field to `SessionConfig`/`SessionEvent`.
5. If it consumes results, implement `SessionEventSubscriber` or
   `HostOutputSubscriber` and register the adapter in the host composition
   list. Do not add a concrete-plugin branch to the generic event pump.
6. Register the statically typed runtime instance, page/settings renderer, and
   lifecycle hooks in the host adapter. These are currently explicit because
   plugin UI/action types are intentionally not erased behind `Any` or a broad
   catch-all command enum.
7. Define activation, deactivation, busy-disable, and shutdown behavior,
   including in-flight work, worker joins, and persisted configuration.
8. Add descriptor/ID migration tests, session-binding and subscriber tests when
   applicable, plus enable/disable/re-enable/shutdown lifecycle tests.

An independently distributed plugin ABI, sandbox, permission manifest, and
version negotiation remain out of scope. Those require a separate process
protocol rather than arbitrary Rust dynamic-library loading.

Architecture cleanup and plugin work must also follow the invariants and
extraction gates in [the refactoring contract](refactoring-contract.md).
