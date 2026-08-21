use super::*;
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

/// Renders the keybinding and navigation cheatsheet in the bottom right corner of the canvas.
pub(super) fn render_canvas_navigation_hint(
    ui: &egui::Ui,
    canvas: Rect,
    language: crate::i18n::UiLanguage,
) {
    let font_id = egui::FontId::monospace(9.5);
    let text_color = Color32::BLACK;

    let items = [
        (
            "NAVIGATE",
            "Space + Left Drag / Middle Drag to pan · Mouse Wheel to zoom",
        ),
        (
            "SELECT",
            "Left Drag on canvas to box select · Shift + Click to multi-select",
        ),
        (
            "CONNECT",
            "Drag socket to connect / unplug · Click empty space to cancel wire",
        ),
        (
            "ACTIONS",
            "Del to delete · Double-Click header to rename · Ctrl+Z: Undo · Ctrl+Y: Redo",
        ),
    ];

    let line_height = 16.0;
    let base_y = canvas.bottom() - 12.0;
    let right_x = canvas.right() - 16.0;

    for (index, (tag, detail)) in items.iter().rev().enumerate() {
        let y = base_y - index as f32 * line_height;
        let line_text = format!(
            "{} · {}",
            crate::i18n::tr(language, tag),
            crate::i18n::tr(language, detail)
        );
        ui.painter().text(
            Pos2::new(right_x, y),
            egui::Align2::RIGHT_BOTTOM,
            line_text,
            font_id.clone(),
            text_color,
        );
    }
}

/// Centers and scales the canvas viewport so all visible graph nodes fit comfortably.
pub(super) fn fit_graph_to_canvas(
    graph: &PromptNodeGraph,
    controller: &mut PromptStudioController,
    available: Vec2,
) {
    if !graph
        .nodes
        .iter()
        .any(|node| controller.node_is_visible(node))
    {
        return;
    }
    let min_x = graph
        .nodes
        .iter()
        .filter(|node| controller.node_is_visible(node))
        .map(|node| node.position[0])
        .fold(f32::INFINITY, f32::min);
    let max_x = graph
        .nodes
        .iter()
        .filter(|node| controller.node_is_visible(node))
        .map(|node| node.position[0] + node_size(node).x)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = graph
        .nodes
        .iter()
        .filter(|node| controller.node_is_visible(node))
        .map(|node| node.position[1])
        .fold(f32::INFINITY, f32::min);
    let max_y = graph
        .nodes
        .iter()
        .filter(|node| controller.node_is_visible(node))
        .map(|node| node.position[1] + node_size(node).y)
        .fold(f32::NEG_INFINITY, f32::max);
    let graph_width = (max_x - min_x).max(NODE_WIDTH);
    let graph_height = (max_y - min_y).max(84.0);
    let viewport = (available - Vec2::splat(48.0)).max(Vec2::splat(1.0));
    controller.zoom = (viewport.x / graph_width)
        .min(viewport.y / graph_height)
        .clamp(0.25, 1.0);
    controller.pan = Vec2::new(
        (available.x - graph_width * controller.zoom) * 0.5 - min_x * controller.zoom,
        (available.y - graph_height * controller.zoom) * 0.5 - min_y * controller.zoom,
    );
}

/// Zooms the canvas anchored at the current mouse pointer location.
pub(super) fn zoom_at_pointer(
    controller: &mut PromptStudioController,
    canvas: Rect,
    pointer: Pos2,
    scroll: f32,
) {
    let old_zoom = controller.zoom;
    let factor = (scroll * 0.0015).exp();
    let new_zoom = (old_zoom * factor).clamp(0.25, 1.6);
    if (new_zoom - old_zoom).abs() <= f32::EPSILON {
        return;
    }
    let pointer_in_canvas = pointer - canvas.min;
    let graph_position = (pointer_in_canvas - controller.pan) / old_zoom;
    controller.zoom = new_zoom;
    controller.pan = pointer_in_canvas - graph_position * new_zoom;
}

/// Handles edge auto-panning and edge-triggered dynamic zoom-out / smooth recovery during wire dragging.
pub(super) fn update_wire_dragging_navigation(
    controller: &mut PromptStudioController,
    canvas: Rect,
    ui: &egui::Ui,
    is_pulling_wire: bool,
) {
    let dt = ui.input(|i| i.predicted_dt).clamp(1.0 / 120.0, 0.1);

    if is_pulling_wire {
        let Some(pointer) = ui
            .ctx()
            .pointer_hover_pos()
            .or_else(|| ui.ctx().pointer_latest_pos())
        else {
            return;
        };

        // 1. Edge Auto-Panning & Edge Proximity Calculation
        let edge_margin = 72.0; // Distance in pixels from canvas edge to trigger auto-pan and zoom out
        let base_pan_speed = 560.0; // Max pan speed in pixels per second

        let mut pan_delta = Vec2::ZERO;
        let mut max_edge_intensity: f32 = 0.0;

        // Horizontal edge pan
        if pointer.x < canvas.left() + edge_margin {
            let overflow = (canvas.left() + edge_margin - pointer.x).max(0.0);
            let intensity = (overflow / edge_margin).clamp(0.0, 3.0);
            pan_delta.x += base_pan_speed * intensity * dt;
            max_edge_intensity = max_edge_intensity.max(intensity);
        } else if pointer.x > canvas.right() - edge_margin {
            let overflow = (pointer.x - (canvas.right() - edge_margin)).max(0.0);
            let intensity = (overflow / edge_margin).clamp(0.0, 3.0);
            pan_delta.x -= base_pan_speed * intensity * dt;
            max_edge_intensity = max_edge_intensity.max(intensity);
        }

        // Vertical edge pan
        if pointer.y < canvas.top() + edge_margin {
            let overflow = (canvas.top() + edge_margin - pointer.y).max(0.0);
            let intensity = (overflow / edge_margin).clamp(0.0, 3.0);
            pan_delta.y += base_pan_speed * intensity * dt;
            max_edge_intensity = max_edge_intensity.max(intensity);
        } else if pointer.y > canvas.bottom() - edge_margin {
            let overflow = (pointer.y - (canvas.bottom() - edge_margin)).max(0.0);
            let intensity = (overflow / edge_margin).clamp(0.0, 3.0);
            pan_delta.y -= base_pan_speed * intensity * dt;
            max_edge_intensity = max_edge_intensity.max(intensity);
        }

        if pan_delta != Vec2::ZERO {
            controller.pan += pan_delta;
            ui.ctx().request_repaint();
        }

        // 2. Edge-Triggered Temporary Dynamic Zoom-Out
        let base_zoom = *controller.wire_base_zoom.get_or_insert(controller.zoom);

        // When nearing or crossing edges to pan, temporarily widen FOV (zoom out up to 22%)
        let zoom_out_ratio = (max_edge_intensity / 1.5).clamp(0.0, 1.0);
        let target_zoom = (base_zoom * (1.0 - zoom_out_ratio * 0.22)).clamp(0.20, 1.6);

        let zoom_change_rate = if target_zoom < controller.zoom { 7.0 } else { 5.0 };
        let zoom_step = 1.0 - (-dt * zoom_change_rate).exp();
        let new_zoom = controller.zoom + (target_zoom - controller.zoom) * zoom_step;

        if (new_zoom - controller.zoom).abs() > 0.0005 {
            let center = canvas.size() * 0.5;
            let graph_pos = (center - controller.pan) / controller.zoom;
            controller.zoom = new_zoom;
            controller.pan = center - graph_pos * new_zoom;
            ui.ctx().request_repaint();
        }
    } else if let Some(base_zoom) = controller.wire_base_zoom {
        // Smoothly recover back to base_zoom when wire is released or stopped
        let zoom_step = 1.0 - (-dt * 10.0).exp();
        let new_zoom = controller.zoom + (base_zoom - controller.zoom) * zoom_step;

        if (new_zoom - controller.zoom).abs() > 0.001 {
            let center = canvas.size() * 0.5;
            let graph_pos = (center - controller.pan) / controller.zoom;
            controller.zoom = new_zoom;
            controller.pan = center - graph_pos * new_zoom;
            ui.ctx().request_repaint();
        } else {
            controller.zoom = base_zoom;
            controller.wire_base_zoom = None;
        }
    }
}
