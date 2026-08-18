use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{LlamaServerSpec, SpecValidationError};

/// A fully materialized, but not yet launched, llama-server command.
///
/// This serializable-in-spirit representation is intentionally separate from
/// [`std::process::Command`] so callers can log it and unit tests can validate
/// argument semantics without launching models.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LlamaServerCommand {
    program: PathBuf,
    arguments: Vec<OsString>,
    current_dir: Option<PathBuf>,
    environment: Vec<(OsString, OsString)>,
}

impl LlamaServerCommand {
    /// Builds a command after validating the launch specification.
    pub fn from_spec(spec: &LlamaServerSpec) -> Result<Self, SpecValidationError> {
        spec.validate()?;
        Ok(Self {
            program: spec.executable.clone(),
            arguments: spec.command_args(),
            current_dir: spec.working_directory().map(Path::to_path_buf),
            environment: spec.environment.clone(),
        })
    }

    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

    #[must_use]
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    #[must_use]
    pub fn current_dir(&self) -> Option<&Path> {
        self.current_dir.as_deref()
    }

    #[must_use]
    pub fn environment(&self) -> &[(OsString, OsString)] {
        &self.environment
    }

    /// Converts the immutable command plan into a standard-library process
    /// command immediately before spawning it.
    #[must_use]
    pub fn as_std_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.arguments);
        if let Some(current_dir) = &self.current_dir {
            command.current_dir(current_dir);
        }
        command.envs(self.environment.iter().cloned());
        command
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        net::Ipv4Addr,
        path::{Path, PathBuf},
    };

    use crate::{
        FlashAttention, GpuLayers, LlamaServerCommand, LlamaServerEndpoint, LlamaServerSpec,
        SpecValidationError,
    };

    fn strings(arguments: &[OsString]) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn qwen3_asr_command_includes_multimodal_projection_and_alias() {
        let spec = LlamaServerSpec::qwen3_asr_gguf(
            "C:/llama/llama-server.exe",
            "C:/models/qwen3-asr.gguf",
            "C:/models/qwen3-asr.mmproj.gguf",
        )
        .with_endpoint(LlamaServerEndpoint::new(Ipv4Addr::LOCALHOST.into(), 8101));

        let command = LlamaServerCommand::from_spec(&spec).expect("valid Qwen3 ASR spec");

        assert_eq!(
            command.program(),
            PathBuf::from("C:/llama/llama-server.exe")
        );
        assert_eq!(
            strings(command.arguments()),
            [
                "--host",
                "127.0.0.1",
                "--port",
                "8101",
                "--model",
                "C:/models/qwen3-asr.gguf",
                "--mmproj",
                "C:/models/qwen3-asr.mmproj.gguf",
                "--alias",
                "qwen3-asr",
                "--ctx-size",
                "2048",
                "--n-gpu-layers",
                "99",
            ]
        );
    }

    #[test]
    fn hunyuan_command_uses_translation_profile_without_mmproj() {
        let mut spec =
            LlamaServerSpec::hunyuan_mt_gguf("C:/llama/llama-server.exe", "C:/models/hy-mt2.gguf");
        spec.gpu_layers = GpuLayers::All;
        spec.flash_attention = Some(FlashAttention::Auto);
        spec.extra_args.push(OsString::from("--no-webui"));

        let command = LlamaServerCommand::from_spec(&spec).expect("valid Hunyuan spec");
        let arguments = strings(command.arguments());

        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--alias", "hy-mt2"])
        );
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--ctx-size", "4096"])
        );
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--n-gpu-layers", "-1"])
        );
        assert!(arguments.windows(2).any(|pair| pair == ["--parallel", "4"]));
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--flash-attn", "auto"])
        );
        assert!(!arguments.iter().any(|argument| argument == "--mmproj"));
        assert_eq!(arguments.last(), Some(&"--no-webui".to_owned()));
    }

    #[test]
    fn qwen3_requires_mmproj_before_a_command_is_built() {
        let mut spec = LlamaServerSpec::qwen3_asr_gguf("llama-server", "qwen.gguf", "mmproj.gguf");
        spec.mmproj = None;

        assert_eq!(
            LlamaServerCommand::from_spec(&spec),
            Err(SpecValidationError::MissingMultimodalProjection)
        );
    }

    #[test]
    fn command_keeps_absolute_runtime_and_model_paths_independent_of_cwd() {
        let mut spec = LlamaServerSpec::hunyuan_mt_gguf(
            "/srv/xrtranslate/runtime/llama.cpp/llama-server",
            "/srv/xrtranslate/models/Hy-MT2.gguf",
        );
        spec.working_directory = Some(PathBuf::from("/srv/xrtranslate"));

        let command = LlamaServerCommand::from_spec(&spec).expect("valid absolute spec");

        assert_eq!(
            command.program(),
            Path::new("/srv/xrtranslate/runtime/llama.cpp/llama-server")
        );
        assert_eq!(command.current_dir(), Some(Path::new("/srv/xrtranslate")));
        let arguments = strings(command.arguments());
        assert!(
            arguments
                .windows(2)
                .any(|pair| { pair == ["--model", "/srv/xrtranslate/models/Hy-MT2.gguf"] })
        );
    }
}
