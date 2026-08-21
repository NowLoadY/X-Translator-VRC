//! Onboarding prerequisite validation, resource readiness detection, and step verification.
//!
//! Provides a unified contract for validating that all required services, models,
//! API keys, and runtime binaries are in place before allowing access to the main session.

use std::path::Path;
use xrtranslate_assets::ModelCapability;

use crate::{
    backend::BackendManager,
    model_install::{self, NativeModelTaskManager},
    runtime_install::RuntimeInstaller,
    service_config::ServiceConfigEditor,
};

/// Evaluates whether any required resource (API keys, ASR/MT models, TTS models,
/// or runtime acceleration packages) is missing from the system.
///
/// Returns `true` if any prerequisite is unmet, indicating that the onboarding
/// setup flow must be completed.
#[must_use]
pub fn has_unmet_prerequisites(
    project_root: &Path,
    service_config: &ServiceConfigEditor,
    backend_manager: &BackendManager,
    model_task_manager: &NativeModelTaskManager,
    runtime_installer: &RuntimeInstaller,
) -> bool {
    let requirements = service_config.runtime_requirements();

    // 1. API key requirements
    if requirements.missing_api_key {
        return true;
    }

    // 2. Local ASR and MT model package requirements
    if requirements.llama_cpp {
        let packages = model_install::configured_model_packages(project_root)
            .unwrap_or_default()
            .into_iter()
            .filter(|package| package.capability != ModelCapability::Tts)
            .collect::<Vec<_>>();
        if !packages
            .iter()
            .all(|package| model_task_manager.is_model_present(package.id))
        {
            return true;
        }
    }

    // 3. Optional TTS model package requirements
    let tts_provider = service_config.onboarding_provider_state("tts");
    let selected_audio8 = tts_provider
        .as_ref()
        .is_some_and(|state| state.selected == "audio8");
    if selected_audio8 {
        let package = model_install::model_package_for_provider_config_key(
            project_root,
            "audio8",
            ModelCapability::Tts,
            "audio8-tts-onnx-fp16",
        )
        .ok();
        let installed = package
            .as_ref()
            .is_some_and(|package| model_task_manager.is_model_present(package.id));
        if !installed {
            return true;
        }
    }

    // 4. Runtime binary and acceleration dependencies
    let llama_ready = !requirements.llama_cpp || backend_manager.llama_server_path_is_valid();
    let onnx_ready = !requirements.onnx_tts || runtime_installer.plan_is_ready();
    if !llama_ready || !onnx_ready {
        return true;
    }

    false
}

/// Evaluates unmet prerequisites for a specific step in the onboarding flow.
/// Used by the footer navigation to guard advancing to subsequent steps.
#[must_use]
pub fn evaluate_step_requirement(
    step: usize,
    project_root: &Path,
    service_config: &ServiceConfigEditor,
    backend_manager: &BackendManager,
    model_task_manager: &NativeModelTaskManager,
    runtime_installer: &RuntimeInstaller,
) -> Option<&'static str> {
    match step {
        1 => {
            let requirements = service_config.runtime_requirements();
            if requirements.missing_api_key {
                Some("Configure every required API key to continue.")
            } else {
                None
            }
        }
        2 => None,
        3 => {
            if model_task_manager.is_busy() {
                return Some("Wait for the current model task to finish.");
            }
            if runtime_installer.is_busy() {
                return Some("Wait for runtime preparation to finish.");
            }
            let requirements = service_config.runtime_requirements();
            if requirements.missing_api_key {
                return Some("Configure every required API key to continue.");
            }
            if requirements.llama_cpp {
                let packages: Vec<model_install::NativeModelPackage> =
                    match model_install::configured_model_packages(project_root) {
                        Ok(packages) => packages
                            .into_iter()
                            .filter(|package| package.capability != ModelCapability::Tts)
                            .collect(),
                        Err(_) => return Some("Download every required model package to continue."),
                    };
                if !packages
                    .iter()
                    .all(|package| model_task_manager.is_model_present(package.id))
                {
                    return Some("Download every required model package to continue.");
                }
            }

            let provider = service_config.onboarding_provider_state("tts");
            let selected_audio8 = provider
                .as_ref()
                .is_some_and(|state| state.selected == "audio8");
            if selected_audio8 {
                let package = model_install::model_package_for_provider_config_key(
                    project_root,
                    "audio8",
                    ModelCapability::Tts,
                    "audio8-tts-onnx-fp16",
                )
                .ok();
                let installed = package
                    .as_ref()
                    .is_some_and(|package| model_task_manager.is_model_present(package.id));
                if !installed {
                    return Some("Download the TTS model package to continue, or choose Skip.");
                }
            }

            let llama_ready =
                !requirements.llama_cpp || backend_manager.llama_server_path_is_valid();
            let onnx_ready = !requirements.onnx_tts || runtime_installer.plan_is_ready();
            if !llama_ready || !onnx_ready {
                Some("Choose or install the runtime to continue.")
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Resolves the onboarding state when the application starts.
/// If `first_run` was recorded or any required resource is missing,
/// opens the onboarding flow at page 0 (Welcome page).
#[must_use]
pub fn resolve_startup_onboarding_state(
    is_first_run: bool,
    project_root: &Path,
    service_config: &ServiceConfigEditor,
    backend_manager: &BackendManager,
    model_task_manager: &NativeModelTaskManager,
    runtime_installer: &RuntimeInstaller,
) -> (bool, usize) {
    if is_first_run
        || has_unmet_prerequisites(
            project_root,
            service_config,
            backend_manager,
            model_task_manager,
            runtime_installer,
        )
    {
        (true, 0)
    } else {
        (false, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_always_resolves_to_welcome_page() {
        let root = std::env::temp_dir().join(format!("xrtranslate-onboarding-test-1-{}", std::process::id()));
        let backend_manager = BackendManager::load();
        let service_config = ServiceConfigEditor::load();
        let model_task_manager = NativeModelTaskManager::default();
        let runtime_installer = RuntimeInstaller::default();

        let (first_run, page) = resolve_startup_onboarding_state(
            true,
            &root,
            &service_config,
            &backend_manager,
            &model_task_manager,
            &runtime_installer,
        );
        assert!(first_run);
        assert_eq!(page, 0);
    }
}
