//! Compact, stateful language routing for bidirectional automatic sessions.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Script {
    Latin,
    Han,
    Japanese,
    Cyrillic,
    Hangul,
    Thai,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LanguageSpec {
    code: &'static str,
    model_name: &'static str,
    script: Script,
}

macro_rules! language {
    ($code:literal, $name:literal, $script:ident) => {
        LanguageSpec {
            code: $code,
            model_name: $name,
            script: Script::$script,
        }
    };
}

const LANGUAGES: &[LanguageSpec] = &[
    language!("af", "Afrikaans", Latin),
    language!("zh", "Chinese", Han),
    language!("en", "English", Latin),
    language!("fr", "French", Latin),
    language!("pt", "Portuguese", Latin),
    language!("es", "Spanish", Latin),
    language!("ja", "Japanese", Japanese),
    language!("ru", "Russian", Cyrillic),
    language!("ko", "Korean", Hangul),
    language!("th", "Thai", Thai),
    language!("it", "Italian", Latin),
    language!("de", "German", Latin),
    language!("vi", "Vietnamese", Latin),
    language!("id", "Indonesian", Latin),
    language!("pl", "Polish", Latin),
    language!("cs", "Czech", Latin),
    language!("nl", "Dutch", Latin),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SupportedLanguage(&'static LanguageSpec);

impl SupportedLanguage {
    pub(crate) fn from_code(code: &str) -> Option<Self> {
        LANGUAGES
            .iter()
            .find(|language| language.code.eq_ignore_ascii_case(code.trim()))
            .map(Self)
    }

    pub(crate) fn from_model_label(label: &str) -> Option<Self> {
        let label = label.trim();
        LANGUAGES
            .iter()
            .find(|language| language.model_name.eq_ignore_ascii_case(label))
            .map(Self)
            .or_else(|| {
                label
                    .eq_ignore_ascii_case("Mandarin")
                    .then(|| Self(&LANGUAGES[1]))
            })
    }

    pub(crate) const fn code(self) -> &'static str {
        self.0.code
    }

    pub(crate) const fn model_name(self) -> &'static str {
        self.0.model_name
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LanguagePair([SupportedLanguage; 2]);

impl LanguagePair {
    fn parse(source: &str, targets: &str) -> Option<Self> {
        if !source.trim().eq_ignore_ascii_case("auto") {
            return None;
        }
        let mut languages = targets.split(',').map(SupportedLanguage::from_code);
        let pair = Self([languages.next()??, languages.next()??]);
        (languages.next().is_none() && pair.0[0] != pair.0[1]).then_some(pair)
    }

    fn contains(self, language: SupportedLanguage) -> bool {
        self.0.contains(&language)
    }

    fn other(self, language: SupportedLanguage) -> SupportedLanguage {
        if language == self.0[0] {
            self.0[1]
        } else {
            self.0[0]
        }
    }

    fn route(self, source: SupportedLanguage) -> LanguageRoute {
        LanguageRoute {
            source,
            target: self.other(source),
        }
    }

    fn language_for_text(self, text: &str) -> Option<SupportedLanguage> {
        match (
            script_evidence(self.0[0], text),
            script_evidence(self.0[1], text),
        ) {
            (Evidence::Compatible, Evidence::Incompatible) => Some(self.0[0]),
            (Evidence::Incompatible, Evidence::Compatible) => Some(self.0[1]),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LanguageRoute {
    pub(crate) source: SupportedLanguage,
    pub(crate) target: SupportedLanguage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutoDecision {
    Accept(LanguageRoute),
    Retry {
        language: SupportedLanguage,
        candidate: Option<SupportedLanguage>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryDecision {
    Keep(LanguageRoute),
    Switch(LanguageRoute),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Evidence {
    Compatible,
    Unknown,
    Incompatible,
}

/// Per-worker adaptive state. The saved user pair remains immutable; only the
/// effective pair used by this live session can change.
#[derive(Debug, Default)]
pub(crate) struct AdaptiveLanguageRoute {
    configured: Option<LanguagePair>,
    active: Option<LanguagePair>,
    anchor: Option<SupportedLanguage>,
    pending: Option<(SupportedLanguage, u8)>,
}

impl AdaptiveLanguageRoute {
    pub(crate) fn configure(&mut self, source: &str, targets: &str) {
        let configured = LanguagePair::parse(source, targets);
        if self.configured != configured {
            self.configured = configured;
            self.active = configured;
            self.anchor = None;
            self.pending = None;
        }
    }

    pub(crate) fn active_targets(&self, fallback: &str) -> String {
        self.active.map_or_else(
            || fallback.to_owned(),
            |pair| format!("{},{}", pair.0[0].code(), pair.0[1].code()),
        )
    }

    pub(crate) fn prompt_hint(&self) -> Option<String> {
        self.active.map(|pair| {
            format!(
                "Expected spoken languages are {} or {}. Prefer these, but identify any clearly different language accurately. Transcribe without translating.",
                pair.0[0].model_name(),
                pair.0[1].model_name()
            )
        })
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn classify(&mut self, detected: Option<&str>, text: &str) -> AutoDecision {
        let pair = self
            .active
            .expect("classify is only used for an automatic pair");
        let languages = || {
            detected
                .into_iter()
                .flat_map(|label| label.split(','))
                .filter_map(SupportedLanguage::from_model_label)
        };
        if let Some(language) = languages().find(|language| {
            pair.contains(*language) && script_evidence(*language, text) != Evidence::Incompatible
        }) {
            self.confirm(language);
            return AutoDecision::Accept(pair.route(language));
        }

        let language = pair
            .language_for_text(text)
            .or(self.anchor.filter(|language| pair.contains(*language)))
            .unwrap_or(pair.0[0]);
        let candidate = languages().find(|language| {
            !pair.contains(*language) && script_evidence(*language, text) != Evidence::Incompatible
        });
        AutoDecision::Retry {
            language,
            candidate,
        }
    }

    pub(crate) fn recovery(
        &mut self,
        forced: SupportedLanguage,
        forced_text: &str,
        candidate: Option<SupportedLanguage>,
    ) -> RecoveryDecision {
        let pair = self
            .active
            .expect("recovery is only used for an automatic pair");
        let Some(candidate) = candidate else {
            self.confirm(forced);
            return RecoveryDecision::Keep(pair.route(forced));
        };

        // A forced result written in a script which excludes the outside
        // candidate is positive recovery evidence, not a reason to switch.
        if script_evidence(forced, forced_text) == Evidence::Compatible
            && script_evidence(candidate, forced_text) == Evidence::Incompatible
        {
            self.confirm(forced);
            return RecoveryDecision::Keep(pair.route(forced));
        }

        let count = match self.pending {
            Some((pending, count)) if pending == candidate => count.saturating_add(1),
            _ => 1,
        };
        self.pending = Some((candidate, count));
        let threshold = if scripts_overlap(candidate, forced) {
            3
        } else {
            2
        };
        if count < threshold {
            return RecoveryDecision::Keep(pair.route(forced));
        }

        let partner = self
            .anchor
            .filter(|language| pair.contains(*language))
            .unwrap_or(pair.0[1]);
        let active = LanguagePair([candidate, partner]);
        self.active = Some(active);
        self.anchor = Some(candidate);
        self.pending = None;
        RecoveryDecision::Switch(active.route(candidate))
    }

    pub(crate) fn evidence(&self, language: SupportedLanguage, text: &str) -> bool {
        script_evidence(language, text) != Evidence::Incompatible
    }

    pub(crate) fn alternate(&self, language: SupportedLanguage) -> SupportedLanguage {
        self.active
            .expect("alternate requires an automatic pair")
            .other(language)
    }

    fn confirm(&mut self, language: SupportedLanguage) {
        self.anchor = Some(language);
        self.pending = None;
    }
}

fn scripts_overlap(left: SupportedLanguage, right: SupportedLanguage) -> bool {
    left.0.script == right.0.script
        || matches!(
            (left.0.script, right.0.script),
            (Script::Han, Script::Japanese) | (Script::Japanese, Script::Han)
        )
}

fn script_evidence(language: SupportedLanguage, text: &str) -> Evidence {
    let observed = observed_scripts(text);
    if observed.is_empty() {
        return Evidence::Unknown;
    }
    if language.0.script == Script::Han && observed.contains(&Script::Japanese) {
        return Evidence::Incompatible;
    }
    if observed.iter().any(|script| {
        *script == language.0.script
            || (language.0.script == Script::Japanese && *script == Script::Han)
    }) {
        Evidence::Compatible
    } else {
        Evidence::Incompatible
    }
}

fn observed_scripts(text: &str) -> Vec<Script> {
    let mut scripts = Vec::with_capacity(2);
    for character in text.chars() {
        let script =
            if character.is_ascii_alphabetic() || ('\u{00c0}'..='\u{024f}').contains(&character) {
                Some(Script::Latin)
            } else if ('\u{3040}'..='\u{31ff}').contains(&character) {
                Some(Script::Japanese)
            } else if ('\u{3400}'..='\u{9fff}').contains(&character) {
                Some(Script::Han)
            } else if ('\u{0400}'..='\u{04ff}').contains(&character) {
                Some(Script::Cyrillic)
            } else if ('\u{1100}'..='\u{11ff}').contains(&character)
                || ('\u{ac00}'..='\u{d7af}').contains(&character)
            {
                Some(Script::Hangul)
            } else if ('\u{0e00}'..='\u{0e7f}').contains(&character) {
                Some(Script::Thai)
            } else {
                None
            };
        if let Some(script) = script
            && !scripts.contains(&script)
        {
            scripts.push(script);
        }
    }
    scripts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn language(code: &str) -> SupportedLanguage {
        SupportedLanguage::from_code(code).unwrap()
    }

    #[test]
    fn all_configured_languages_round_trip() {
        for spec in LANGUAGES {
            assert_eq!(
                SupportedLanguage::from_code(spec.code),
                Some(SupportedLanguage(spec))
            );
            assert_eq!(
                SupportedLanguage::from_model_label(spec.model_name),
                Some(SupportedLanguage(spec))
            );
        }
    }

    #[test]
    fn one_outside_detection_never_switches() {
        let mut route = AdaptiveLanguageRoute::default();
        route.configure("auto", "ja,en");
        assert!(matches!(
            route.classify(Some("Chinese,English"), "hello"),
            AutoDecision::Accept(LanguageRoute { source, .. }) if source == language("en")
        ));
        assert!(matches!(
            route.classify(Some("Chinese"), "再会"),
            AutoDecision::Retry {
                candidate: Some(_),
                ..
            }
        ));
        assert!(matches!(
            route.recovery(language("ja"), "再会", Some(language("zh"))),
            RecoveryDecision::Keep(_)
        ));
        assert_eq!(route.active_targets(""), "ja,en");
    }

    #[test]
    fn distinct_script_switches_after_two_confirmations_and_keeps_anchor() {
        let mut route = AdaptiveLanguageRoute::default();
        route.configure("auto", "ja,en");
        assert!(matches!(
            route.classify(Some("English"), "hello"),
            AutoDecision::Accept(_)
        ));
        for expected_switch in [false, true] {
            let decision = route.classify(Some("Korean"), "안녕하세요");
            let AutoDecision::Retry { candidate, .. } = decision else {
                panic!()
            };
            let switched = matches!(
                route.recovery(language("ja"), "안녕하세요", candidate),
                RecoveryDecision::Switch(_)
            );
            assert_eq!(switched, expected_switch);
        }
        assert_eq!(route.active_targets(""), "ko,en");
    }

    #[test]
    fn shared_script_requires_three_confirmations() {
        let mut route = AdaptiveLanguageRoute::default();
        route.configure("auto", "ja,en");
        for expected_switch in [false, false, true] {
            let AutoDecision::Retry {
                language: forced,
                candidate,
            } = route.classify(Some("Chinese"), "再会")
            else {
                panic!()
            };
            assert_eq!(
                matches!(
                    route.recovery(forced, "再会", candidate),
                    RecoveryDecision::Switch(_)
                ),
                expected_switch
            );
        }
        assert_eq!(route.active_targets(""), "zh,en");
    }

    #[test]
    fn decisive_in_pair_recovery_cancels_candidate() {
        let mut route = AdaptiveLanguageRoute::default();
        route.configure("auto", "ja,en");
        for _ in 0..4 {
            let AutoDecision::Retry {
                language,
                candidate,
            } = route.classify(Some("Chinese"), "中国語")
            else {
                panic!()
            };
            assert!(matches!(
                route.recovery(language, "これは日本語です", candidate),
                RecoveryDecision::Keep(_)
            ));
        }
        assert_eq!(route.active_targets(""), "ja,en");
    }

    #[test]
    fn reconfiguration_resets_adaptation() {
        let mut route = AdaptiveLanguageRoute::default();
        route.configure("auto", "ja,en");
        route.pending = Some((language("zh"), 2));
        route.configure("auto", "fr,de");
        assert_eq!(route.active_targets(""), "fr,de");
        assert_eq!(route.pending, None);
    }

    #[test]
    fn invalid_automatic_pair_is_not_configured() {
        let mut route = AdaptiveLanguageRoute::default();
        route.configure("auto", "en,en");
        assert!(!route.is_configured());
    }
}
