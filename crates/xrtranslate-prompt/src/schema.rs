use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::TranslationPromptBlock;

fn current_schema_version() -> u16 {
    PromptNodeGraph::CURRENT_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default)]
    pub page: PromptNodePage,
    pub kind: PromptNodeKind,
    #[serde(default)]
    pub position: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptNodeKind {
    Input {
        block: TranslationPromptBlock,
    },
    Variable {
        variable: PromptVariable,
    },
    Compose {
        text: String,
    },
    Switch {
        condition: PromptCondition,
    },
    Request {
        #[serde(default)]
        target: PromptProviderTarget,
        roles: Vec<PromptMessageRole>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PromptNodePage {
    #[default]
    Shared,
    OpenAiCompatible,
    Hunyuan,
}

impl PromptNodePage {
    pub fn for_target(target: PromptProviderTarget) -> Self {
        match target {
            PromptProviderTarget::OpenAiCompatible => Self::OpenAiCompatible,
            PromptProviderTarget::Hunyuan => Self::Hunyuan,
        }
    }

    pub fn is_visible_on(self, target: PromptProviderTarget) -> bool {
        self == Self::Shared || self == Self::for_target(target)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptVariable {
    SourceLanguage,
    TargetLanguage,
    CurrentInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptCondition {
    SourceIsAuto,
    HasReferenceContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PromptProviderTarget {
    Hunyuan,
    #[default]
    OpenAiCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PromptMessageRole {
    System,
    #[default]
    User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptLink {
    pub from: String,
    pub to: String,
    pub input: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptNodeGraph {
    #[serde(default = "current_schema_version")]
    pub schema_version: u16,
    #[serde(default)]
    pub nodes: Vec<PromptNode>,
    #[serde(default)]
    pub links: Vec<PromptLink>,
    #[serde(default)]
    pub layout_version: u16,
}

impl PromptNode {
    pub fn layout_height(&self) -> f32 {
        let height = match &self.kind {
            PromptNodeKind::Input {
                block: TranslationPromptBlock::CustomText { text },
            } => content_node_height(text, 122.0, 31),
            PromptNodeKind::Compose { text } => {
                let inputs = crate::compose_input_indexes(text)
                    .map(|inputs| inputs.len())
                    .unwrap_or_default();
                content_node_height(text, 142.0, 43).max(72.0 + inputs as f32 * 25.0)
            }
            PromptNodeKind::Switch { .. } => 124.0,
            PromptNodeKind::Request { roles, .. } => 88.0 + roles.len() as f32 * 25.0,
            _ => 84.0,
        };
        height.max(156.0)
    }
}

fn content_node_height(text: &str, minimum: f32, wrap_chars: usize) -> f32 {
    let lines = text
        .lines()
        .map(|line| {
            let characters = line.chars().count().max(1);
            characters.div_ceil(wrap_chars)
        })
        .sum::<usize>()
        .max(1);
    minimum.max(54.0 + lines as f32 * 13.0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptGraphError {
    message: String,
}

impl PromptGraphError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for PromptGraphError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PromptGraphError {}

impl PromptNodeGraph {
    pub const CURRENT_SCHEMA_VERSION: u16 = 6;
    pub const CURRENT_LAYOUT_VERSION: u16 = 7;
    pub const MAX_COMPOSE_INPUT_INDEX: u8 = 9;

    pub fn empty() -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            nodes: Vec::new(),
            links: Vec::new(),
            layout_version: 0,
        }
    }

    pub fn fingerprint(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
        format!("{hash:016x}")
    }

    pub fn add_input(
        &mut self,
        page: PromptNodePage,
        block: TranslationPromptBlock,
        position: [f32; 2],
    ) -> String {
        self.add_node(page, PromptNodeKind::Input { block }, position)
    }

    pub fn add_variable(
        &mut self,
        page: PromptNodePage,
        variable: PromptVariable,
        position: [f32; 2],
    ) -> String {
        self.add_node(page, PromptNodeKind::Variable { variable }, position)
    }

    pub fn add_switch(
        &mut self,
        page: PromptNodePage,
        condition: PromptCondition,
        position: [f32; 2],
    ) -> String {
        self.add_node(page, PromptNodeKind::Switch { condition }, position)
    }

    pub fn add_compose(
        &mut self,
        page: PromptNodePage,
        text: String,
        position: [f32; 2],
    ) -> String {
        self.add_node(page, PromptNodeKind::Compose { text }, position)
    }

    pub fn add_request(
        &mut self,
        target: PromptProviderTarget,
        roles: Vec<PromptMessageRole>,
        position: [f32; 2],
    ) -> String {
        self.add_node(
            PromptNodePage::for_target(target),
            PromptNodeKind::Request { target, roles },
            position,
        )
    }

    fn add_node(
        &mut self,
        page: PromptNodePage,
        kind: PromptNodeKind,
        position: [f32; 2],
    ) -> String {
        let prefix = match kind {
            PromptNodeKind::Input { .. } => "input",
            PromptNodeKind::Variable { .. } => "variable",
            PromptNodeKind::Compose { .. } => "compose",
            PromptNodeKind::Switch { .. } => "switch",
            PromptNodeKind::Request { .. } => "request",
        };
        let id = self.next_id(prefix);
        self.nodes.push(PromptNode {
            id: id.clone(),
            label: default_node_label(&kind),
            page,
            kind,
            position,
        });
        id
    }

    pub fn remove_node(&mut self, id: &str) {
        self.nodes.retain(|node| node.id != id);
        self.links.retain(|link| link.from != id && link.to != id);
    }

    pub fn connect(&mut self, from: &str, to: &str, input: u8) -> bool {
        let pages_match = self
            .nodes
            .iter()
            .find(|node| node.id == from)
            .zip(self.nodes.iter().find(|node| node.id == to))
            .is_some_and(|(source, target)| pages_can_connect(source.page, target.page));
        if from == to
            || !self.nodes.iter().any(|node| node.id == from)
            || !self.nodes.iter().any(|node| node.id == to)
            || !pages_match
            || !self.accepts_input(to, input)
            || self.reaches(to, from)
        {
            return false;
        }
        if let Some(PromptNode {
            kind: PromptNodeKind::Compose { text },
            ..
        }) = self.nodes.iter_mut().find(|node| node.id == to)
        {
            let is_declared =
                crate::compose_input_indexes(text).is_ok_and(|inputs| inputs.contains(&input));
            if !is_declared {
                append_compose_input(text, input);
            }
        }
        self.links
            .retain(|link| !(link.to == to && link.input == input));
        self.links.push(PromptLink {
            from: from.into(),
            to: to.into(),
            input,
        });
        true
    }

    pub fn compose_input_socket_indexes(&self, id: &str) -> Vec<u8> {
        let Some(PromptNode {
            kind: PromptNodeKind::Compose { text },
            ..
        }) = self.nodes.iter().find(|node| node.id == id)
        else {
            return Vec::new();
        };
        let mut inputs = crate::compose_input_indexes(text).unwrap_or_default();
        for input in self
            .links
            .iter()
            .filter(|link| link.to == id)
            .map(|link| link.input)
        {
            if !inputs.contains(&input) {
                inputs.push(input);
            }
        }
        inputs.sort_unstable();
        inputs.dedup();

        let has_available_input = inputs.iter().any(|input| {
            !self
                .links
                .iter()
                .any(|link| link.to == id && link.input == *input)
        });
        if !has_available_input {
            if let Some(spare) =
                (0..=Self::MAX_COMPOSE_INPUT_INDEX).find(|input| !inputs.contains(input))
            {
                inputs.push(spare);
                inputs.sort_unstable();
            }
        }
        inputs
    }

    pub fn validate_for_activation(&self) -> Result<(), PromptGraphError> {
        if self.schema_version != Self::CURRENT_SCHEMA_VERSION {
            return Err(PromptGraphError::new(format!(
                "prompt graph schema {} must be migrated to {}",
                self.schema_version,
                Self::CURRENT_SCHEMA_VERSION
            )));
        }
        let mut ids = HashSet::new();
        for node in &self.nodes {
            if node.id.trim().is_empty() {
                return Err(PromptGraphError::new("prompt node IDs cannot be empty"));
            }
            if !ids.insert(node.id.as_str()) {
                return Err(PromptGraphError::new(format!(
                    "duplicate prompt node ID: {}",
                    node.id
                )));
            }
            if let PromptNodeKind::Compose { text } = &node.kind {
                crate::compose_input_indexes(text).map_err(|error| {
                    PromptGraphError::new(format!("compose node {}: {error}", node.id))
                })?;
            }
            if let PromptNodeKind::Request { target, roles } = &node.kind {
                if roles.is_empty() || roles.len() > u8::MAX as usize {
                    return Err(PromptGraphError::new(format!(
                        "provider request {} must contain at least one message",
                        node.id
                    )));
                }
                if node.page != PromptNodePage::for_target(*target) {
                    return Err(PromptGraphError::new(format!(
                        "provider request {} is assigned to the wrong page",
                        node.id
                    )));
                }
            }
        }
        let nodes = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect::<HashMap<_, _>>();
        let mut sockets = HashSet::new();
        for link in &self.links {
            if !nodes.contains_key(link.from.as_str()) || !nodes.contains_key(link.to.as_str()) {
                return Err(PromptGraphError::new(
                    "prompt link references a missing node",
                ));
            }
            if link.from == link.to || !self.has_declared_input(&link.to, link.input) {
                return Err(PromptGraphError::new("prompt link uses an invalid socket"));
            }
            if !pages_can_connect(nodes[link.from.as_str()].page, nodes[link.to.as_str()].page) {
                return Err(PromptGraphError::new(
                    "prompt link crosses incompatible provider pages",
                ));
            }
            if !sockets.insert((link.to.as_str(), link.input)) {
                return Err(PromptGraphError::new(
                    "prompt input socket has multiple links",
                ));
            }
            if self.reaches(&link.to, &link.from) {
                return Err(PromptGraphError::new("prompt graph contains a cycle"));
            }
        }
        for node in &self.nodes {
            let required_inputs = match &node.kind {
                PromptNodeKind::Compose { text } => crate::compose_input_indexes(text)?,
                PromptNodeKind::Request { roles, .. } => (0..roles.len() as u8).collect(),
                _ => continue,
            };
            for input in required_inputs {
                if !self
                    .links
                    .iter()
                    .any(|link| link.to == node.id && link.input == input)
                {
                    return Err(PromptGraphError::new(format!(
                        "node {} input {input} is not connected",
                        node.id
                    )));
                }
            }
        }
        for target in [
            PromptProviderTarget::Hunyuan,
            PromptProviderTarget::OpenAiCompatible,
        ] {
            let outputs = self
                .nodes
                .iter()
                .filter(|node| {
                    matches!(node.kind, PromptNodeKind::Request { target: value, .. } if value == target)
                })
                .collect::<Vec<_>>();
            if outputs.is_empty() {
                return Err(PromptGraphError::new(format!(
                    "prompt graph has no {target:?} provider request"
                )));
            }
            if outputs.len() != 1 {
                return Err(PromptGraphError::new(format!(
                    "prompt graph must have exactly one {target:?} provider request"
                )));
            }
            if !outputs
                .iter()
                .any(|output| self.has_variable_ancestor(&output.id, PromptVariable::CurrentInput))
            {
                return Err(PromptGraphError::new(format!(
                    "the {target:?} prompt must include Current Input"
                )));
            }
        }
        Ok(())
    }

    pub fn auto_layout(&mut self) {
        let mut layers = self
            .nodes
            .iter()
            .map(|node| (node.id.clone(), 0_usize))
            .collect::<HashMap<_, _>>();
        for _ in 0..self.nodes.len() {
            let mut changed = false;
            for link in &self.links {
                let Some(from_layer) = layers.get(&link.from).copied() else {
                    continue;
                };
                let next = from_layer.saturating_add(1);
                let entry = layers.entry(link.to.clone()).or_default();
                if *entry < next {
                    *entry = next;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut shared = nodes_by_layer(&self.nodes, &layers, PromptNodePage::Shared);
        let mut shared_bottoms = HashMap::new();
        for (layer, ids) in &mut shared {
            ids.sort();
            let bottom = position_column(&mut self.nodes, *layer, ids, 40.0);
            shared_bottoms.insert(*layer, bottom);
        }

        // Provider pages are mutually exclusive canvas views, so their nodes can
        // reuse the same vertical space after any Shared nodes in that column.
        for page in [PromptNodePage::OpenAiCompatible, PromptNodePage::Hunyuan] {
            let mut grouped = nodes_by_layer(&self.nodes, &layers, page);
            for (layer, ids) in &mut grouped {
                ids.sort();
                let y = shared_bottoms.get(layer).copied().unwrap_or(40.0);
                position_column(&mut self.nodes, *layer, ids, y);
            }
        }
        self.layout_version = Self::CURRENT_LAYOUT_VERSION;
    }

    pub(crate) fn accepts_input(&self, id: &str, input: u8) -> bool {
        self.nodes
            .iter()
            .find(|node| node.id == id)
            .is_some_and(|node| match node.kind {
                PromptNodeKind::Compose { .. } => {
                    self.compose_input_socket_indexes(id).contains(&input)
                }
                PromptNodeKind::Switch { .. } => input < 2,
                PromptNodeKind::Request { ref roles, .. } => usize::from(input) < roles.len(),
                _ => false,
            })
    }

    fn has_declared_input(&self, id: &str, input: u8) -> bool {
        self.nodes
            .iter()
            .find(|node| node.id == id)
            .is_some_and(|node| match node.kind {
                PromptNodeKind::Compose { ref text } => {
                    crate::compose_input_indexes(text).is_ok_and(|inputs| inputs.contains(&input))
                }
                PromptNodeKind::Switch { .. } => input < 2,
                PromptNodeKind::Request { ref roles, .. } => usize::from(input) < roles.len(),
                _ => false,
            })
    }

    pub(crate) fn reaches(&self, start: &str, target: &str) -> bool {
        let mut pending = vec![start.to_owned()];
        let mut visited = HashSet::new();
        while let Some(current) = pending.pop() {
            if current == target {
                return true;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            pending.extend(
                self.links
                    .iter()
                    .filter(|link| link.from == current)
                    .map(|link| link.to.clone()),
            );
        }
        false
    }

    fn has_variable_ancestor(&self, output: &str, variable: PromptVariable) -> bool {
        let mut pending = vec![output];
        let mut visited = HashSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            if self.nodes.iter().find(|node| node.id == current).is_some_and(
                |node| matches!(node.kind, PromptNodeKind::Variable { variable: value } if value == variable),
            ) {
                return true;
            }
            pending.extend(
                self.links
                    .iter()
                    .filter(|link| link.to == current)
                    .map(|link| link.from.as_str()),
            );
        }
        false
    }

    fn next_id(&self, prefix: &str) -> String {
        let mut index = self.nodes.len() + 1;
        loop {
            let candidate = format!("{prefix}-{index}");
            if !self.nodes.iter().any(|node| node.id == candidate) {
                return candidate;
            }
            index += 1;
        }
    }
}

fn append_compose_input(text: &mut String, input: u8) {
    if !text.is_empty() {
        if text.ends_with("\n\n") {
            // The existing paragraph boundary already separates the new input.
        } else if text.ends_with('\n') {
            text.push('\n');
        } else {
            text.push_str("\n\n");
        }
    }
    text.push_str(&format!("{{{input}}}"));
}

fn pages_can_connect(source: PromptNodePage, target: PromptNodePage) -> bool {
    source == PromptNodePage::Shared || source == target
}

pub(crate) fn default_node_label(kind: &PromptNodeKind) -> String {
    match kind {
        PromptNodeKind::Input { block } => block.preview_name().into(),
        PromptNodeKind::Variable { variable } => match variable {
            PromptVariable::SourceLanguage => "SOURCE LANGUAGE".into(),
            PromptVariable::TargetLanguage => "TARGET LANGUAGE".into(),
            PromptVariable::CurrentInput => "CURRENT INPUT".into(),
        },
        PromptNodeKind::Compose { .. } => "COMPOSE TEXT".into(),
        PromptNodeKind::Switch { condition } => match condition {
            PromptCondition::SourceIsAuto => "SELECT SOURCE MODE".into(),
            PromptCondition::HasReferenceContext => "SELECT CONTEXT MODE".into(),
        },
        PromptNodeKind::Request { target, .. } => {
            let provider = match target {
                PromptProviderTarget::Hunyuan => "HUNYUAN",
                PromptProviderTarget::OpenAiCompatible => "OPENAI",
            };
            format!("{provider} REQUEST")
        }
    }
}

fn nodes_by_layer(
    nodes: &[PromptNode],
    layers: &HashMap<String, usize>,
    page: PromptNodePage,
) -> BTreeMap<usize, Vec<String>> {
    let mut grouped = BTreeMap::new();
    for node in nodes.iter().filter(|node| node.page == page) {
        grouped
            .entry(layers.get(&node.id).copied().unwrap_or_default())
            .or_insert_with(Vec::new)
            .push(node.id.clone());
    }
    grouped
}

fn position_column(nodes: &mut [PromptNode], layer: usize, ids: &[String], mut y: f32) -> f32 {
    for id in ids {
        if let Some(node) = nodes.iter_mut().find(|node| node.id == *id) {
            node.position = [48.0 + layer as f32 * 600.0, y];
            y += node.layout_height() + 32.0;
        }
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_nodes_feed_both_pages_but_provider_nodes_stay_on_their_page() {
        let mut graph = PromptNodeGraph::empty();
        let shared = graph.add_variable(
            PromptNodePage::Shared,
            PromptVariable::CurrentInput,
            [0.0, 0.0],
        );
        let openai =
            graph.add_compose(PromptNodePage::OpenAiCompatible, "{0}".into(), [300.0, 0.0]);
        let hunyuan = graph.add_compose(PromptNodePage::Hunyuan, "{0}".into(), [300.0, 200.0]);
        let shared_target = graph.add_compose(PromptNodePage::Shared, "{0}".into(), [300.0, 400.0]);

        assert!(graph.connect(&shared, &openai, 0));
        assert!(graph.connect(&shared, &hunyuan, 0));
        assert!(!graph.connect(&openai, &hunyuan, 0));
        assert!(!graph.connect(&openai, &shared_target, 0));
    }

    #[test]
    fn request_serialization_names_the_node_by_its_actual_role() {
        let graph = PromptNodeGraph::builtin_default();
        let request = graph
            .nodes
            .iter()
            .find(|node| node.id == "openai-request")
            .unwrap();
        let value = serde_json::to_value(request).unwrap();

        assert_eq!(value["kind"]["type"], "request");
        assert_eq!(value["kind"]["roles"][0], "system");
        assert_eq!(value["kind"]["roles"][1], "user");
    }

    #[test]
    fn links_serialize_only_data_flow() {
        let graph = PromptNodeGraph::builtin_default();
        let value = serde_json::to_value(graph).unwrap();

        for link in value["links"].as_array().unwrap() {
            assert!(link.get("from").is_some());
            assert!(link.get("to").is_some());
            assert!(link.get("input").is_some());
            assert!(link.get("newline").is_none());
        }
    }

    #[test]
    fn graph_fingerprint_is_stable_and_covers_graph_content() {
        let graph = PromptNodeGraph::builtin_default();
        let mut changed = graph.clone();
        let PromptNodeKind::Compose { text } = &mut changed
            .nodes
            .iter_mut()
            .find(|node| node.id == "reference-handling-rules")
            .unwrap()
            .kind
        else {
            panic!("expected compose node");
        };
        text.push('!');

        assert_eq!(graph.fingerprint(), graph.clone().fingerprint());
        assert_ne!(graph.fingerprint(), changed.fingerprint());
    }

    #[test]
    fn compose_inputs_grow_with_one_spare_until_ten_are_connected() {
        let mut graph = PromptNodeGraph::empty();
        let source = graph.add_variable(
            PromptNodePage::Shared,
            PromptVariable::CurrentInput,
            [0.0, 0.0],
        );
        let compose = graph.add_compose(PromptNodePage::Shared, "Instruction".into(), [300.0, 0.0]);

        assert_eq!(graph.compose_input_socket_indexes(&compose), vec![0]);
        for input in 0..=PromptNodeGraph::MAX_COMPOSE_INPUT_INDEX {
            assert!(graph.connect(&source, &compose, input));
            let sockets = graph.compose_input_socket_indexes(&compose);
            let expected_count = usize::from(input + 1)
                + usize::from(input < PromptNodeGraph::MAX_COMPOSE_INPUT_INDEX);
            assert_eq!(sockets.len(), expected_count);
        }
        assert!(!graph.connect(&source, &compose, 10));
        assert_eq!(
            graph.compose_input_socket_indexes(&compose),
            (0..=PromptNodeGraph::MAX_COMPOSE_INPUT_INDEX).collect::<Vec<_>>()
        );
        assert_eq!(
            graph
                .nodes
                .iter()
                .find(|node| node.id == compose)
                .and_then(|node| match &node.kind {
                    PromptNodeKind::Compose { text } => Some(text.as_str()),
                    _ => None,
                }),
            Some(
                "Instruction\n\n{0}\n\n{1}\n\n{2}\n\n{3}\n\n{4}\n\n{5}\n\n{6}\n\n{7}\n\n{8}\n\n{9}"
            )
        );
    }

    #[test]
    fn an_unconnected_declared_compose_input_is_the_spare() {
        let mut graph = PromptNodeGraph::empty();
        let source = graph.add_variable(
            PromptNodePage::Shared,
            PromptVariable::CurrentInput,
            [0.0, 0.0],
        );
        let compose = graph.add_compose(PromptNodePage::Shared, "{0}".into(), [300.0, 0.0]);

        assert_eq!(graph.compose_input_socket_indexes(&compose), vec![0]);
        assert!(graph.connect(&source, &compose, 0));
        assert_eq!(graph.compose_input_socket_indexes(&compose), vec![0, 1]);
        graph.links.clear();
        assert_eq!(graph.compose_input_socket_indexes(&compose), vec![0]);
    }

    #[test]
    fn validation_rejects_a_compose_link_without_a_text_placeholder() {
        let mut graph = PromptNodeGraph::builtin_default();
        graph.links.push(PromptLink {
            from: "current-input".into(),
            to: "reference-handling-rules".into(),
            input: 0,
        });

        assert_eq!(
            graph.validate_for_activation().unwrap_err().to_string(),
            "prompt link uses an invalid socket"
        );
    }
}
