# XRTranslate built-in plugin architecture

XRTranslate plugins are statically linked Rust modules. This avoids exposing a
Rust dynamic-library ABI while still keeping feature ownership and host access
explicit. A plugin is identified by a stable string ID, contributes navigation
and settings metadata, owns its runtime state, and communicates with the host
through typed events and commands.

## Ownership boundary

The host owns capabilities that are shared or exclusive across features:

- backend and translation-session lifecycle;
- microphone and system-audio capture;
- streaming media-audio import and resampling;
- navigation, settings persistence, localization, and the shared UI kit;
- typed recognition/translation events and user-visible error reporting.

Each plugin owns its domain-specific state, persistence, background workers,
page UI, settings UI, and assets. Plugin UI must not start workers or mutate the
root application directly. It renders from a small snapshot and returns a typed
action for the host to execute.

```text
audio/backend session
        |
        v
typed SessionEvent -----> host history / overlay
        |
        +---------------> OSC caption subscriber
        |
        +---------------> Meeting segment subscriber

plugin UI --PluginAction--> host capability command
```

## Built-in plugin contract

`plugins::PluginDescriptor` is the declarative contract used by navigation and
settings. It contains the stable ID, translated label key, ordering, icon, page
scroll policy, and whether the plugin contributes settings UI.

`plugins::PluginRegistry` stores only enabled/disabled preferences and metadata.
Runtime implementations remain in their plugin module:

- `plugins::osc` owns OSC settings, UDP listener/writer, caption formatting,
  preview UI, and mute-state capability.
- `plugins::meeting` owns meeting storage, controller, retained recording, and
  meeting UI. It requests the host-owned `media_import` capability for external
  audio.
- `plugins::player` owns media tasks, playback, subtitle state, and player UI.
  It uses the same host-owned `media_import` capability for transcription.

Disabling a plugin hides its page and deactivates its background capability.
Disabling a busy plugin is rejected until its active operation has ended. If a
persisted or current page belongs to a disabled plugin, navigation falls back to
the core Translation page.

## Adding another built-in plugin

1. Add one module under `rust-client/src/plugins/<id>` and keep all domain files,
   UI, tests, and assets below that boundary.
2. Add a descriptor with a stable lowercase ID. IDs are persisted and must never
   be reused for a different feature.
3. Consume host state through a purpose-built snapshot or capability handle.
   Return typed actions for mutations; do not accept `&mut XRTranslateApp`.
4. Subscribe only to the typed events the plugin needs. Core networking and audio
   code must not import the plugin module.
5. Define activation, deactivation, and shutdown behavior, including what happens
   to in-flight work and persisted configuration.
6. Add migration tests for legacy settings/page IDs and lifecycle tests for
   enable, disable, re-enable, and application shutdown.

An independently distributed plugin ABI, sandbox, permission manifest, and
version negotiation are intentionally out of scope. Those should be designed as
a separate process protocol rather than loading arbitrary Rust dynamic libraries.

Architecture cleanup and new plugin work must also follow the invariants and
extraction gates in [the refactoring contract](refactoring-contract.md).
