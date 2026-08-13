use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayState {
    pub font_size: u32,
    pub max_items: usize,
    pub visible_entries: Vec<OverlayEntry>,
    pub partial_text: Option<String>,
    pub vad_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayEntry {
    pub source: String,
    pub translated: String,
    pub live: bool,
    pub vad_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverlayEvent {
    CloseRequested,
    MaxCountChanged(usize),
}

#[cfg(test)]
mod tests {
    use super::OverlayState;

    #[test]
    fn vad_activity_is_sent_to_the_overlay_process() {
        let state = OverlayState {
            font_size: 14,
            max_items: 5,
            visible_entries: Vec::new(),
            partial_text: None,
            vad_active: true,
        };
        let json = serde_json::to_string(&state).unwrap();

        assert!(json.contains(r#""vad_active":true"#));
    }
}
