//! Resolves configured model providers into one backend runtime plan.
//!
//! Provider-specific knowledge belongs here: the transport entrypoint and
//! session pipeline consume this plan without branching on model names.

use std::{
    net::{IpAddr, Ipv4Addr},
    path::Path,
};

use xrtranslate_assets::{
    ModelAssetId, ModelAssetsConfig, ModelCapability, ModelFileRole, ResolvedModelAsset,
    ResolvedModelAssets,
};
use xrtranslate_config::{
    AppConfig, LocalModelRuntimeConfig, NativeModelRouteConfig, RuntimeLayout,
};
use xrtranslate_inference::{
    AsrTranscript, InferenceError, Qwen3AsrAdapter, Qwen3AsrOptions, ReqwestClient,
    TranslationAdapter, TranslationProvider,
};
use xrtranslate_supervisor::{LlamaServerEndpoint, LlamaServerSpec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AsrProfile {
    Qwen3Local,
    OpenAiAudio,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TranslationProfile {
    HunyuanLocal,
    OpenAiCompatible,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeAsrOptions {
    pub(crate) language: Option<String>,
    pub(crate) prompt_context: Option<String>,
    pub(crate) max_tokens: u32,
}

/// Provider-erased ASR adapter consumed by the generic pipeline. New native
/// ASR families add one dispatch variant here without leaking their options
/// into session processing.
#[derive(Clone, Debug)]
pub(crate) enum NativeAsrAdapter {
    Qwen3(Qwen3AsrAdapter<ReqwestClient>),
}

impl NativeAsrAdapter {
    pub(crate) async fn transcribe_pcm16(
        &self,
        pcm: &[u8],
        options: NativeAsrOptions,
    ) -> Result<AsrTranscript, InferenceError> {
        match self {
            Self::Qwen3(adapter) => {
                adapter
                    .transcribe_pcm16(
                        pcm,
                        Qwen3AsrOptions {
                            language: options.language,
                            prompt_context: options.prompt_context,
                            max_tokens: options.max_tokens,
                        },
                    )
                    .await
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativeProviderPlan {
    route: NativeModelRouteConfig,
    assets: ResolvedModelAssets,
    asr_profile: AsrProfile,
    translation_profile: TranslationProfile,
    translation_supports_prompt_context: bool,
}

impl NativeProviderPlan {
    pub(crate) fn resolve(config: &AppConfig, project_root: &Path) -> Result<Self, String> {
        let mut route = config
            .native_model_route()
            .map_err(|error| error.to_string())?;
        route.llama_server_path = RuntimeLayout::for_project_root(project_root)
            .resolve_configured_path(&route.llama_server_path);
        let asr_profile = AsrProfile::registered(&route.asr.provider, &route.asr.transport)
            .ok_or_else(|| format!("unsupported ASR provider {:?}", route.asr.provider))?;
        let translation_profile = TranslationProfile::registered(
            &route.translation.provider,
            &route.translation.transport,
        )
        .ok_or_else(|| {
            format!(
                "unsupported translation provider {:?}",
                route.translation.provider
            )
        })?;
        let asr_asset_id = route
            .asr
            .uses_local_runtime()
            .then(|| {
                route_asset_id(
                    &route.asr,
                    asr_profile.default_asset(),
                    ModelCapability::Asr,
                )
            })
            .transpose()?;
        let translation_asset_id = route
            .translation
            .uses_local_runtime()
            .then(|| {
                route_asset_id(
                    &route.translation,
                    translation_profile.default_asset(),
                    ModelCapability::Translation,
                )
            })
            .transpose()?;
        let assets = resolve_model_assets(
            config,
            project_root,
            asr_asset_id.into_iter().chain(translation_asset_id),
        );
        let translation_supports_prompt_context = route.translation.supports_prompt_context;

        Ok(Self {
            route,
            assets,
            asr_profile,
            translation_profile,
            translation_supports_prompt_context,
        })
    }

    pub(crate) fn check_assets(&self) -> Result<(), String> {
        if !self.uses_local_runtime() {
            return Ok(());
        }
        self.assets
            .check()
            .into_result()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn uses_local_runtime(&self) -> bool {
        self.route.uses_local_runtime()
    }

    pub(crate) fn asr_uses_local_runtime(&self) -> bool {
        self.route.asr.uses_local_runtime()
    }

    pub(crate) fn translation_uses_local_runtime(&self) -> bool {
        self.route.translation.uses_local_runtime()
    }

    pub(crate) fn asr_http_client(&self) -> Result<ReqwestClient, String> {
        if self.asr_uses_local_runtime() {
            ReqwestClient::with_default_direct_timeout().map_err(|error| error.to_string())
        } else {
            ReqwestClient::with_default_timeout().map_err(|error| error.to_string())
        }
    }

    pub(crate) fn translation_http_client(&self) -> Result<ReqwestClient, String> {
        if self.translation_uses_local_runtime() {
            ReqwestClient::with_default_direct_timeout().map_err(|error| error.to_string())
        } else {
            ReqwestClient::with_default_timeout().map_err(|error| error.to_string())
        }
    }

    pub(crate) fn llama_server_path(&self) -> &Path {
        &self.route.llama_server_path
    }

    pub(crate) fn asr_runtime(&self) -> LocalModelRuntimeConfig {
        self.route.asr.runtime
    }

    pub(crate) fn translation_runtime(&self) -> LocalModelRuntimeConfig {
        self.route.translation.runtime
    }

    pub(crate) fn asr_url(&self) -> &str {
        &self.route.asr.url
    }

    pub(crate) fn translation_url(&self) -> &str {
        &self.route.translation.url
    }

    pub(crate) fn asr_model_alias(&self) -> &str {
        match self.asr_profile {
            AsrProfile::Qwen3Local => "qwen3-asr",
            AsrProfile::OpenAiAudio => &self.route.asr.model,
        }
    }

    pub(crate) fn translation_model_alias(&self) -> &str {
        match self.translation_profile {
            TranslationProfile::HunyuanLocal => "hy-mt2",
            TranslationProfile::OpenAiCompatible => &self.route.translation.model,
        }
    }

    pub(crate) fn translation_supports_prompt_context(&self) -> bool {
        self.translation_supports_prompt_context
    }

    pub(crate) fn asr_adapter(
        &self,
        http: ReqwestClient,
    ) -> Result<NativeAsrAdapter, InferenceError> {
        match self.asr_profile {
            AsrProfile::Qwen3Local => {
                Qwen3AsrAdapter::new(http, self.asr_url(), self.asr_model_alias())
                    .map(NativeAsrAdapter::Qwen3)
            }
            AsrProfile::OpenAiAudio => {
                let adapter = if let Some(token) = self.route.asr.api_key.as_deref() {
                    Qwen3AsrAdapter::with_bearer_token(
                        http,
                        self.asr_url(),
                        self.asr_model_alias(),
                        token,
                    )
                } else {
                    Qwen3AsrAdapter::new(http, self.asr_url(), self.asr_model_alias())
                }?;
                Ok(NativeAsrAdapter::Qwen3(adapter))
            }
        }
    }

    pub(crate) fn translation_adapter(
        &self,
        http: ReqwestClient,
    ) -> Result<TranslationAdapter<ReqwestClient>, InferenceError> {
        match self.translation_profile {
            TranslationProfile::HunyuanLocal => TranslationAdapter::new(
                http,
                self.translation_url(),
                self.translation_model_alias(),
                TranslationProvider::Hunyuan,
            ),
            TranslationProfile::OpenAiCompatible => {
                if let Some(token) = self.route.translation.api_key.as_deref() {
                    TranslationAdapter::with_bearer_token(
                        http,
                        self.translation_url(),
                        self.translation_model_alias(),
                        TranslationProvider::OpenAiCompatible,
                        token,
                    )
                } else {
                    TranslationAdapter::new(
                        http,
                        self.translation_url(),
                        self.translation_model_alias(),
                        TranslationProvider::OpenAiCompatible,
                    )
                }
            }
        }
    }

    pub(crate) fn managed_server_specs(
        &self,
        asr_port: u16,
        translation_port: u16,
    ) -> Result<(Option<LlamaServerSpec>, Option<LlamaServerSpec>), String> {
        let bind = |port| LlamaServerEndpoint::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let asr = if self.asr_uses_local_runtime() {
            let asset = self.assets.active_asset(ModelCapability::Asr);
            let mut spec = LlamaServerSpec::qwen3_asr_gguf(
                self.llama_server_path(),
                model_file(asset, ModelFileRole::Weights)?,
                model_file(asset, ModelFileRole::MultimodalProjection)?,
            )
            .with_endpoint(bind(asr_port));
            apply_model_runtime(&mut spec, self.asr_runtime())?;
            Some(spec)
        } else {
            None
        };
        let translation = if self.translation_uses_local_runtime() {
            let asset = self.assets.active_asset(ModelCapability::Translation);
            let mut spec = LlamaServerSpec::hunyuan_mt_gguf(
                self.llama_server_path(),
                model_file(asset, ModelFileRole::Weights)?,
            )
            .with_endpoint(bind(translation_port));
            apply_model_runtime(&mut spec, self.translation_runtime())?;
            Some(spec)
        } else {
            None
        };
        Ok((asr, translation))
    }
}

fn model_file(
    asset: &ResolvedModelAsset,
    role: ModelFileRole,
) -> Result<std::path::PathBuf, String> {
    asset.file_path(role).ok_or_else(|| {
        format!(
            "model asset {} does not declare required file role {role:?}",
            asset.manifest().id
        )
    })
}

impl AsrProfile {
    fn registered(provider: &str, transport: &str) -> Option<Self> {
        if transport == "openai" {
            return Some(Self::OpenAiAudio);
        }
        match provider {
            "qwen3-gguf" => Some(Self::Qwen3Local),
            _ => None,
        }
    }

    const fn default_asset(self) -> ModelAssetId {
        match self {
            Self::Qwen3Local => ModelAssetId::Qwen3AsrGguf,
            Self::OpenAiAudio => ModelAssetId::Qwen3AsrGguf,
        }
    }
}

impl TranslationProfile {
    fn registered(provider: &str, transport: &str) -> Option<Self> {
        if transport == "openai" {
            return Some(Self::OpenAiCompatible);
        }
        match provider {
            "hunyuan" => Some(Self::HunyuanLocal),
            _ => None,
        }
    }

    const fn default_asset(self) -> ModelAssetId {
        match self {
            Self::HunyuanLocal => ModelAssetId::HunyuanMtGguf,
            Self::OpenAiCompatible => ModelAssetId::HunyuanMtGguf,
        }
    }
}

fn route_asset_id(
    provider: &xrtranslate_config::NativeProviderConfig,
    fallback: ModelAssetId,
    capability: ModelCapability,
) -> Result<ModelAssetId, String> {
    let Some(key) = provider.model_asset.as_deref() else {
        return Ok(fallback);
    };
    let id = ModelAssetId::from_config_key(key).ok_or_else(|| {
        format!(
            "unknown model asset {key:?} for provider {:?}",
            provider.provider
        )
    })?;
    let manifest = xrtranslate_assets::manifest_for(id);
    if manifest.provider != provider.provider || manifest.capability != capability {
        return Err(format!(
            "model asset {key:?} does not belong to provider {:?} for {capability:?}",
            provider.provider
        ));
    }
    Ok(id)
}

fn resolve_model_assets(
    config: &AppConfig,
    project_root: &Path,
    active_asset_ids: impl IntoIterator<Item = ModelAssetId>,
) -> ResolvedModelAssets {
    let mut assets = ModelAssetsConfig::with_directory_overrides(
        config.model_manager.models_directory.clone(),
        config.model_manager.qwen3_asr_gguf_directory.clone(),
        config.model_manager.hunyuan_mt_gguf_directory.clone(),
    );
    for id in active_asset_ids {
        assets.select_asset(id);
    }
    assets.resolve(project_root)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn big_translation_level_selects_the_7b_model_for_backend_launch() {
        let mut document: serde_json::Value =
            serde_json::from_str(include_str!("../../../config.json")).unwrap();
        document["translation"]["providers"]["hunyuan"]["model_asset"] =
            serde_json::Value::from("hy-mt2-big");
        let config = AppConfig::from_value(document).unwrap();

        let plan = NativeProviderPlan::resolve(&config, Path::new("release-root")).unwrap();
        let asset = plan.assets.active_asset(ModelCapability::Translation);

        assert_eq!(asset.manifest().id, ModelAssetId::HunyuanMt7bGguf);
        assert_eq!(
            asset.required_file_path(0),
            PathBuf::from("release-root/models/Hy-MT2-7B-GGUF/Hy-MT2-7B-Q4_K_M.gguf")
        );
    }

    #[test]
    fn runtime_plan_materializes_both_managed_server_specs() {
        let config = AppConfig::from_json_str(include_str!("../../../config.json")).unwrap();
        let plan = NativeProviderPlan::resolve(&config, Path::new("release-root")).unwrap();

        assert_eq!(
            plan.llama_server_path(),
            Path::new("release-root")
                .join("runtime/llama.cpp")
                .join(format!("llama-server{}", std::env::consts::EXE_SUFFIX))
        );
        let (asr, translation) = plan.managed_server_specs(8101, 8102).unwrap();
        let asr = asr.unwrap();
        let translation = translation.unwrap();

        assert_eq!(asr.model_alias, "qwen3-asr");
        assert_eq!(translation.model_alias, "hy-mt2");
        assert_eq!(asr.endpoint.port, 8101);
        assert_eq!(translation.endpoint.port, 8102);
        assert_eq!(translation.context_size, 4_096);
        assert_eq!(translation.parallel_slots, Some(2));
    }

    #[test]
    fn runtime_plan_preserves_explicit_external_server_path() {
        let mut document: serde_json::Value =
            serde_json::from_str(include_str!("../../../config.json")).unwrap();
        document["model_manager"]["llama_server_path"] =
            serde_json::Value::from("/opt/llama.cpp/llama-server");
        let config = AppConfig::from_value(document).unwrap();
        let plan = NativeProviderPlan::resolve(&config, Path::new("/srv/xrtranslate")).unwrap();

        assert_eq!(
            plan.llama_server_path(),
            Path::new("/opt/llama.cpp/llama-server")
        );
    }

    #[test]
    fn unsupported_provider_is_rejected_at_the_runtime_factory_boundary() {
        let mut document: serde_json::Value =
            serde_json::from_str(include_str!("../../../config.json")).unwrap();
        document["translation"]["provider"] = serde_json::Value::from("future-provider");
        document["translation"]["providers"]["future-provider"] = serde_json::json!({
            "url": "http://127.0.0.1:8010/v1/chat/completions",
            "model_asset": "hy-mt2"
        });
        let config = AppConfig::from_value(document).unwrap();

        let error = NativeProviderPlan::resolve(&config, Path::new("release-root")).unwrap_err();

        assert!(error.contains("unsupported translation provider"));
    }

    #[test]
    fn legacy_missing_asset_keys_use_provider_profile_defaults() {
        let mut document: serde_json::Value =
            serde_json::from_str(include_str!("../../../config.json")).unwrap();
        document["asr"]["providers"]["qwen3-gguf"]
            .as_object_mut()
            .unwrap()
            .remove("model_asset");
        document["translation"]["providers"]["hunyuan"]
            .as_object_mut()
            .unwrap()
            .remove("model_asset");
        let config = AppConfig::from_value(document).unwrap();

        let plan = NativeProviderPlan::resolve(&config, Path::new("release-root")).unwrap();

        assert_eq!(
            plan.assets.active_asset(ModelCapability::Asr).manifest().id,
            ModelAssetId::Qwen3AsrGguf
        );
        assert_eq!(
            plan.assets
                .active_asset(ModelCapability::Translation)
                .manifest()
                .id,
            ModelAssetId::HunyuanMtGguf
        );
    }

    #[test]
    fn normalized_provider_selection_drives_assets_and_capabilities_once() {
        let mut document: serde_json::Value =
            serde_json::from_str(include_str!("../../../config.json")).unwrap();
        document["translation"]["provider"] = serde_json::Value::from(" hunyuan ");
        document["translation"]["providers"]["hunyuan"]["model_asset"] =
            serde_json::Value::from("hy-mt2-big");
        let config = AppConfig::from_value(document).unwrap();

        let plan = NativeProviderPlan::resolve(&config, Path::new("release-root")).unwrap();

        assert_eq!(
            plan.assets
                .active_asset(ModelCapability::Translation)
                .manifest()
                .id,
            ModelAssetId::HunyuanMt7bGguf
        );
        assert!(plan.translation_supports_prompt_context());
    }

    #[test]
    fn remote_routes_skip_native_assets_and_use_configured_models() {
        let mut document: serde_json::Value =
            serde_json::from_str(include_str!("../../../config.json")).unwrap();
        document["asr"]["provider"] = serde_json::Value::from("openai-custom");
        document["translation"]["provider"] = serde_json::Value::from("openai-custom");
        let asr_remote = document["asr"]["providers"]["openai"].clone();
        let translation_remote = document["translation"]["providers"]["openai"].clone();
        document["asr"]["providers"]["openai-custom"] = asr_remote;
        document["translation"]["providers"]["openai-custom"] = translation_remote;
        let config = AppConfig::from_value(document).unwrap();
        let plan = NativeProviderPlan::resolve(&config, Path::new("release-root")).unwrap();

        assert!(!plan.uses_local_runtime());
        assert!(plan.check_assets().is_ok());
        assert_eq!(plan.asr_model_alias(), "gpt-4o-audio-preview");
        assert_eq!(plan.translation_model_alias(), "gpt-4o-mini");
        assert!(plan.managed_server_specs(8101, 8102).unwrap().0.is_none());
        assert!(plan.managed_server_specs(8101, 8102).unwrap().1.is_none());
    }

    #[test]
    fn every_catalog_provider_has_a_backend_runtime_profile() {
        for manifest in xrtranslate_assets::DEFAULT_GGUF_MANIFEST {
            let registered = match manifest.capability {
                ModelCapability::Asr => {
                    AsrProfile::registered(manifest.provider, "local").is_some()
                }
                ModelCapability::Translation => {
                    TranslationProfile::registered(manifest.provider, "local").is_some()
                }
            };
            assert!(
                registered,
                "catalog provider {} has no backend runtime profile",
                manifest.provider
            );
        }
    }

    #[test]
    fn generic_pipeline_does_not_name_a_concrete_model_provider() {
        let pipeline = include_str!("pipeline.rs");
        for concrete in ["Qwen3", "Hunyuan", "TranslationProvider"] {
            assert!(
                !pipeline.contains(concrete),
                "pipeline must consume provider-neutral adapters, found {concrete}"
            );
        }
    }
}
