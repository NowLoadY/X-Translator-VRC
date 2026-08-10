use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayState {
    pub font_size: u32,
    pub max_items: usize,
    pub visible_entries: Vec<(String, String)>,
    pub partial_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverlayEvent {
    CloseRequested,
    MaxCountChanged(usize),
}
