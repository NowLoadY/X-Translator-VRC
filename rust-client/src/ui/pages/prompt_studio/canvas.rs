use super::*;

#[derive(Clone, Copy)]
struct NodePalette {
    fill: Color32,
    header: Color32,
    connector: Color32,
}

fn node_palette(kind: &PromptNodeKind) -> NodePalette {
    let (fill, header, connector) = match kind {
        PromptNodeKind::Input { .. } => ((250, 251, 249), (220, 224, 220), (100, 108, 103)),
        PromptNodeKind::Variable { .. } => ((248, 251, 251), (216, 222, 223), (98, 109, 111)),
        PromptNodeKind::Compose { .. } => ((252, 251, 247), (225, 221, 210), (116, 108, 88)),
        PromptNodeKind::Switch { .. } => ((251, 249, 252), (223, 217, 224), (110, 100, 113)),
        PromptNodeKind::Request { .. } => ((248, 250, 252), (215, 222, 228), (96, 107, 117)),
    };
    NodePalette {
        fill: Color32::from_rgb(fill.0, fill.1, fill.2),
        header: Color32::from_rgb(header.0, header.1, header.2),
        connector: Color32::from_rgb(connector.0, connector.1, connector.2),
    }
}

fn node_kind_tag(graph: &PromptNodeGraph, node: &PromptNode) -> String {
    match &node.kind {
        PromptNodeKind::Input { .. } => "DATA".into(),
        PromptNodeKind::Variable { .. } => "VALUE".into(),
        PromptNodeKind::Compose { .. } => {
            let count = graph.links.iter().filter(|link| link.to == node.id).count();
            format!("COMPOSE · {count}/10")
        }
        PromptNodeKind::Switch { .. } => "BRANCH".into(),
        PromptNodeKind::Request { roles, .. } => format!("REQUEST · {}", roles.len()),
    }
}

fn input_description(kind: &PromptNodeKind) -> String {
    match kind {
        PromptNodeKind::Input {
            block: TranslationPromptBlock::LanguageOrder,
        } => "Preferred language sequence".into(),
        PromptNodeKind::Input {
            block: TranslationPromptBlock::Terminology,
        } => "Required terminology rows".into(),
        PromptNodeKind::Input {
            block: TranslationPromptBlock::RecentTurns { limit },
        } => limit.map_or_else(
            || "Completed bilingual history".into(),
            |limit| format!("Last {limit} bilingual turns"),
        ),
        PromptNodeKind::Input {
            block: TranslationPromptBlock::PreviousRevision,
        } => "Earlier streaming revision".into(),
        PromptNodeKind::Input {
            block: TranslationPromptBlock::SurroundingSource,
        } => "Nearby source speech".into(),
        PromptNodeKind::Input {
            block: TranslationPromptBlock::CustomText { .. },
        } => "Fixed instruction text".into(),
        _ => String::new(),
    }
}

fn condition_expression(condition: PromptCondition) -> &'static str {
    match condition {
        PromptCondition::SourceIsAuto => "Is source language set to auto?",
        PromptCondition::HasReferenceContext => "Is reference context available?",
    }
}

fn request_summary(message_count: usize) -> String {
    let noun = if message_count == 1 {
        "MESSAGE"
    } else {
        "MESSAGES"
    };
    format!("{message_count} {noun} · ONE API REQUEST")
}

fn input_socket_tooltip(graph: &PromptNodeGraph, node: &PromptNode, input: u8) -> String {
    let socket = input_socket_label(node, input);
    graph
        .links
        .iter()
        .find(|link| link.to == node.id && link.input == input)
        .and_then(|link| graph.nodes.iter().find(|source| source.id == link.from))
        .map_or_else(
            || format!("{socket} · Not connected"),
            |source| format!("{socket} · {}", node_display_label(source)),
        )
}

pub(super) fn render_graph_editor(
    snapshot: &PromptStudioSnapshot,
    controller: &mut PromptStudioController,
    ui: &mut egui::Ui,
    actions: &mut Vec<PromptStudioAction>,
) {
    let Some(mut draft) = controller.draft.clone() else {
        return;
    };
    let runtime_trace = (snapshot.selected_id == snapshot.active_id && !controller.dirty)
        .then(|| controller.runtime_trace.clone())
        .flatten()
        .filter(|trace| {
            trace.target == controller.active_provider
                && trace.graph_fingerprint == draft.graph.fingerprint()
        });
    let validation_error = draft.graph.validate_for_activation().err();
    let editable = !draft.read_only;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("GRAPH /")
                .font(egui::FontId::monospace(10.0))
                .color(style::MUTED)
                .strong(),
        );
        ui.add_space(5.0);
        if editable {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut draft.name)
                        .font(egui::FontId::monospace(12.0))
                        .desired_width(260.0),
                )
                .changed()
            {
                controller.mark_dirty();
            }
        } else {
            ui.label(
                RichText::new(&draft.name)
                    .font(egui::FontId::monospace(12.0))
                    .color(style::INK)
                    .strong(),
            );
        }
        ui.add_space(10.0);
        if !editable {
            status_chip(ui, "LOCKED");
        }
        ui.separator();
        render_provider_tabs(controller, ui);
        if editable {
            ui.separator();
            render_node_toolbar(&mut draft, controller, ui);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if editable {
                if small_outline_button(ui, "DELETE", "Delete prompt design").clicked() {
                    actions.push(PromptStudioAction::DeleteProfile(draft.id.clone()));
                }
            } else if small_outline_button(ui, "EDIT COPY", "Create an editable graph copy")
                .clicked()
            {
                let mut copy = PromptTemplateLibrary::editable_copy_of(
                    &draft,
                    format!("custom-{}", uuid::Uuid::new_v4()),
                );
                copy.name = format!("{} copy", draft.name);
                actions.push(PromptStudioAction::CloneProfile(copy.clone()));
                controller.set_draft(copy);
            }
            if validation_error.is_none()
                && draft.id != snapshot.active_id
                && style::command_button(ui, "ACTIVATE", true).clicked()
            {
                actions.push(PromptStudioAction::ActivateProfile(draft.clone()));
                controller.dirty = false;
            }
            if editable && small_outline_button(ui, "AUTO", "Automatically arrange nodes").clicked()
            {
                draft.graph.auto_layout();
                controller.fit_pending = true;
                controller.mark_dirty();
            }
            if editable && small_outline_button(ui, "FIT", "Fit graph to canvas").clicked() {
                controller.fit_pending = true;
            }
            if small_icon_button(ui, "-", "Zoom out").clicked() {
                controller.zoom = (controller.zoom - 0.1).clamp(0.25, 1.6);
            }
            if small_icon_button(ui, "+", "Zoom in").clicked() {
                controller.zoom = (controller.zoom + 0.1).clamp(0.25, 1.6);
            }
        });
    });
    if let Some(error) = &validation_error {
        ui.label(
            RichText::new(format!("INVALID GRAPH / {error}"))
                .font(egui::FontId::monospace(10.0))
                .color(Color32::from_rgb(232, 135, 119)),
        );
    }
    ui.add_space(4.0);

    Frame::new()
        .fill(style::CANVAS_FILL)
        .stroke(Stroke::new(1.0, style::CANVAS_BORDER))
        .corner_radius(CornerRadius::same(2))
        .inner_margin(Margin::same(5))
        .show(ui, |ui| {
            let canvas_height = ui.available_height().max(1.0);
            let (canvas, response) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), canvas_height),
                Sense::drag(),
            );
            controller.canvas_size = canvas.size();
            if editable && response.secondary_clicked() {
                controller.add_node_center = response.interact_pointer_pos().map(|pointer| {
                    let position = (pointer - canvas.min - controller.pan) / controller.zoom;
                    [position.x, position.y]
                });
            }
            if editable {
                let preferred_center = controller.add_node_center;
                response.context_menu(|ui| {
                    render_node_menu(&mut draft, controller, ui, preferred_center);
                });
            }
            if controller.fit_pending {
                fit_graph_to_canvas(&draft.graph, controller, canvas.size());
                controller.fit_pending = false;
            }
            let mut canvas_ui = canvas_viewport(ui, canvas);
            if response.dragged_by(egui::PointerButton::Middle) {
                controller.pan += canvas_ui.input(|input| input.pointer.delta());
            }
            if editable && response.drag_started_by(egui::PointerButton::Primary) {
                if let Some(pointer) = response.interact_pointer_pos() {
                    let over_node = draft
                        .graph
                        .nodes
                        .iter()
                        .filter(|node| controller.node_is_visible(node))
                        .any(|node| {
                            graph_rect(canvas, controller, node.position, node_size(node))
                                .contains(pointer)
                        });
                    if !over_node {
                        if !canvas_ui.input(|input| input.modifiers.shift || input.modifiers.ctrl) {
                            controller.selected_nodes.clear();
                        }
                        controller.box_select_start = Some(pointer);
                        controller.box_select_current = Some(pointer);
                    }
                }
            }
            if response.dragged_by(egui::PointerButton::Primary) {
                if controller.box_select_start.is_some() {
                    controller.box_select_current = response.interact_pointer_pos();
                }
            }
            if editable && response.drag_stopped_by(egui::PointerButton::Primary) {
                if let (Some(start), Some(current)) = (
                    controller.box_select_start.take(),
                    controller.box_select_current.take(),
                ) {
                    let selection = rect_between(start, current);
                    let active_provider = controller.active_provider;
                    for node in draft
                        .graph
                        .nodes
                        .iter()
                        .filter(|node| node.page.is_visible_on(active_provider))
                    {
                        if selection.intersects(graph_rect(
                            canvas,
                            controller,
                            node.position,
                            node_size(node),
                        )) {
                            controller.selected_nodes.insert(node.id.clone());
                        }
                    }
                }
            }
            if response.clicked_by(egui::PointerButton::Primary) {
                controller.selected_nodes.clear();
                controller.wire_from = None;
            }
            if response.hovered() {
                let scroll = canvas_ui.input(|input| input.smooth_scroll_delta.y);
                if scroll.abs() > f32::EPSILON {
                    let pointer = canvas_ui
                        .input(|input| input.pointer.hover_pos())
                        .unwrap_or(canvas.center());
                    let over_runtime_preview = draft
                        .graph
                        .nodes
                        .iter()
                        .filter(|node| controller.node_is_visible(node))
                        .any(|node| {
                            let rect =
                                graph_rect(canvas, controller, node.position, node_size(node));
                            node_scale(rect, node) >= 0.58
                                && runtime_preview::pane_rect(rect, node_scale(rect, node))
                                    .contains(pointer)
                        });
                    if !over_runtime_preview {
                        zoom_at_pointer(controller, canvas, pointer, scroll);
                    }
                }
            }
            if editable
                && !canvas_ui.ctx().egui_wants_keyboard_input()
                && canvas_ui.input(|input| {
                    input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace)
                })
            {
                let selected = controller.selected_nodes.drain().collect::<Vec<_>>();
                for id in &selected {
                    draft.graph.remove_node(&id);
                }
                if !selected.is_empty() {
                    controller.mark_dirty();
                }
            }
            if canvas_ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                controller.wire_from = None;
                controller.box_select_start = None;
                controller.box_select_current = None;
            }
            draw_canvas_background(&mut canvas_ui, canvas, controller);
            render_links(&mut canvas_ui, canvas, &mut draft, controller, editable);
            render_nodes(
                &mut canvas_ui,
                canvas,
                &mut draft,
                controller,
                editable,
                runtime_trace.as_ref(),
            );
            render_wire_preview(&mut canvas_ui, canvas, &draft, controller);
            render_selection_box(&mut canvas_ui, controller);
        });
    controller.draft = Some(draft);
}

fn canvas_viewport(parent: &mut egui::Ui, canvas: Rect) -> egui::Ui {
    let mut viewport = parent.new_child(
        UiBuilder::new()
            .max_rect(canvas)
            .layout(Layout::top_down(Align::Min)),
    );
    viewport.set_clip_rect(parent.clip_rect().intersect(canvas));
    viewport
}

fn render_provider_tabs(controller: &mut PromptStudioController, ui: &mut egui::Ui) {
    for (target, label) in [
        (PromptProviderTarget::OpenAiCompatible, "OPENAI"),
        (PromptProviderTarget::Hunyuan, "HUNYUAN"),
    ] {
        if style::provider_tab(ui, label, controller.active_provider == target).clicked() {
            controller.select_provider(target);
        }
    }
}

fn render_node_toolbar(
    draft: &mut PromptTemplateProfile,
    controller: &mut PromptStudioController,
    ui: &mut egui::Ui,
) {
    ui.menu_button("+ ADD NODE", |ui| {
        render_node_menu(draft, controller, ui, None);
    });
}

fn render_node_menu(
    draft: &mut PromptTemplateProfile,
    controller: &mut PromptStudioController,
    ui: &mut egui::Ui,
    preferred_center: Option<[f32; 2]>,
) {
    let page = PromptNodePage::for_target(controller.active_provider);
    ui.menu_button("Input", |ui| {
        ui.label(RichText::new("RUNTIME VALUES").small().strong());
        for (label, variable) in [
            ("Source language", PromptVariable::SourceLanguage),
            ("Target language", PromptVariable::TargetLanguage),
            ("Current input", PromptVariable::CurrentInput),
        ] {
            if ui.button(label).clicked() {
                let position = node_add_position(controller, &draft.graph, preferred_center);
                let id = draft.graph.add_variable(page, variable, position);
                finish_node_add(draft, controller, id);
                ui.close();
            }
        }
        ui.separator();
        ui.label(RichText::new("REFERENCE DATA").small().strong());
        for (label, block) in available_blocks() {
            if ui.button(label).clicked() {
                let position = node_add_position(controller, &draft.graph, preferred_center);
                let id = draft.graph.add_input(page, block, position);
                finish_node_add(draft, controller, id);
                ui.close();
            }
        }
    });
    ui.menu_button("Logic", |ui| {
        for (label, condition) in [
            ("Source is auto", PromptCondition::SourceIsAuto),
            (
                "Has reference context",
                PromptCondition::HasReferenceContext,
            ),
        ] {
            if ui.button(label).clicked() {
                let position = node_add_position(controller, &draft.graph, preferred_center);
                let id = draft.graph.add_switch(page, condition, position);
                finish_node_add(draft, controller, id);
                ui.close();
            }
        }
    });
    ui.menu_button("Compose", |ui| {
        if ui
            .button("Compose")
            .on_hover_text("Arrange fixed text and connected {0}-{4} input slots")
            .clicked()
        {
            let position = node_add_position(controller, &draft.graph, preferred_center);
            let id = draft
                .graph
                .add_compose(page, "Write prompt text here: {0}".into(), position);
            finish_node_add(draft, controller, id);
            ui.close();
        }
    });
    ui.menu_button("Request", |ui| {
        let target = controller.active_provider;
        let exists = draft.graph.nodes.iter().any(
            |node| matches!(node.kind, PromptNodeKind::Request { target: value, .. } if value == target),
        );
        let (label, roles) = match target {
            PromptProviderTarget::OpenAiCompatible => (
                "OpenAI request",
                vec![PromptMessageRole::System, PromptMessageRole::User],
            ),
            PromptProviderTarget::Hunyuan => {
                ("Hunyuan request", vec![PromptMessageRole::User])
            }
        };
        if ui
            .add_enabled(!exists, egui::Button::new(label))
            .on_disabled_hover_text("This provider page already has its API request")
            .clicked()
        {
            let position = node_add_position(controller, &draft.graph, preferred_center);
            let id = draft.graph.add_request(target, roles, position);
            finish_node_add(draft, controller, id);
            ui.close();
        }
    });
}

fn node_add_position(
    controller: &PromptStudioController,
    graph: &PromptNodeGraph,
    preferred_center: Option<[f32; 2]>,
) -> [f32; 2] {
    preferred_center.map_or_else(
        || controller.new_node_position(graph),
        |center| controller.new_node_position_near(graph, center),
    )
}

fn finish_node_add(
    draft: &mut PromptTemplateProfile,
    controller: &mut PromptStudioController,
    id: String,
) {
    draft.graph.layout_version = 0;
    controller.selected_nodes.clear();
    controller.selected_nodes.insert(id);
    controller.mark_dirty();
}

fn draw_canvas_background(ui: &egui::Ui, canvas: Rect, controller: &PromptStudioController) {
    let painter = ui.painter();
    let grid = (32.0 * controller.zoom).max(8.0);
    let color = style::GRID;
    let mut x = canvas.left() + controller.pan.x.rem_euclid(grid);
    while x <= canvas.right() {
        painter.line_segment(
            [Pos2::new(x, canvas.top()), Pos2::new(x, canvas.bottom())],
            Stroke::new(1.0, color),
        );
        x += grid;
    }
    let mut y = canvas.top() + controller.pan.y.rem_euclid(grid);
    while y <= canvas.bottom() {
        painter.line_segment(
            [Pos2::new(canvas.left(), y), Pos2::new(canvas.right(), y)],
            Stroke::new(1.0, color),
        );
        y += grid;
    }
}

fn render_links(
    ui: &mut egui::Ui,
    canvas: Rect,
    profile: &mut PromptTemplateProfile,
    controller: &mut PromptStudioController,
    editable: bool,
) {
    let links = profile.graph.links.clone();
    let mut remove_link = None;
    for (index, link) in links.iter().enumerate() {
        let endpoints_visible = profile
            .graph
            .nodes
            .iter()
            .filter(|node| node.id == link.from || node.id == link.to)
            .all(|node| controller.node_is_visible(node));
        if !endpoints_visible {
            continue;
        }
        let Some((from, to)) = link_points(canvas, controller, &profile.graph, link) else {
            continue;
        };
        let points = bezier_points(from, to);
        let bounds = points
            .iter()
            .fold(Rect::from_min_max(points[0], points[0]), |rect, point| {
                Rect::from_min_max(
                    Pos2::new(rect.left().min(point.x), rect.top().min(point.y)),
                    Pos2::new(rect.right().max(point.x), rect.bottom().max(point.y)),
                )
            });
        let hit = ui.interact(
            bounds.expand(8.0),
            ui.make_persistent_id(("prompt_link", index)),
            if editable {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let pointer_near = ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|pointer| pointer_near_curve(pointer, points, 9.0));
        if editable && pointer_near && hit.secondary_clicked() {
            remove_link = Some(index);
        }
        let source_color = profile
            .graph
            .nodes
            .iter()
            .find(|node| node.id == link.from)
            .map(|node| node_palette(&node.kind).connector)
            .unwrap_or(GRAPH_ACCENT);
        let wire_color = if pointer_near {
            style::GRAPH_ACCENT
        } else {
            Color32::from_rgba_unmultiplied(
                source_color.r(),
                source_color.g(),
                source_color.b(),
                178,
            )
        };
        ui.painter().add(egui::Shape::CubicBezier(
            egui::epaint::CubicBezierShape::from_points_stroke(
                points,
                false,
                Color32::TRANSPARENT,
                Stroke::new(if pointer_near { 2.6 } else { 1.6 }, wire_color),
            ),
        ));
    }
    if let Some(index) = remove_link {
        profile.graph.links.remove(index);
        controller.mark_dirty();
    }
}

fn render_nodes(
    ui: &mut egui::Ui,
    canvas: Rect,
    profile: &mut PromptTemplateProfile,
    controller: &mut PromptStudioController,
    editable: bool,
    runtime_trace: Option<&PromptExecutionTrace>,
) {
    let nodes = profile
        .graph
        .nodes
        .iter()
        .filter(|node| controller.node_is_visible(node))
        .cloned()
        .collect::<Vec<_>>();
    let mut remove_id = None;
    for node in nodes {
        let rect = graph_rect(canvas, controller, node.position, node_size(&node));
        let header = Rect::from_min_size(
            rect.min,
            Vec2::new(rect.width(), NODE_HEADER_HEIGHT * node_scale(rect, &node)),
        );
        let response = ui.interact(
            header,
            ui.make_persistent_id(("prompt_node", &node.id)),
            if editable {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            },
        );
        let response = if matches!(node.kind, PromptNodeKind::Request { .. }) {
            let preview = profile
                .graph
                .compose_request_preview(&node.id)
                .unwrap_or_else(|| "(no connected messages)".into());
            response.on_hover_text(format!(
                "API REQUEST PREVIEW\n\n{}",
                truncate_preview(&preview, 1200)
            ))
        } else {
            response
        };
        if editable && response.clicked() {
            let extend = ui.input(|input| input.modifiers.shift || input.modifiers.ctrl);
            if extend {
                if !controller.selected_nodes.insert(node.id.clone()) {
                    controller.selected_nodes.remove(&node.id);
                }
            } else {
                controller.selected_nodes.clear();
                controller.selected_nodes.insert(node.id.clone());
            }
        }
        if editable && response.drag_started() {
            if !controller.selected_nodes.contains(&node.id) {
                controller.selected_nodes.clear();
                controller.selected_nodes.insert(node.id.clone());
            }
            controller.drag_node = Some(node.id.clone());
            controller.drag_origins = profile
                .graph
                .nodes
                .iter()
                .filter(|candidate| controller.selected_nodes.contains(&candidate.id))
                .map(|candidate| (candidate.id.clone(), candidate.position))
                .collect();
        }
        if editable
            && response.dragged()
            && controller.drag_node.as_deref() == Some(node.id.as_str())
        {
            let delta = response.drag_delta() / controller.zoom;
            for target in &mut profile.graph.nodes {
                if let Some(origin) = controller.drag_origins.get(&target.id) {
                    target.position = [origin[0] + delta.x, origin[1] + delta.y];
                }
            }
            controller.mark_dirty();
        }
        if editable
            && response.drag_stopped()
            && controller.drag_node.as_deref() == Some(node.id.as_str())
        {
            for target in &mut profile.graph.nodes {
                if controller.selected_nodes.contains(&target.id) {
                    target.position[0] = (target.position[0] / 16.0).round() * 16.0;
                    target.position[1] = (target.position[1] / 16.0).round() * 16.0;
                }
            }
            controller.drag_node = None;
            controller.drag_origins.clear();
        }
        let scale = node_scale(rect, &node);
        let close_rect = Rect::from_center_size(
            Pos2::new(rect.right() - 13.0 * scale, rect.top() + 13.0 * scale),
            Vec2::splat((22.0 * scale).max(14.0)),
        );
        if editable
            && ui
                .interact(
                    close_rect,
                    ui.make_persistent_id(("prompt_node_remove", &node.id)),
                    Sense::click(),
                )
                .clicked()
        {
            remove_id = Some(node.id.clone());
        }
        let selected = controller.selected_nodes.contains(&node.id);
        draw_node(
            ui,
            rect,
            &node,
            profile,
            controller,
            editable,
            selected,
            runtime_trace,
        );
        render_node_sockets(ui, rect, &node, profile, controller, editable);
    }
    if let Some(id) = remove_id {
        profile.graph.remove_node(&id);
        controller.selected_nodes.remove(&id);
        controller.mark_dirty();
    }
}

fn draw_node(
    ui: &mut egui::Ui,
    rect: Rect,
    node: &PromptNode,
    profile: &mut PromptTemplateProfile,
    controller: &mut PromptStudioController,
    editable: bool,
    selected: bool,
    runtime_trace: Option<&PromptExecutionTrace>,
) {
    let scale = node_scale(rect, node);
    let header_height = NODE_HEADER_HEIGHT * scale;
    let palette = node_palette(&node.kind);
    let title = node_display_label(node);
    ui.painter()
        .rect_filled(rect, CornerRadius::same(2), palette.fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected {
                GRAPH_ACCENT
            } else {
                style::NODE_BORDER
            },
        ),
        egui::epaint::StrokeKind::Inside,
    );
    ui.painter().rect_filled(
        Rect::from_min_size(rect.min, Vec2::new(rect.width(), header_height)),
        CornerRadius::same(2),
        palette.header,
    );
    let show_kind = scale >= 0.72;
    let kind_tag = node_kind_tag(&profile.graph, node);
    let title_font_size = (9.5 * scale).max(7.0);
    let kind_font_size = 7.5 * scale;
    let kind_width = if show_kind {
        kind_tag.chars().count() as f32 * kind_font_size * 0.62 + 10.0 * scale
    } else {
        0.0
    };
    let close_width = if editable { 22.0 * scale } else { 0.0 };
    let title_width =
        (rect.width() - 18.0 * scale - kind_width - close_width).max(title_font_size * 8.0);
    let title_chars = (title_width / (title_font_size * 0.62)).floor() as usize;
    if editable && matches!(node.kind, PromptNodeKind::Compose { .. }) {
        let mut edited_title = if node.label.trim().is_empty() || node.label == "COMPOSE TEXT" {
            title
        } else {
            node.label.clone()
        };
        let title_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 7.0 * scale, rect.top() + 2.0 * scale),
            Vec2::new(title_width, (header_height - 4.0 * scale).max(12.0)),
        );
        let changed = ui
            .put(
                title_rect,
                egui::TextEdit::singleline(&mut edited_title)
                    .font(egui::FontId::monospace(title_font_size))
                    .text_color(style::NODE_TEXT)
                    .desired_width(title_width)
                    .frame(egui::Frame::NONE),
            )
            .on_hover_text("Rename this Compose node")
            .changed();
        if changed {
            if let Some(actual) = profile
                .graph
                .nodes
                .iter_mut()
                .find(|actual| actual.id == node.id)
            {
                actual.label = edited_title;
                controller.mark_dirty();
            }
        }
    } else {
        ui.painter().text(
            Pos2::new(rect.left() + 10.0 * scale, rect.top() + 8.0 * scale),
            egui::Align2::LEFT_TOP,
            truncate_preview(&title, title_chars),
            egui::FontId::monospace(title_font_size),
            style::NODE_TEXT,
        );
    }
    if show_kind {
        ui.painter().text(
            Pos2::new(
                rect.right() - (if editable { 28.0 } else { 8.0 }) * scale,
                rect.top() + 9.0 * scale,
            ),
            egui::Align2::RIGHT_TOP,
            kind_tag,
            egui::FontId::monospace(kind_font_size),
            style::NODE_MUTED,
        );
    }
    if editable {
        ui.painter().text(
            Pos2::new(rect.right() - 8.0 * scale, rect.top() + 8.0 * scale),
            egui::Align2::RIGHT_TOP,
            "×",
            egui::FontId::monospace(12.0 * scale),
            style::NODE_TEXT,
        );
    }
    if scale < 0.58 {
        return;
    }
    match &node.kind {
        PromptNodeKind::Input {
            block: TranslationPromptBlock::CustomText { .. },
        } if editable => {
            if let Some(actual) = profile
                .graph
                .nodes
                .iter_mut()
                .find(|actual| actual.id == node.id)
            {
                if let PromptNodeKind::Input {
                    block: TranslationPromptBlock::CustomText { text },
                } = &mut actual.kind
                {
                    let body = Rect::from_min_max(
                        Pos2::new(rect.left() + 9.0 * scale, rect.top() + 34.0 * scale),
                        Pos2::new(
                            runtime_preview::configuration_right(rect, scale),
                            rect.bottom() - 6.0 * scale,
                        ),
                    );
                    ui.scope_builder(
                        UiBuilder::new()
                            .max_rect(body)
                            .layout(Layout::top_down(Align::Min)),
                        |ui| {
                            ui.set_clip_rect(ui.clip_rect().intersect(body));
                            if ui
                                .add(
                                    egui::TextEdit::multiline(text)
                                        .font(egui::FontId::monospace(10.0 * scale))
                                        .text_color(style::NODE_TEXT)
                                        .desired_rows(
                                            ((node.layout_height() - 54.0) / 13.0).floor().max(3.0)
                                                as usize,
                                        )
                                        .frame(egui::Frame::NONE),
                                )
                                .changed()
                            {
                                profile.graph.layout_version = 0;
                                controller.mark_dirty();
                            }
                        },
                    );
                }
            }
        }
        PromptNodeKind::Input { .. } => {
            ui.painter().text(
                Pos2::new(rect.left() + 10.0 * scale, rect.top() + 47.0 * scale),
                egui::Align2::LEFT_TOP,
                input_description(&node.kind),
                egui::FontId::monospace(10.0 * scale),
                style::NODE_TEXT,
            );
        }
        PromptNodeKind::Variable { variable } => {
            ui.painter().text(
                Pos2::new(rect.left() + 10.0 * scale, rect.top() + 47.0 * scale),
                egui::Align2::LEFT_TOP,
                format!("[{}]", variable_name(*variable)),
                egui::FontId::monospace(10.0 * scale),
                style::NODE_TEXT,
            );
        }
        PromptNodeKind::Compose { .. } if editable => {
            let mut changed = false;
            if let Some(actual) = profile
                .graph
                .nodes
                .iter_mut()
                .find(|actual| actual.id == node.id)
            {
                if let PromptNodeKind::Compose { text } = &mut actual.kind {
                    let body = Rect::from_min_max(
                        Pos2::new(rect.left() + 30.0 * scale, rect.top() + 34.0 * scale),
                        Pos2::new(
                            runtime_preview::configuration_right(rect, scale),
                            rect.bottom() - 6.0 * scale,
                        ),
                    );
                    ui.scope_builder(
                        UiBuilder::new()
                            .max_rect(body)
                            .layout(Layout::top_down(Align::Min)),
                        |ui| {
                            ui.set_clip_rect(ui.clip_rect().intersect(body));
                            if ui
                                .add(
                                    egui::TextEdit::multiline(text)
                                        .font(egui::FontId::monospace(10.0 * scale))
                                        .text_color(style::NODE_TEXT)
                                        .desired_rows(
                                            ((node.layout_height() - 54.0) / 13.0).floor().max(5.0)
                                                as usize,
                                        )
                                        .frame(egui::Frame::NONE),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        },
                    );
                }
            }
            if changed {
                let inputs = profile
                    .graph
                    .nodes
                    .iter()
                    .find(|actual| actual.id == node.id)
                    .and_then(|actual| match &actual.kind {
                        PromptNodeKind::Compose { text } => compose_input_indexes(text).ok(),
                        _ => None,
                    })
                    .unwrap_or_default();
                profile
                    .graph
                    .links
                    .retain(|link| link.to != node.id || inputs.contains(&link.input));
                profile.graph.layout_version = 0;
                controller.mark_dirty();
            }
        }
        PromptNodeKind::Compose { text } => {
            let body = Rect::from_min_max(
                Pos2::new(rect.left() + 30.0 * scale, rect.top() + 35.0 * scale),
                Pos2::new(
                    runtime_preview::configuration_right(rect, scale),
                    rect.bottom() - 6.0 * scale,
                ),
            );
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(body)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(body));
                    ui.label(
                        RichText::new(text.as_str())
                            .font(egui::FontId::monospace(10.0 * scale))
                            .color(style::NODE_TEXT),
                    );
                },
            );
        }
        PromptNodeKind::Switch { condition } => {
            ui.painter().text(
                Pos2::new(rect.left() + 18.0 * scale, rect.bottom() - 18.0 * scale),
                egui::Align2::LEFT_BOTTOM,
                condition_expression(*condition),
                egui::FontId::monospace(9.0 * scale),
                style::NODE_TEXT,
            );
        }
        PromptNodeKind::Request { roles, .. } => {
            ui.painter().text(
                Pos2::new(rect.left() + 12.0 * scale, rect.bottom() - 12.0 * scale),
                egui::Align2::LEFT_BOTTOM,
                request_summary(roles.len()),
                egui::FontId::monospace(9.0 * scale),
                style::NODE_TEXT,
            );
        }
    }
    runtime_preview::render(ui, rect, node, runtime_trace, scale);
}

fn render_node_sockets(
    ui: &mut egui::Ui,
    rect: Rect,
    node: &PromptNode,
    profile: &mut PromptTemplateProfile,
    controller: &mut PromptStudioController,
    editable: bool,
) {
    let scale = node_scale(rect, node);
    if !matches!(node.kind, PromptNodeKind::Request { .. }) {
        let output = socket_position(&profile.graph, rect, node, false, 0);
        let output_response = ui.interact(
            Rect::from_center_size(output, Vec2::splat(22.0)),
            ui.make_persistent_id(("prompt_output_socket", &node.id)),
            if editable {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if editable && (output_response.clicked() || output_response.drag_started()) {
            controller.wire_from = Some(node.id.clone());
        }
        ui.painter().circle_filled(
            output,
            (SOCKET_RADIUS * scale).max(2.5),
            if controller.wire_from.as_deref() == Some(node.id.as_str()) {
                style::GRAPH_ACCENT
            } else {
                node_palette(&node.kind).connector
            },
        );
    }

    let inputs = input_socket_indexes(&profile.graph, node);
    if !inputs.is_empty() {
        for input in inputs {
            let position = socket_position(&profile.graph, rect, node, true, input);
            let input_response = ui
                .interact(
                    Rect::from_center_size(position, Vec2::splat(22.0)),
                    ui.make_persistent_id(("prompt_input_socket", &node.id, input)),
                    if editable {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                )
                .on_hover_text(input_socket_tooltip(&profile.graph, node, input));
            if editable && (input_response.clicked() || input_response.drag_stopped()) {
                if let Some(from) = controller.finish_wire() {
                    if profile.graph.connect(&from, &node.id, input) {
                        controller.mark_dirty();
                    }
                }
            }
            ui.painter().circle_filled(
                position,
                (SOCKET_RADIUS * scale).max(2.5),
                style::NODE_MUTED,
            );
            if matches!(
                node.kind,
                PromptNodeKind::Compose { .. }
                    | PromptNodeKind::Switch { .. }
                    | PromptNodeKind::Request { .. }
            ) {
                ui.painter().text(
                    Pos2::new(position.x + 12.0 * scale, position.y - 6.0 * scale),
                    egui::Align2::LEFT_TOP,
                    input_socket_label(node, input),
                    egui::FontId::monospace((9.0 * scale).max(6.5)),
                    style::NODE_TEXT,
                );
            }
        }
    }
}

fn render_wire_preview(
    ui: &mut egui::Ui,
    canvas: Rect,
    profile: &PromptTemplateProfile,
    controller: &PromptStudioController,
) {
    let Some(from_id) = controller.wire_from.as_deref() else {
        return;
    };
    let Some(node) = profile.graph.nodes.iter().find(|node| node.id == from_id) else {
        return;
    };
    let from = socket_position(
        &profile.graph,
        graph_rect(canvas, controller, node.position, node_size(node)),
        node,
        false,
        0,
    );
    let to = ui
        .ctx()
        .pointer_hover_pos()
        .map(|position| position.clamp(canvas.min, canvas.max))
        .unwrap_or_else(|| Pos2::new(from.x + 100.0, from.y));
    let points = bezier_points(from, to);
    ui.painter().add(egui::Shape::CubicBezier(
        egui::epaint::CubicBezierShape::from_points_stroke(
            points,
            false,
            Color32::TRANSPARENT,
            Stroke::new(2.0, GRAPH_ACCENT),
        ),
    ));
}

fn link_points(
    canvas: Rect,
    controller: &PromptStudioController,
    graph: &PromptNodeGraph,
    link: &PromptLink,
) -> Option<(Pos2, Pos2)> {
    let from = graph.nodes.iter().find(|node| node.id == link.from)?;
    let to = graph.nodes.iter().find(|node| node.id == link.to)?;
    Some((
        socket_position(
            graph,
            graph_rect(canvas, controller, from.position, node_size(from)),
            from,
            false,
            0,
        ),
        socket_position(
            graph,
            graph_rect(canvas, controller, to.position, node_size(to)),
            to,
            true,
            link.input,
        ),
    ))
}

fn render_selection_box(ui: &egui::Ui, controller: &PromptStudioController) {
    let (Some(start), Some(current)) = (controller.box_select_start, controller.box_select_current)
    else {
        return;
    };
    let selection = rect_between(start, current);
    ui.painter().rect_filled(
        selection,
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(GRAPH_ACCENT.r(), GRAPH_ACCENT.g(), GRAPH_ACCENT.b(), 24),
    );
    ui.painter().rect_stroke(
        selection,
        CornerRadius::ZERO,
        Stroke::new(1.0, GRAPH_ACCENT),
        egui::epaint::StrokeKind::Inside,
    );
}

fn rect_between(first: Pos2, second: Pos2) -> Rect {
    Rect::from_min_max(
        Pos2::new(first.x.min(second.x), first.y.min(second.y)),
        Pos2::new(first.x.max(second.x), first.y.max(second.y)),
    )
}

fn graph_rect(
    canvas: Rect,
    controller: &PromptStudioController,
    position: [f32; 2],
    size: Vec2,
) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            canvas.left() + controller.pan.x + position[0] * controller.zoom,
            canvas.top() + controller.pan.y + position[1] * controller.zoom,
        ),
        size * controller.zoom,
    )
}

fn fit_graph_to_canvas(
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

fn socket_position(
    graph: &PromptNodeGraph,
    rect: Rect,
    node: &PromptNode,
    input: bool,
    index: u8,
) -> Pos2 {
    let scale = node_scale(rect, node);
    if input {
        if matches!(
            node.kind,
            PromptNodeKind::Compose { .. }
                | PromptNodeKind::Switch { .. }
                | PromptNodeKind::Request { .. }
        ) {
            let row = input_socket_indexes(graph, node)
                .iter()
                .position(|value| *value == index)
                .unwrap_or_default();
            return Pos2::new(
                rect.left(),
                rect.top() + (NODE_HEADER_HEIGHT + 22.0 + row as f32 * 25.0) * scale,
            );
        }
        return Pos2::new(rect.left(), rect.center().y);
    }
    Pos2::new(rect.right(), rect.center().y)
}

fn node_scale(rect: Rect, node: &PromptNode) -> f32 {
    rect.width() / node_size(node).x
}

fn input_socket_indexes(graph: &PromptNodeGraph, node: &PromptNode) -> Vec<u8> {
    match &node.kind {
        PromptNodeKind::Compose { .. } => graph.compose_input_socket_indexes(&node.id),
        PromptNodeKind::Switch { .. } => vec![0, 1],
        PromptNodeKind::Request { roles, .. } => (0..roles.len() as u8).collect(),
        _ => Vec::new(),
    }
}

fn zoom_at_pointer(
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

fn bezier_points(from: Pos2, to: Pos2) -> [Pos2; 4] {
    let dx = ((to.x - from.x).abs() * 0.5).max(48.0);
    [
        from,
        Pos2::new(from.x + dx, from.y),
        Pos2::new(to.x - dx, to.y),
        to,
    ]
}

fn cubic_point(points: [Pos2; 4], t: f32) -> Pos2 {
    let a = points[0].lerp(points[1], t);
    let b = points[1].lerp(points[2], t);
    let c = points[2].lerp(points[3], t);
    a.lerp(b, t).lerp(b.lerp(c, t), t)
}

fn pointer_near_curve(pointer: Pos2, points: [Pos2; 4], threshold: f32) -> bool {
    let mut previous = points[0];
    for step in 1..=24 {
        let next = cubic_point(points, step as f32 / 24.0);
        if distance_to_segment(pointer, previous, next) <= threshold {
            return true;
        }
        previous = next;
    }
    false
}

fn distance_to_segment(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_sq();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let t = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panned_canvas_controls_cannot_expand_the_parent_layout() {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let (canvas, _) = ui.allocate_exact_size(Vec2::new(240.0, 160.0), Sense::hover());
            let parent_layout = ui.min_rect();
            let mut text = String::from("offscreen node editor");

            {
                let mut viewport = canvas_viewport(ui, canvas);
                let offscreen = Rect::from_min_size(
                    canvas.max + Vec2::new(10_000.0, 10_000.0),
                    Vec2::new(400.0, 200.0),
                );
                viewport.put(offscreen, egui::TextEdit::multiline(&mut text));
            }

            assert_eq!(ui.min_rect(), parent_layout);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn fit_keeps_complete_node_bounds_inside_the_canvas() {
        let mut graph = PromptNodeGraph::empty();
        graph.add_variable(
            PromptNodePage::OpenAiCompatible,
            PromptVariable::CurrentInput,
            [100.0, 100.0],
        );
        graph.add_request(
            PromptProviderTarget::OpenAiCompatible,
            vec![PromptMessageRole::System, PromptMessageRole::User],
            [700.0, 500.0],
        );
        let available = Vec2::new(1000.0, 600.0);
        let canvas = Rect::from_min_size(Pos2::ZERO, available);
        let mut controller = PromptStudioController::default();

        fit_graph_to_canvas(&graph, &mut controller, available);

        for node in &graph.nodes {
            let rect = graph_rect(canvas, &controller, node.position, node_size(node));
            assert!(canvas.contains(rect.min));
            assert!(canvas.contains(rect.max));
        }
    }
}
