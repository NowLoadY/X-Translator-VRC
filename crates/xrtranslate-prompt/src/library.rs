use serde::{Deserialize, Serialize};

use crate::PromptNodeGraph;
use crate::builtin::BUILTIN_ID;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptTemplateProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub graph: PromptNodeGraph,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptTemplateLibrary {
    pub active_id: String,
    pub profiles: Vec<PromptTemplateProfile>,
}

impl Default for PromptTemplateLibrary {
    fn default() -> Self {
        Self {
            active_id: BUILTIN_ID.into(),
            profiles: vec![builtin_default_profile()],
        }
    }
}

impl PromptTemplateLibrary {
    pub fn normalize(&mut self) {
        self.profiles
            .retain(|profile| !profile.id.trim().is_empty());

        for profile in &mut self.profiles {
            if profile.id == BUILTIN_ID {
                continue;
            }
            if profile.graph.schema_version != PromptNodeGraph::CURRENT_SCHEMA_VERSION
                || profile.graph.nodes.is_empty()
            {
                profile.graph = PromptNodeGraph::builtin_default();
            }
            profile.read_only = false;
        }

        if let Some(profile) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.id == BUILTIN_ID)
        {
            *profile = builtin_default_profile();
        } else {
            self.profiles.insert(0, builtin_default_profile());
        }

        if !self
            .profiles
            .iter()
            .any(|profile| profile.id == self.active_id)
        {
            self.active_id = BUILTIN_ID.into();
        }
    }

    pub fn active_profile(&self) -> Option<&PromptTemplateProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.id == self.active_id)
            .or_else(|| self.profiles.first())
    }

    pub fn active_graph(&self) -> PromptNodeGraph {
        self.active_profile()
            .map(|profile| profile.graph.clone())
            .unwrap_or_default()
    }

    pub fn is_builtin_id(id: &str) -> bool {
        id == BUILTIN_ID
    }

    pub fn editable_copy_of(
        profile: &PromptTemplateProfile,
        id: impl Into<String>,
    ) -> PromptTemplateProfile {
        let mut copy = profile.clone();
        copy.id = id.into();
        copy.name = format!("{} (copy)", profile.name);
        copy.read_only = false;
        copy
    }
}

fn builtin_default_profile() -> PromptTemplateProfile {
    PromptTemplateProfile {
        id: BUILTIN_ID.into(),
        name: "Built-in Default".into(),
        description: "The original provider prompts with configurable translation context.".into(),
        graph: PromptNodeGraph::builtin_default(),
        read_only: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_restores_the_canonical_builtin() {
        let mut library = PromptTemplateLibrary::default();
        library.profiles[0].graph = PromptNodeGraph::empty();
        library.profiles[0].read_only = false;
        library.normalize();
        assert_eq!(library.profiles[0], builtin_default_profile());
    }

    #[test]
    fn template_profiles_do_not_serialize_visual_color_configuration() {
        let value = serde_json::to_value(PromptTemplateLibrary::default()).unwrap();
        assert!(value["profiles"][0].get("accent").is_none());
    }
}
