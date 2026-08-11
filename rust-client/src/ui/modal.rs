use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Vec2,
};

#[derive(Clone, Debug)]
pub struct ModalPage {
    pub title: String,
    pub content: String,
    pub is_code: bool,
    pub icon: Option<String>,
}

impl ModalPage {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            content: content.into(),
            is_code: false,
            icon: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn code(mut self) -> Self {
        self.is_code = true;
        self
    }
}

pub struct ModalDialog {
    pub open: bool,
    pub pages: Vec<ModalPage>,
    pub current_page: usize,
    pub show_ok_button: bool,
    pub ok_label: String,
    pub show_cancel_button: bool,
    pub cancel_label: String,
}

impl Default for ModalDialog {
    fn default() -> Self {
        Self {
            open: false,
            pages: Vec::new(),
            current_page: 0,
            show_ok_button: true,
            ok_label: "OK".into(),
            show_cancel_button: false,
            cancel_label: "Cancel".into(),
        }
    }
}

impl ModalDialog {
    pub fn error(
        title: impl Into<String>,
        message: impl Into<String>,
        details: Option<&str>,
    ) -> Self {
        let mut content = message.into();
        if let Some(details) = details {
            if !details.trim().is_empty() {
                content.push_str("\n\n--- Detailed Log Output ---\n");
                content.push_str(details.trim());
            }
        }
        let page = ModalPage::new(title, content).code();
        Self {
            open: true,
            pages: vec![page],
            current_page: 0,
            show_ok_button: true,
            ok_label: "OK".into(),
            show_cancel_button: false,
            cancel_label: "Close".into(),
        }
    }

    #[allow(dead_code)]
    pub fn info(title: impl Into<String>, message: impl Into<String>) -> Self {
        let page = ModalPage::new(title, message);
        Self {
            open: true,
            pages: vec![page],
            current_page: 0,
            show_ok_button: true,
            ok_label: "OK".into(),
            show_cancel_button: false,
            cancel_label: "Close".into(),
        }
    }

    pub fn carousel(pages: Vec<ModalPage>) -> Self {
        Self {
            open: true,
            pages,
            current_page: 0,
            show_ok_button: true,
            ok_label: "Finish".into(),
            show_cancel_button: false,
            cancel_label: "Close".into(),
        }
    }

    pub fn render(&mut self, ctx: &egui::Context, language: crate::i18n::UiLanguage) {
        if !self.open || self.pages.is_empty() {
            return;
        }

        // 1. Dimmed Backdrop Overlay
        egui::Area::new(egui::Id::new("modal_backdrop"))
            .interactable(true)
            .fixed_pos([0.0, 0.0])
            .show(ctx, |ui| {
                let screen = ctx
                    .input(|i| i.raw.screen_rect)
                    .unwrap_or_else(|| ui.max_rect());
                ui.allocate_rect(screen, egui::Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, Color32::from_black_alpha(140));
            });

        if self.current_page >= self.pages.len() {
            self.current_page = 0;
        }
        let page = self.pages[self.current_page].clone();
        let total_pages = self.pages.len();
        let is_multi_page = total_pages > 1;

        let mut close_dialog = false;

        // 2. Centered Floating Window
        egui::Window::new("modal_dialog_window")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([540.0, 380.0])
            .frame(
                Frame::new()
                    .fill(Color32::WHITE)
                    .corner_radius(CornerRadius::same(14))
                    .stroke(Stroke::new(1.0, Color32::from_gray(210)))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Header Bar
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&page.title)
                                .size(17.0)
                                .color(crate::ui::theme::text_strong())
                                .strong(),
                        );

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let close_btn = ui.add(
                                egui::Button::new(RichText::new("X").size(12.0).strong())
                                    .min_size(Vec2::new(24.0, 24.0))
                                    .corner_radius(CornerRadius::same(6)),
                            );
                            if close_btn.clicked() {
                                close_dialog = true;
                            }
                        });
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    // Body Content Area
                    let body_height = if is_multi_page { 220.0 } else { 240.0 };
                    egui::ScrollArea::vertical()
                        .id_salt("modal_body_scroll")
                        .max_height(body_height)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if page.is_code {
                                Frame::new()
                                    .fill(Color32::from_rgb(24, 26, 38))
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(Margin::same(12))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(
                                            RichText::new(&page.content)
                                                .family(egui::FontFamily::Monospace)
                                                .color(Color32::from_rgb(240, 244, 255))
                                                .size(12.0),
                                        );
                                    });

                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    if crate::ui::components::animated_button(
                                        ui,
                                        crate::i18n::tr(language, "Copy Log"),
                                    )
                                    .clicked()
                                    {
                                        ctx.copy_text(page.content.clone());
                                    }
                                });
                            } else {
                                ui.label(
                                    RichText::new(&page.content)
                                        .size(13.5)
                                        .color(crate::ui::theme::text_normal()),
                                );
                            }
                        });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Footer / Controls
                    ui.horizontal(|ui| {
                        if is_multi_page {
                            ui.label(
                                RichText::new(format!(
                                    "{} {}/{}",
                                    crate::i18n::tr(language, "Page"),
                                    self.current_page + 1,
                                    total_pages
                                ))
                                .size(12.0)
                                .color(crate::ui::theme::text_weak())
                                .strong(),
                            );

                            ui.add_space(12.0);

                            if self.current_page > 0 {
                                if crate::ui::components::animated_button(
                                    ui,
                                    crate::i18n::tr(language, "Prev"),
                                )
                                .clicked()
                                {
                                    self.current_page -= 1;
                                }
                            }

                            if self.current_page + 1 < total_pages {
                                if crate::ui::components::primary_button(
                                    ui,
                                    crate::i18n::tr(language, "Next"),
                                )
                                .clicked()
                                {
                                    self.current_page += 1;
                                }
                            }
                        }

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if self.show_ok_button {
                                let ok_text =
                                    if is_multi_page && self.current_page + 1 < total_pages {
                                        crate::i18n::tr(language, "Close")
                                    } else {
                                        &self.ok_label
                                    };
                                if crate::ui::components::primary_button(ui, ok_text).clicked() {
                                    close_dialog = true;
                                }
                            }
                            if self.show_cancel_button {
                                if crate::ui::components::animated_button(ui, &self.cancel_label)
                                    .clicked()
                                {
                                    close_dialog = true;
                                }
                            }
                        });
                    });
                });
            });

        if close_dialog {
            self.open = false;
        }
    }
}
