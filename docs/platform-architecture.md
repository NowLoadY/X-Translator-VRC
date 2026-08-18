# Platform Architecture

Platform support is a host concern. Domain crates and plugins consume neutral
contracts and must not branch on operating-system names to download models,
start inference, or choose storage locations.

## Boundaries

- `xrtranslate-assets` owns model manifests, immutable downloads, staging,
  integrity verification, and atomic activation. Model packages are identical
  across operating systems.
- `xrtranslate-supervisor` owns the neutral `LlamaServerSpec` and process
  lifecycle. It receives an executable path and never selects a platform or
  model asset.
- `xrtranslate-config` describes runtime archives declaratively. Each archive
  declares `target`, `archive_format`, `kind`, `executable`, required files,
  and (when relevant) `cuda_version`.
  Adding Linux assets is a configuration/catalogue change, not a second
  downloader or inference pipeline.
- `rust-client/src/runtime_install.rs` performs one generic workflow: select
  assets for the current target, download with `xrtranslate-download`, verify,
  extract, and persist the resulting executable path. It must not inspect
  vendor filenames such as `*-win-*`. A small, separately named legacy
  migration path may interpret old configuration entries once; normal runtime
  selection must consume normalized metadata only.
- `rust-client/src/audio.rs` and the player window host expose capability
  methods. Unsupported host capabilities return typed/actionable errors; they
  are not represented by fake devices or duplicated UI pipelines.

## Adding a target

1. Add runtime archive metadata to `config.json` (or the release manifest) with
   the target identifier `<os>-<arch>` and the executable path inside the
   archive.
2. Keep the model manifests and backend provider plan unchanged.
3. Add host integration only where the capability genuinely differs, behind the
   existing host module boundary.
4. Add selection and lifecycle tests using declared metadata, never filename
   parsing. The generic model downloader and inference adapters must remain
   untouched.

This preserves the dependency direction in `docs/refactoring-contract.md`:
platform code composes shared capabilities, while shared capabilities remain
independent of concrete plugins and operating systems.
