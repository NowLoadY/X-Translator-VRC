use crate::{
    PromptCondition, PromptLink, PromptMessageRole, PromptNode, PromptNodeGraph, PromptNodeKind,
    PromptNodePage, PromptProviderTarget, PromptVariable, TranslationPromptBlock,
};

pub(crate) const BUILTIN_ID: &str = "builtin-default";
pub(crate) const EXPLICIT_REFERENCE_CONTEXT_INSTRUCTION: &str = concat!(
    "Use the provided context to translate the current {0} input into 100% natural, idiomatic {1}.\n\n",
    "First understand the actual meaning of the current {0} input, then use the context to determine references, tone, speaker relationships, and intended meaning. Do not translate word-for-word, preserve {0} sentence structure, or produce translationese.\n\n",
    "The translation should sound like something a native {1} speaker would naturally say or type in Discord, QQ, WeChat, gaming chats, and everyday conversations. Naturally adjust {1} word order, sentence structure, and wording according to the context.\n\n",
    "Preserve the original meaning, tone, emotion, attitude, personality, and level of formality. Do not unnecessarily add, remove, or change the original meaning.\n\n",
    "Use vocabulary and expressions commonly and naturally used in {1}. Do not use non-standard phrasing when a natural {1} expression exists.\n\n",
    "When encountering slang, idioms, internet expressions, or conversational {0}, convey the intended meaning using an expression that {1} speakers would naturally understand and use rather than translating it literally.\n\n",
    "Translate only the current {0} input. Do not translate, repeat, summarize, or explain the context. Unless explicitly requested otherwise, output only the final {1} translation."
);
pub(crate) const AUTO_REFERENCE_CONTEXT_INSTRUCTION: &str = concat!(
    "Use the provided context to translate the current input into the other language among {0} into 100% natural, idiomatic expression.\n\n",
    "First understand the actual meaning of the current input, then use the context to determine references, tone, speaker relationships, and intended meaning. Do not translate word-for-word, preserve original sentence structure, or produce translationese.\n\n",
    "The translation should sound like something a native speaker of the target language would naturally say or type in Discord, QQ, WeChat, gaming chats, and everyday conversations. Naturally adjust target-language word order, sentence structure, and wording according to the context.\n\n",
    "Preserve the original meaning, tone, emotion, attitude, personality, and level of formality. Do not unnecessarily add, remove, or change the original meaning.\n\n",
    "Use vocabulary and expressions commonly and naturally used in the target language. Do not use non-standard phrasing when a natural expression exists.\n\n",
    "When encountering slang, idioms, internet expressions, or conversational speech, convey the intended meaning using an expression that native speakers of the target language would naturally understand and use rather than translating it literally.\n\n",
    "Translate only the current input. Do not translate, repeat, summarize, or explain the context. Unless explicitly requested otherwise, output only the final translation."
);

impl PromptNodeGraph {
    pub fn builtin_default() -> Self {
        let mut builder = GraphBuilder::default();

        builder.variable("source-language", PromptVariable::SourceLanguage);
        builder.variable("target-language", PromptVariable::TargetLanguage);
        builder.variable("current-input", PromptVariable::CurrentInput);

        for (id, block) in [
            (
                "context-language-order",
                TranslationPromptBlock::LanguageOrder,
            ),
            ("context-terminology", TranslationPromptBlock::Terminology),
            (
                "context-recent-turns",
                TranslationPromptBlock::RecentTurns { limit: None },
            ),
            (
                "context-previous-revision",
                TranslationPromptBlock::PreviousRevision,
            ),
            (
                "context-surrounding-source",
                TranslationPromptBlock::SurroundingSource,
            ),
        ] {
            builder.node(id, PromptNodeKind::Input { block });
        }

        builder.compose(
            "reference-sections",
            "ASSEMBLE CONTEXT SECTIONS",
            "{0}\n\n{1}\n\n{2}\n\n{3}\n\n{4}",
            &[
                "context-language-order",
                "context-terminology",
                "context-recent-turns",
                "context-previous-revision",
                "context-surrounding-source",
            ],
        );
        builder.compose(
            "reference-context",
            "TRANSLATION CONTEXT",
            "# Translation Context\n\n{0}",
            &["reference-sections"],
        );
        builder.compose(
            "reference-explicit-rules",
            "EXPLICIT REFERENCE RULES",
            EXPLICIT_REFERENCE_CONTEXT_INSTRUCTION,
            &["source-language", "target-language"],
        );
        builder.compose(
            "reference-auto-rules",
            "AUTO REFERENCE RULES",
            AUTO_REFERENCE_CONTEXT_INSTRUCTION,
            &["target-language"],
        );
        builder.switch(
            "reference-handling-rules",
            "SELECT REFERENCE RULES",
            PromptCondition::SourceIsAuto,
            "reference-explicit-rules",
            "reference-auto-rules",
        );

        builder.compose(
            "openai-explicit-instruction",
            "EXPLICIT SOURCE INSTRUCTION",
            "You are a real-time speech translator. If input is already {0}, output it unchanged. Otherwise translate it into natural, fluent {0}. Output only the translation.",
            &["target-language"],
        );
        builder.compose(
            "openai-auto-instruction",
            "AUTO SOURCE INSTRUCTION",
            "You are a real-time speech translator. The input language is one of the following: {0}. Translate it into the OTHER language from that list. Output only the translation.",
            &["target-language"],
        );
        builder.switch(
            "openai-instruction",
            "SELECT SOURCE INSTRUCTION",
            PromptCondition::SourceIsAuto,
            "openai-explicit-instruction",
            "openai-auto-instruction",
        );

        builder.compose(
            "openai-system-with-context",
            "SYSTEM PROMPT WITH CONTEXT",
            "{0}\n\n{1}\n{2}",
            &[
                "openai-instruction",
                "reference-handling-rules",
                "reference-context",
            ],
        );
        builder.switch(
            "openai-system",
            "SELECT SYSTEM PROMPT",
            PromptCondition::HasReferenceContext,
            "openai-instruction",
            "openai-system-with-context",
        );
        builder.compose(
            "openai-explicit-user",
            "EXPLICIT SOURCE MESSAGE",
            "Source language: {0}\nCurrent input:\n{1}",
            &["source-language", "current-input"],
        );
        builder.compose(
            "openai-auto-user",
            "AUTO SOURCE MESSAGE",
            "Current input:\n{0}",
            &["current-input"],
        );
        builder.switch(
            "openai-user",
            "SELECT USER MESSAGE",
            PromptCondition::SourceIsAuto,
            "openai-explicit-user",
            "openai-auto-user",
        );
        builder.output(
            "openai-request",
            PromptProviderTarget::OpenAiCompatible,
            &[
                (PromptMessageRole::System, "openai-system"),
                (PromptMessageRole::User, "openai-user"),
            ],
        );

        builder.compose(
            "hunyuan-explicit-instruction",
            "EXPLICIT SOURCE INSTRUCTION",
            "Translate the following {0} text into natural {1}. Output only the translation, do not output the prompt; do not add explanations.",
            &["source-language", "target-language"],
        );
        builder.compose(
            "hunyuan-auto-instruction",
            "AUTO SOURCE INSTRUCTION",
            "Translate the following text into the other language among {0}. Output only the translation; do not add explanations.",
            &["target-language"],
        );
        builder.switch(
            "hunyuan-instruction",
            "SELECT SOURCE INSTRUCTION",
            PromptCondition::SourceIsAuto,
            "hunyuan-explicit-instruction",
            "hunyuan-auto-instruction",
        );
        builder.compose(
            "hunyuan-with-context",
            "USER PROMPT WITH CONTEXT",
            "{0}\n\n{1}\n\n--- BEGIN REFERENCE CONTEXT ---\n{2}\n--- END REFERENCE CONTEXT ---\n\nCurrent input:\n{3}",
            &[
                "hunyuan-instruction",
                "reference-handling-rules",
                "reference-context",
                "current-input",
            ],
        );
        builder.compose(
            "hunyuan-without-context",
            "USER PROMPT WITHOUT CONTEXT",
            "{0}\n\n{1}",
            &["hunyuan-instruction", "current-input"],
        );
        builder.switch(
            "hunyuan-user",
            "SELECT USER PROMPT",
            PromptCondition::HasReferenceContext,
            "hunyuan-without-context",
            "hunyuan-with-context",
        );
        builder.output(
            "hunyuan-request",
            PromptProviderTarget::Hunyuan,
            &[(PromptMessageRole::User, "hunyuan-user")],
        );

        let mut graph = builder.finish();
        graph.auto_layout();
        graph
    }
}

impl Default for PromptNodeGraph {
    fn default() -> Self {
        Self::builtin_default()
    }
}

#[derive(Default)]
struct GraphBuilder {
    nodes: Vec<PromptNode>,
    links: Vec<PromptLink>,
}

impl GraphBuilder {
    fn node(&mut self, id: &str, kind: PromptNodeKind) {
        let label = crate::schema::default_node_label(&kind);
        self.labeled_node(id, &label, kind);
    }

    fn labeled_node(&mut self, id: &str, label: &str, kind: PromptNodeKind) {
        self.nodes.push(PromptNode {
            id: id.into(),
            label: label.into(),
            page: node_page(id),
            kind,
            position: [0.0, 0.0],
        });
    }

    fn variable(&mut self, id: &str, variable: PromptVariable) {
        self.node(id, PromptNodeKind::Variable { variable });
    }

    fn compose(&mut self, id: &str, label: &str, text: &str, sources: &[&str]) {
        self.labeled_node(id, label, PromptNodeKind::Compose { text: text.into() });
        for (input, source) in sources.iter().enumerate() {
            self.link(source, id, input as u8);
        }
    }

    fn switch(
        &mut self,
        id: &str,
        label: &str,
        condition: PromptCondition,
        false_source: &str,
        true_source: &str,
    ) {
        self.labeled_node(id, label, PromptNodeKind::Switch { condition });
        self.link(false_source, id, 0);
        self.link(true_source, id, 1);
    }

    fn output(
        &mut self,
        id: &str,
        target: PromptProviderTarget,
        messages: &[(PromptMessageRole, &str)],
    ) {
        self.node(
            id,
            PromptNodeKind::Request {
                target,
                roles: messages.iter().map(|(role, _)| *role).collect(),
            },
        );
        for (input, (_, source)) in messages.iter().enumerate() {
            self.link(source, id, input as u8);
        }
    }

    fn link(&mut self, from: &str, to: &str, input: u8) {
        self.links.push(PromptLink {
            from: from.into(),
            to: to.into(),
            input,
        });
    }

    fn finish(self) -> PromptNodeGraph {
        PromptNodeGraph {
            schema_version: PromptNodeGraph::CURRENT_SCHEMA_VERSION,
            nodes: self.nodes,
            links: self.links,
            layout_version: 0,
        }
    }
}

fn node_page(id: &str) -> PromptNodePage {
    if id.starts_with("openai-") {
        PromptNodePage::OpenAiCompatible
    } else if id.starts_with("hunyuan-") {
        PromptNodePage::Hunyuan
    } else {
        PromptNodePage::Shared
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PromptMessage, PromptTurn, SurroundingSource, TranslationPromptContext};

    fn context() -> TranslationPromptContext {
        TranslationPromptContext {
            language_order: vec!["en".into(), "zh".into()],
            terminology_rows: vec!["天使,Mercy".into()],
            recent_turns: vec![PromptTurn {
                turn_id: Some("previous".into()),
                speaker_id: "speaker-01".into(),
                source_language: "en".into(),
                target_language: "zh".into(),
                source_text: "We changed the plan.".into(),
                translated_text: "我们改计划了。".into(),
            }],
            previous_revision: Some(PromptTurn {
                turn_id: Some("current".into()),
                speaker_id: "speaker-01".into(),
                source_language: "en".into(),
                target_language: "zh".into(),
                source_text: "The current window.".into(),
                translated_text: "当前窗口。".into(),
            }),
            surrounding_source: Some(SurroundingSource {
                speaker_id: "speaker-01".into(),
                source_language: "en".into(),
                before: "Before it.".into(),
                after: "After it.".into(),
            }),
        }
    }

    fn reference() -> String {
        "# Translation Context\n\n\
## Language Order\n\n\
en,zh\n\n\
## Terminology\n\n\
天使,Mercy\n\n\
## Recent Bilingual History\n\n\
speaker-01 en: We changed the plan.\n\
speaker-01 zh: 我们改计划了。\n\n\
## Previous Revision of Current Speech\n\n\
speaker-01 en: The current window.\n\
speaker-01 zh: 当前窗口。\n\n\
## Current Utterance Context (context only; do not translate)\n\n\
Before current input: speaker-01 en / Before it.\n\
After current input: speaker-01 en / After it."
            .into()
    }

    fn explicit_reference_rules_rendered(source: &str, target: &str) -> String {
        EXPLICIT_REFERENCE_CONTEXT_INSTRUCTION
            .replace("{0}", source)
            .replace("{1}", target)
    }

    fn auto_reference_rules_rendered(target: &str) -> String {
        AUTO_REFERENCE_CONTEXT_INSTRUCTION
            .replace("{0}", target)
    }

    #[test]
    fn reference_handling_rules_are_canonical() {
        assert_eq!(
            EXPLICIT_REFERENCE_CONTEXT_INSTRUCTION,
            concat!(
                "Use the provided context to translate the current {0} input into 100% natural, idiomatic {1}.\n\n",
                "First understand the actual meaning of the current {0} input, then use the context to determine references, tone, speaker relationships, and intended meaning. Do not translate word-for-word, preserve {0} sentence structure, or produce translationese.\n\n",
                "The translation should sound like something a native {1} speaker would naturally say or type in Discord, QQ, WeChat, gaming chats, and everyday conversations. Naturally adjust {1} word order, sentence structure, and wording according to the context.\n\n",
                "Preserve the original meaning, tone, emotion, attitude, personality, and level of formality. Do not unnecessarily add, remove, or change the original meaning.\n\n",
                "Use vocabulary and expressions commonly and naturally used in {1}. Do not use non-standard phrasing when a natural {1} expression exists.\n\n",
                "When encountering slang, idioms, internet expressions, or conversational {0}, convey the intended meaning using an expression that {1} speakers would naturally understand and use rather than translating it literally.\n\n",
                "Translate only the current {0} input. Do not translate, repeat, summarize, or explain the context. Unless explicitly requested otherwise, output only the final {1} translation."
            )
        );
        assert_eq!(
            AUTO_REFERENCE_CONTEXT_INSTRUCTION,
            concat!(
                "Use the provided context to translate the current input into the other language among {0} into 100% natural, idiomatic expression.\n\n",
                "First understand the actual meaning of the current input, then use the context to determine references, tone, speaker relationships, and intended meaning. Do not translate word-for-word, preserve original sentence structure, or produce translationese.\n\n",
                "The translation should sound like something a native speaker of the target language would naturally say or type in Discord, QQ, WeChat, gaming chats, and everyday conversations. Naturally adjust target-language word order, sentence structure, and wording according to the context.\n\n",
                "Preserve the original meaning, tone, emotion, attitude, personality, and level of formality. Do not unnecessarily add, remove, or change the original meaning.\n\n",
                "Use vocabulary and expressions commonly and naturally used in the target language. Do not use non-standard phrasing when a natural expression exists.\n\n",
                "When encountering slang, idioms, internet expressions, or conversational speech, convey the intended meaning using an expression that native speakers of the target language would naturally understand and use rather than translating it literally.\n\n",
                "Translate only the current input. Do not translate, repeat, summarize, or explain the context. Unless explicitly requested otherwise, output only the final translation."
            )
        );
    }

    #[test]
    fn openai_explicit_with_context_matches_the_canonical_messages() {
        let rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::OpenAiCompatible,
                "Good morning",
                "English",
                "Chinese",
                &context(),
            )
            .unwrap();
        assert_eq!(
            rendered.messages,
            vec![
                PromptMessage {
                    role: PromptMessageRole::System,
                    content: format!(
                        "You are a real-time speech translator. If input is already Chinese, output it unchanged. Otherwise translate it into natural, fluent Chinese. Output only the translation.\n\n{}\n{}",
                        explicit_reference_rules_rendered("English", "Chinese"),
                        reference()
                    ),
                },
                PromptMessage {
                    role: PromptMessageRole::User,
                    content: "Source language: English\nCurrent input:\nGood morning".into(),
                },
            ]
        );
    }

    #[test]
    fn openai_auto_with_context_matches_the_canonical_messages() {
        let rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::OpenAiCompatible,
                "Good morning",
                "auto",
                "Chinese,English",
                &context(),
            )
            .unwrap();
        assert_eq!(
            rendered.messages[0].content,
            format!(
                "You are a real-time speech translator. The input language is one of the following: Chinese,English. Translate it into the OTHER language from that list. Output only the translation.\n\n{}\n{}",
                auto_reference_rules_rendered("Chinese,English"),
                reference()
            )
        );
        assert_eq!(rendered.messages[1].content, "Current input:\nGood morning");
    }

    #[test]
    fn openai_without_context_matches_the_original_messages() {
        let rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::OpenAiCompatible,
                "Good morning",
                "English",
                "Chinese",
                &TranslationPromptContext::default(),
            )
            .unwrap();
        assert_eq!(
            rendered.messages[0].content,
            "You are a real-time speech translator. If input is already Chinese, output it unchanged. Otherwise translate it into natural, fluent Chinese. Output only the translation."
        );
        assert_eq!(
            rendered.messages[1].content,
            "Source language: English\nCurrent input:\nGood morning"
        );
    }

    #[test]
    fn compose_skips_empty_reference_slots_without_extra_blank_lines() {
        let context = TranslationPromptContext {
            language_order: vec!["en".into(), "zh".into()],
            ..TranslationPromptContext::default()
        };
        let rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::OpenAiCompatible,
                "Good morning",
                "English",
                "Chinese",
                &context,
            )
            .unwrap();

        assert_eq!(
            rendered.messages[0].content,
            format!(
                "You are a real-time speech translator. If input is already Chinese, output it unchanged. Otherwise translate it into natural, fluent Chinese. Output only the translation.\n\n{}\n# Translation Context\n\n## Language Order\n\nen,zh",
                explicit_reference_rules_rendered("English", "Chinese")
            )
        );
    }

    #[test]
    fn hunyuan_explicit_with_context_matches_the_canonical_message() {
        let rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::Hunyuan,
                "Good morning",
                "English",
                "Chinese",
                &context(),
            )
            .unwrap();
        assert_eq!(
            rendered.messages,
            vec![PromptMessage {
                role: PromptMessageRole::User,
                content: format!(
                    "Translate the following English text into natural Chinese. Output only the translation, do not output the prompt; do not add explanations.\n\n{}\n\n--- BEGIN REFERENCE CONTEXT ---\n{}\n--- END REFERENCE CONTEXT ---\n\nCurrent input:\nGood morning",
                    explicit_reference_rules_rendered("English", "Chinese"),
                    reference()
                ),
            }]
        );
    }

    #[test]
    fn hunyuan_auto_with_context_matches_the_canonical_message() {
        let rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::Hunyuan,
                "Good morning",
                "auto",
                "Chinese,English",
                &context(),
            )
            .unwrap();
        assert_eq!(
            rendered.messages[0].content,
            format!(
                "Translate the following text into the other language among Chinese,English. Output only the translation; do not add explanations.\n\n{}\n\n--- BEGIN REFERENCE CONTEXT ---\n{}\n--- END REFERENCE CONTEXT ---\n\nCurrent input:\nGood morning",
                auto_reference_rules_rendered("Chinese,English"),
                reference()
            )
        );
    }

    #[test]
    fn runtime_trace_records_real_outputs_and_the_selected_path() {
        let execution = PromptNodeGraph::builtin_default()
            .render_with_trace(
                PromptProviderTarget::Hunyuan,
                "Then when will you?",
                "English",
                "Chinese",
                &context(),
            )
            .unwrap();

        assert_eq!(
            execution.trace.node("current-input").unwrap().output,
            "Then when will you?"
        );
        assert!(
            execution
                .trace
                .node("context-recent-turns")
                .unwrap()
                .output
                .contains("We changed the plan.")
        );
        assert_eq!(
            execution.trace.node("hunyuan-user").unwrap().selected_input,
            Some(1)
        );
        assert!(
            execution
                .trace
                .node("hunyuan-request")
                .unwrap()
                .output
                .ends_with("Current input:\nThen when will you?")
        );
        assert!(execution.trace.node("hunyuan-without-context").is_none());
    }

    #[test]
    fn hunyuan_without_context_matches_the_original_message() {
        let rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::Hunyuan,
                "Good morning",
                "English",
                "Chinese",
                &TranslationPromptContext::default(),
            )
            .unwrap();
        assert_eq!(
            rendered.messages[0].content,
            "Translate the following English text into natural Chinese. Output only the translation, do not output the prompt; do not add explanations.\n\nGood morning"
        );
    }

    #[test]
    fn builtin_graph_uses_compose_nodes_instead_of_fragmented_text_nodes() {
        let graph = PromptNodeGraph::builtin_default();
        assert!(graph.nodes.len() <= 30, "{} nodes", graph.nodes.len());
        assert!(
            graph
                .nodes
                .iter()
                .any(|node| matches!(node.kind, PromptNodeKind::Compose { .. }))
        );
    }

    #[test]
    fn builtin_graph_has_one_ordered_request_per_provider_page() {
        let graph = PromptNodeGraph::builtin_default();
        let requests = graph
            .nodes
            .iter()
            .filter_map(|node| match &node.kind {
                PromptNodeKind::Request { target, roles } => Some((node, target, roles)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(requests.len(), 2);
        let openai = requests
            .iter()
            .find(|(_, target, _)| **target == PromptProviderTarget::OpenAiCompatible)
            .unwrap();
        assert_eq!(openai.0.id, "openai-request");
        assert_eq!(openai.0.page, PromptNodePage::OpenAiCompatible);
        assert_eq!(
            openai.2.as_slice(),
            [PromptMessageRole::System, PromptMessageRole::User]
        );

        let hunyuan = requests
            .iter()
            .find(|(_, target, _)| **target == PromptProviderTarget::Hunyuan)
            .unwrap();
        assert_eq!(hunyuan.0.id, "hunyuan-request");
        assert_eq!(hunyuan.0.page, PromptNodePage::Hunyuan);
        assert_eq!(hunyuan.2.as_slice(), [PromptMessageRole::User]);
    }

    #[test]
    fn provider_page_layouts_do_not_reserve_space_for_hidden_nodes() {
        let graph = PromptNodeGraph::builtin_default();

        for target in [
            PromptProviderTarget::OpenAiCompatible,
            PromptProviderTarget::Hunyuan,
        ] {
            let visible = graph
                .nodes
                .iter()
                .filter(|node| node.page.is_visible_on(target))
                .collect::<Vec<_>>();
            for (index, first) in visible.iter().enumerate() {
                for second in visible.iter().skip(index + 1) {
                    if first.position[0] != second.position[0] {
                        continue;
                    }
                    let first_bottom = first.position[1] + first.layout_height();
                    let second_bottom = second.position[1] + second.layout_height();
                    assert!(
                        first_bottom <= second.position[1] || second_bottom <= first.position[1],
                        "{} overlaps {} on {target:?}",
                        first.id,
                        second.id
                    );
                }
            }
        }
    }

    #[test]
    fn builtin_composition_nodes_have_semantic_editor_labels() {
        let graph = PromptNodeGraph::builtin_default();
        for (id, label) in [
            ("reference-context", "TRANSLATION CONTEXT"),
            ("reference-explicit-rules", "EXPLICIT REFERENCE RULES"),
            ("reference-auto-rules", "AUTO REFERENCE RULES"),
            ("reference-handling-rules", "SELECT REFERENCE RULES"),
            ("openai-explicit-instruction", "EXPLICIT SOURCE INSTRUCTION"),
            ("openai-system", "SELECT SYSTEM PROMPT"),
            ("hunyuan-with-context", "USER PROMPT WITH CONTEXT"),
        ] {
            assert_eq!(
                graph
                    .nodes
                    .iter()
                    .find(|node| node.id == id)
                    .map(|node| node.label.as_str()),
                Some(label)
            );
        }
    }

    #[test]
    fn reference_rules_are_a_visible_graph_input() {
        let graph = PromptNodeGraph::builtin_default();
        let explicit_rules = graph
            .nodes
            .iter()
            .find(|node| node.id == "reference-explicit-rules")
            .unwrap();
        let auto_rules = graph
            .nodes
            .iter()
            .find(|node| node.id == "reference-auto-rules")
            .unwrap();
        let switch_rules = graph
            .nodes
            .iter()
            .find(|node| node.id == "reference-handling-rules")
            .unwrap();
        let openai = graph
            .nodes
            .iter()
            .find(|node| node.id == "openai-system-with-context")
            .unwrap();
        let hunyuan = graph
            .nodes
            .iter()
            .find(|node| node.id == "hunyuan-with-context")
            .unwrap();

        assert_eq!(explicit_rules.label, "EXPLICIT REFERENCE RULES");
        assert_eq!(auto_rules.label, "AUTO REFERENCE RULES");
        assert_eq!(switch_rules.label, "SELECT REFERENCE RULES");
        assert!(explicit_rules.layout_height() > 142.0);
        assert!(auto_rules.layout_height() > 142.0);
        assert_eq!(
            crate::compose_input_indexes(match &openai.kind {
                PromptNodeKind::Compose { text } => text,
                _ => unreachable!(),
            })
            .unwrap(),
            vec![0, 1, 2]
        );
        assert_eq!(
            crate::compose_input_indexes(match &hunyuan.kind {
                PromptNodeKind::Compose { text } => text,
                _ => unreachable!(),
            })
            .unwrap(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn source_auto_condition_preserves_the_original_case_sensitive_behavior() {
        let rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::OpenAiCompatible,
                "Good morning",
                "AUTO",
                "Chinese",
                &TranslationPromptContext::default(),
            )
            .unwrap();
        assert!(
            rendered.messages[0]
                .content
                .contains("If input is already Chinese")
        );
        assert_eq!(
            rendered.messages[1].content,
            "Source language: AUTO\nCurrent input:\nGood morning"
        );
    }

    #[test]
    fn compose_placeholders_must_be_valid_and_connected() {
        let mut graph = PromptNodeGraph::builtin_default();
        let node = graph
            .nodes
            .iter_mut()
            .find(|node| node.id == "openai-explicit-instruction")
            .unwrap();
        node.kind = PromptNodeKind::Compose {
            text: "Translate {5}".into(),
        };
        assert!(graph.validate_for_activation().is_err());

        let mut graph = PromptNodeGraph::builtin_default();
        graph
            .links
            .retain(|link| !(link.to == "openai-explicit-instruction" && link.input == 0));
        assert!(graph.validate_for_activation().is_err());
    }

    #[test]
    fn builtin_default_contains_xiaoyv_instructions_and_prompt_suppression() {
        let graph = PromptNodeGraph::builtin_default();

        let hunyuan_explicit = graph
            .nodes
            .iter()
            .find(|node| node.id == "hunyuan-explicit-instruction")
            .unwrap();
        match &hunyuan_explicit.kind {
            PromptNodeKind::Compose { text } => {
                assert!(
                    text.contains("do not output the prompt"),
                    "hunyuan-explicit-instruction must contain 'do not output the prompt'"
                );
                assert_eq!(
                    text,
                    "Translate the following {0} text into natural {1}. Output only the translation, do not output the prompt; do not add explanations."
                );
            }
            _ => panic!("hunyuan-explicit-instruction must be a Compose node"),
        }

        let explicit_rules = graph
            .nodes
            .iter()
            .find(|node| node.id == "reference-explicit-rules")
            .unwrap();
        match &explicit_rules.kind {
            PromptNodeKind::Compose { text } => {
                assert!(text.contains("100% natural, idiomatic {1}"));
                assert!(text.contains("Discord, QQ, WeChat, gaming chats, and everyday conversations"));
                assert!(text.contains("Unless explicitly requested otherwise, output only the final {1} translation."));
                assert_eq!(text, EXPLICIT_REFERENCE_CONTEXT_INSTRUCTION);
            }
            _ => panic!("reference-explicit-rules must be a Compose node"),
        }

        let auto_rules = graph
            .nodes
            .iter()
            .find(|node| node.id == "reference-auto-rules")
            .unwrap();
        match &auto_rules.kind {
            PromptNodeKind::Compose { text } => {
                assert!(text.contains("into the other language among {0} into 100% natural, idiomatic expression."));
                assert!(text.contains("Discord, QQ, WeChat, gaming chats, and everyday conversations"));
                assert!(text.contains("Unless explicitly requested otherwise, output only the final translation."));
                assert_eq!(text, AUTO_REFERENCE_CONTEXT_INSTRUCTION);
            }
            _ => panic!("reference-auto-rules must be a Compose node"),
        }
    }

    #[test]
    fn reference_handling_rules_adapt_to_source_and_target_languages() {
        let rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::OpenAiCompatible,
                "こんにちは",
                "Japanese",
                "English",
                &context(),
            )
            .unwrap();
        assert!(rendered.messages[0].content.contains("Use the provided context to translate the current Japanese input into 100% natural, idiomatic English."));
        assert!(rendered.messages[0].content.contains("Translate only the current Japanese input. Do not translate, repeat, summarize, or explain the context. Unless explicitly requested otherwise, output only the final English translation."));

        let auto_rendered = PromptNodeGraph::builtin_default()
            .render(
                PromptProviderTarget::OpenAiCompatible,
                "こんにちは",
                "auto",
                "Japanese,English",
                &context(),
            )
            .unwrap();
        assert!(auto_rendered.messages[0].content.contains("Use the provided context to translate the current input into the other language among Japanese,English into 100% natural, idiomatic expression."));
        assert!(auto_rendered.messages[0].content.contains("Translate only the current input. Do not translate, repeat, summarize, or explain the context. Unless explicitly requested otherwise, output only the final translation."));
    }
}

