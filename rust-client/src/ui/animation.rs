use eframe::egui::{self, Color32, Id};

/// High-performance systematic animation infrastructure for egui.
pub struct AnimationSystem;

impl AnimationSystem {
    /// Computes a smooth cubic ease-out curve from a normalized progress (0.0 to 1.0).
    pub fn ease_out_cubic(t: f32) -> f32 {
        let p = (1.0 - t.clamp(0.0, 1.0)).max(0.0);
        1.0 - p * p * p
    }

    /// Computes an animated float over a specified duration (in seconds).
    /// Automatically requests repaints while animating to maintain 60 FPS performance,
    /// and pauses repaints when static for zero idle CPU usage.
    pub fn animate_value(
        ctx: &egui::Context,
        id: Id,
        target_val: f32,
        duration: f32,
    ) -> f32 {
        let current = ctx.animate_value_with_time(id, target_val, duration);
        if (current - target_val).abs() > 0.001 {
            ctx.request_repaint();
        }
        current
    }

    /// Animates a boolean state (e.g. hover, active, toggle) returning 0.0..1.0 factor.
    pub fn animate_bool(ctx: &egui::Context, id: Id, active: bool, duration: f32) -> f32 {
        let target = if active { 1.0 } else { 0.0 };
        Self::animate_value(ctx, id, target, duration)
    }

    /// Interpolates linearly between two colors based on progress t (0.0 .. 1.0).
    pub fn lerp_color(from: Color32, to: Color32, t: f32) -> Color32 {
        let factor = t.clamp(0.0, 1.0);
        Color32::from_rgba_premultiplied(
            (from.r() as f32 + (to.r() as f32 - from.r() as f32) * factor) as u8,
            (from.g() as f32 + (to.g() as f32 - from.g() as f32) * factor) as u8,
            (from.b() as f32 + (to.b() as f32 - from.b() as f32) * factor) as u8,
            (from.a() as f32 + (to.a() as f32 - from.a() as f32) * factor) as u8,
        )
    }

    /// Smooth page transition wrapper that applies vertical slide offset and opacity cross-fade.
    pub fn render_animated_page<P, R>(
        ui: &mut egui::Ui,
        page_id: P,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R 
    where
        P: std::hash::Hash + std::fmt::Debug,
    {
        let current_time = ui.ctx().input(|i| i.time);
        // Scope the state by the type of the page_id to prevent nested transitions from fighting over the same state
        let global_id = Id::new("page_transition_state").with(std::any::type_name::<P>());
        
        let target_hash = Id::new(&page_id).value();
        
        let start_time = ui.ctx().memory_mut(|m| {
            let state = m.data.get_temp_mut_or_insert_with(global_id, || (target_hash, current_time));
            if state.0 != target_hash {
                state.0 = target_hash;
                state.1 = current_time;
            }
            state.1
        });

        let elapsed = (current_time - start_time) as f32;
        let duration = 0.25;
        let raw_t = (elapsed / duration).clamp(0.0, 1.0);
        
        if raw_t < 1.0 {
            ui.ctx().request_repaint();
        }

        let eased = Self::ease_out_cubic(raw_t);

        let y_offset = (1.0 - eased) * 12.0;

        ui.scope(|ui| {
            if y_offset > 0.1 {
                ui.add_space(y_offset);
            }
            if eased < 0.999 {
                ui.set_opacity(eased);
            }
            add_contents(ui)
        })
        .inner
    }

    /// Liquid smooth audio level decay easing for audio meters.
    pub fn smooth_audio_level(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug, target_level: f32) -> f32 {
        let persistent_id = Id::new(id);
        Self::animate_value(ctx, persistent_id, target_level, 0.08)
    }
}
