use std::collections::BTreeMap;

use egui_commonmark::CommonMarkCache;

use crate::{
    AppAction, ScriptDialogContent, ScriptDialogElement, ScriptDialogId, ScriptDialogKind,
    ScriptDialogState,
};

#[derive(Default)]
struct DialogPresentationState {
    open: bool,
    last_open_request: u64,
    page: usize,
}

#[derive(Default)]
pub struct ScriptDialogs {
    states: BTreeMap<ScriptDialogId, DialogPresentationState>,
    markdown_caches: BTreeMap<(ScriptDialogId, usize, usize), CommonMarkCache>,
    #[cfg(test)]
    control_rect: Option<egui::Rect>,
    #[cfg(test)]
    entry_rects: BTreeMap<ScriptDialogId, egui::Rect>,
    #[cfg(test)]
    button_rects: BTreeMap<(ScriptDialogId, crate::ScriptDialogButtonId), egui::Rect>,
    #[cfg(test)]
    link_rects: BTreeMap<(ScriptDialogId, crate::ScriptDialogButtonId), egui::Rect>,
    #[cfg(test)]
    next_rects: BTreeMap<ScriptDialogId, egui::Rect>,
}

impl ScriptDialogs {
    pub fn show_control(&mut self, ui: &mut egui::Ui, dialogs: &[ScriptDialogState]) {
        self.synchronize(dialogs);
        if dialogs.is_empty() {
            #[cfg(test)]
            {
                self.control_rect = None;
            }
            return;
        }
        let count = dialogs.len();
        let label = if count == 1 {
            "1 Script Dialog".to_owned()
        } else {
            format!("{count} Script Dialogs")
        };
        let _menu = ui.menu_button(label, |ui| {
            for dialog in dialogs {
                let duplicate = dialogs
                    .iter()
                    .filter(|candidate| candidate.name == dialog.name)
                    .count()
                    > 1;
                let label = if duplicate {
                    format!("{} — {}", dialog.name, dialog.owner_script_name)
                } else {
                    dialog.name.clone()
                };
                let response = ui.button(label);
                #[cfg(test)]
                self.entry_rects.insert(dialog.id, response.rect);
                if response.clicked() {
                    if let Some(state) = self.states.get_mut(&dialog.id) {
                        state.open = true;
                    }
                    ui.close();
                }
            }
        });
        #[cfg(test)]
        {
            self.control_rect = Some(_menu.response.rect);
        }
    }

    pub fn show_windows(
        &mut self,
        context: &egui::Context,
        dialogs: &[ScriptDialogState],
        script_paths: Option<&BTreeMap<crate::ScriptId, String>>,
    ) -> Vec<AppAction> {
        self.synchronize(dialogs);
        let mut actions = Vec::new();
        for dialog in dialogs {
            let Some(state) = self.states.get(&dialog.id) else {
                continue;
            };
            if !state.open {
                continue;
            }
            let mut open = state.open;
            let mut page = state.page;
            let script_path = script_paths
                .and_then(|paths| paths.get(&dialog.owner_script_id))
                .map(String::as_str)
                .unwrap_or(&dialog.owner_script_name);
            egui::Window::new(&dialog.name)
                .id(egui::Id::new(("script_dialog", dialog.id.raw())))
                .open(&mut open)
                .resizable(true)
                .default_width(420.0)
                .default_height(220.0)
                .show(context, |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "owned by {}",
                                    dialog.owner_script_name
                                ))
                                .italics()
                                .weak(),
                            );
                        },
                    );
                    ui.separator();
                    match &dialog.kind {
                        ScriptDialogKind::Simple(content) => {
                            egui::ScrollArea::vertical()
                                .id_salt(("script_dialog_content", dialog.id.raw()))
                                .show(ui, |ui| {
                                    show_content(
                                        ui,
                                        dialog.owner_script_id,
                                        script_path,
                                        dialog.id,
                                        content,
                                        0,
                                        &mut actions,
                                        self,
                                    )
                                });
                        }
                        ScriptDialogKind::Paged(pages) => {
                            page = page.min(pages.len().saturating_sub(1));
                            let content_height = (ui.available_height() - 40.0).max(80.0);
                            egui::ScrollArea::vertical()
                                .id_salt(("script_dialog_page", dialog.id.raw(), page))
                                .max_height(content_height)
                                .show(ui, |ui| {
                                    if let Some(content) = pages.get(page) {
                                        show_content(
                                            ui,
                                            dialog.owner_script_id,
                                            script_path,
                                            dialog.id,
                                            content,
                                            page + 1,
                                            &mut actions,
                                            self,
                                        );
                                    }
                                });
                            ui.separator();
                            show_page_control(ui, dialog.id, &mut page, pages.len(), self);
                        }
                    }
                });
            if let Some(state) = self.states.get_mut(&dialog.id) {
                state.open = open;
                state.page = page;
            }
        }
        actions
    }

    #[cfg(target_arch = "wasm32")]
    pub fn browser_test_state(&self, id: ScriptDialogId) -> Option<(bool, usize)> {
        self.states.get(&id).map(|state| (state.open, state.page))
    }

    #[cfg(target_arch = "wasm32")]
    pub fn browser_test_close(&mut self, id: ScriptDialogId) {
        if let Some(state) = self.states.get_mut(&id) {
            state.open = false;
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn browser_test_open_from_list(&mut self, id: ScriptDialogId) {
        if let Some(state) = self.states.get_mut(&id) {
            state.open = true;
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn browser_test_set_page(&mut self, id: ScriptDialogId, page: usize) {
        if let Some(state) = self.states.get_mut(&id) {
            state.page = page;
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn browser_test_count(&self) -> usize {
        self.states.len()
    }

    fn synchronize(&mut self, dialogs: &[ScriptDialogState]) {
        self.states
            .retain(|id, _| dialogs.iter().any(|dialog| dialog.id == *id));
        self.markdown_caches
            .retain(|(id, _, _), _| dialogs.iter().any(|dialog| dialog.id == *id));
        for dialog in dialogs {
            let state = self
                .states
                .entry(dialog.id)
                .or_insert_with(|| DialogPresentationState {
                    open: dialog.open_request > 0,
                    last_open_request: dialog.open_request,
                    page: 0,
                });
            if state.last_open_request != dialog.open_request {
                state.open = true;
                state.last_open_request = dialog.open_request;
            }
            if let ScriptDialogKind::Paged(pages) = &dialog.kind {
                state.page = state.page.min(pages.len().saturating_sub(1));
            } else {
                state.page = 0;
            }
        }
    }
}

fn show_page_control(
    ui: &mut egui::Ui,
    _dialog_id: ScriptDialogId,
    page: &mut usize,
    page_count: usize,
    _dialogs: &mut ScriptDialogs,
) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(*page > 0, egui::Button::new("Previous"))
            .clicked()
        {
            *page -= 1;
        }
        ui.label(format!("{} / {}", *page + 1, page_count));
        let next = ui.add_enabled(*page + 1 < page_count, egui::Button::new("Next"));
        #[cfg(test)]
        _dialogs.next_rects.insert(_dialog_id, next.rect);
        if next.clicked() {
            *page += 1;
        }
    });
}

fn show_content(
    ui: &mut egui::Ui,
    owner_script_id: crate::ScriptId,
    script_path: &str,
    dialog_id: ScriptDialogId,
    content: &ScriptDialogContent,
    content_index: usize,
    actions: &mut Vec<AppAction>,
    _dialogs: &mut ScriptDialogs,
) {
    ui.vertical(|ui| {
        for (element_index, element) in content.elements.iter().enumerate() {
            match element {
                ScriptDialogElement::RichText { text, style } => {
                    let mut text = egui::RichText::new(text);
                    if style.strong {
                        text = text.strong();
                    }
                    if style.italics {
                        text = text.italics();
                    }
                    if style.monospace {
                        text = text.monospace();
                    }
                    if style.underline {
                        text = text.underline();
                    }
                    if style.strikethrough {
                        text = text.strikethrough();
                    }
                    ui.add(egui::Label::new(text).wrap());
                }
                ScriptDialogElement::Markdown { text, links } => {
                    let (_response, clicked) = {
                        let cache = _dialogs
                            .markdown_caches
                            .entry((dialog_id, content_index, element_index))
                            .or_default();
                        for link in links.iter() {
                            cache.add_link_hook(&link.destination);
                        }
                        let response =
                            crate::script_markdown_viewer(script_path).show(ui, cache, text);
                        let clicked = links
                            .iter()
                            .filter(|link| cache.get_link_hook(&link.destination) == Some(true))
                            .map(|link| link.callback_id)
                            .collect::<Vec<_>>();
                        (response, clicked)
                    };
                    for callback_id in clicked {
                        actions.push(button_action(owner_script_id, dialog_id, callback_id));
                    }
                    #[cfg(test)]
                    for link in links.iter() {
                        _dialogs
                            .link_rects
                            .insert((dialog_id, link.callback_id), _response.response.rect);
                    }
                }
                ScriptDialogElement::Button { id, label } => {
                    let response = ui.button(label);
                    if let Some(button_id) = id {
                        #[cfg(test)]
                        _dialogs
                            .button_rects
                            .insert((dialog_id, *button_id), response.rect);
                        if response.clicked() {
                            actions.push(button_action(owner_script_id, dialog_id, *button_id));
                        }
                    }
                }
            }
        }
    });
}

fn button_action(
    script_id: crate::ScriptId,
    dialog_id: ScriptDialogId,
    button_id: crate::ScriptDialogButtonId,
) -> AppAction {
    AppAction::InvokeScriptDialogButton {
        script_id,
        dialog_id,
        button_id,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        ScriptDialogButtonId, ScriptDialogElement, ScriptDialogMarkdownLink,
        ScriptDialogRichTextStyle, ScriptId,
    };

    fn simple(id: u64, owner: u64, name: &str, open_request: u64) -> ScriptDialogState {
        ScriptDialogState {
            id: ScriptDialogId::from_raw(id),
            owner_script_id: ScriptId::from_raw(owner),
            owner_script_name: format!("owner-{owner}.lua"),
            name: name.to_owned(),
            kind: ScriptDialogKind::Simple(ScriptDialogContent {
                elements: Arc::from([
                    ScriptDialogElement::RichText {
                        text: "A long styled explanation that should wrap within a constrained window instead of requiring a native window.".to_owned(),
                        style: ScriptDialogRichTextStyle {
                            strong: true,
                            italics: true,
                            monospace: true,
                            underline: true,
                            strikethrough: true,
                        },
                    },
                    ScriptDialogElement::Button {
                        id: None,
                        label: "No action".to_owned(),
                    },
                    ScriptDialogElement::Button {
                        id: Some(ScriptDialogButtonId::from_raw(id * 10)),
                        label: "Apply".to_owned(),
                    },
                ]),
            }),
            open_request,
        }
    }

    fn markdown(id: u64, owner: u64, open_request: u64) -> ScriptDialogState {
        let callback_id = ScriptDialogButtonId::from_raw(id * 10);
        ScriptDialogState {
            id: ScriptDialogId::from_raw(id),
            owner_script_id: ScriptId::from_raw(owner),
            owner_script_name: format!("owner-{owner}.lua"),
            name: "Markdown".to_owned(),
            kind: ScriptDialogKind::Simple(ScriptDialogContent {
                elements: Arc::from([ScriptDialogElement::Markdown {
                    text: "[Run callback](run)".to_owned(),
                    links: Arc::from([ScriptDialogMarkdownLink {
                        destination: "run".to_owned(),
                        callback_id,
                    }]),
                }]),
            }),
            open_request,
        }
    }

    fn paged(id: u64, open_request: u64) -> ScriptDialogState {
        ScriptDialogState {
            id: ScriptDialogId::from_raw(id),
            owner_script_id: ScriptId::from_raw(9),
            owner_script_name: "pages.lua".to_owned(),
            name: "Guide".to_owned(),
            kind: ScriptDialogKind::Paged(Arc::from([
                ScriptDialogContent {
                    elements: Arc::from([ScriptDialogElement::RichText {
                        text: "Page one".to_owned(),
                        style: Default::default(),
                    }]),
                },
                ScriptDialogContent {
                    elements: Arc::from([ScriptDialogElement::RichText {
                        text: "Page two".to_owned(),
                        style: Default::default(),
                    }]),
                },
            ])),
            open_request,
        }
    }

    fn frame(
        context: &egui::Context,
        component: &mut ScriptDialogs,
        dialogs: &[ScriptDialogState],
        events: Vec<egui::Event>,
        size: egui::Vec2,
    ) -> Vec<AppAction> {
        let mut actions = Vec::new();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| component.show_control(ui, dialogs));
                actions.extend(component.show_windows(ui.ctx(), dialogs, None));
            },
        );
        actions
    }

    fn content_frame(
        context: &egui::Context,
        component: &mut ScriptDialogs,
        dialog: &ScriptDialogState,
        content: &ScriptDialogContent,
        events: Vec<egui::Event>,
    ) -> Vec<AppAction> {
        let mut actions = Vec::new();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                show_content(
                    ui,
                    dialog.owner_script_id,
                    &dialog.owner_script_name,
                    dialog.id,
                    content,
                    0,
                    &mut actions,
                    component,
                )
            },
        );
        actions
    }

    fn page_control_frame(
        context: &egui::Context,
        component: &mut ScriptDialogs,
        dialog_id: ScriptDialogId,
        page: &mut usize,
        events: Vec<egui::Event>,
    ) {
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| show_page_control(ui, dialog_id, page, 2, component),
        );
    }

    fn click(
        context: &egui::Context,
        component: &mut ScriptDialogs,
        dialogs: &[ScriptDialogState],
        position: egui::Pos2,
    ) -> Vec<AppAction> {
        let mut actions = frame(
            context,
            component,
            dialogs,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::vec2(900.0, 600.0),
        );
        actions.extend(frame(
            context,
            component,
            dialogs,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::vec2(900.0, 600.0),
        ));
        actions
    }

    #[shoop_wasm_test_support::shoop_test]
    fn control_is_hidden_without_dialogs_and_combines_count_with_label() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut component = ScriptDialogs::default();
        let empty = context.run_ui(Default::default(), |ui| {
            component.show_control(ui, &[]);
        });
        assert!(component.control_rect.is_none());
        assert!(empty.shapes.is_empty());

        let dialogs = (1..=10)
            .map(|id| simple(id, id, &format!("Dialog {id}"), 0))
            .collect::<Vec<_>>();
        let output = context.run_ui(Default::default(), |ui| {
            component.show_control(ui, &dialogs);
        });
        assert!(component.control_rect.is_some());

        fn collect_text(shape: &egui::Shape, text: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(value) => text.push(value.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_text(shape, text);
                    }
                }
                _ => {}
            }
        }
        let mut text = Vec::new();
        for shape in output.shapes {
            collect_text(&shape.shape, &mut text);
        }
        assert!(text.iter().any(|text| text == "10 Script Dialogs"));
        assert!(!text.iter().any(|text| text == "10"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dialog_window_shows_its_owner_as_plain_italic_text() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let dialogs = [simple(2, 7, "Owned dialog", 1)];
        let mut component = ScriptDialogs::default();
        frame(
            &context,
            &mut component,
            &dialogs,
            Vec::new(),
            egui::vec2(900.0, 600.0),
        );
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                component.show_windows(ui.ctx(), &dialogs, None);
            },
        );

        fn collect_text(shape: &egui::Shape, text: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(value) => text.push(value.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_text(shape, text);
                    }
                }
                _ => {}
            }
        }
        let mut text = Vec::new();
        for shape in output.shapes {
            collect_text(&shape.shape, &mut text);
        }
        assert!(text.iter().any(|text| text == "owned by owner-7.lua"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn control_opens_closed_dialog_and_callback_emits_exact_intent() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let dialogs = [simple(3, 7, "Shared", 0), simple(4, 8, "Shared", 0)];
        let mut component = ScriptDialogs::default();
        frame(
            &context,
            &mut component,
            &dialogs,
            Vec::new(),
            egui::vec2(900.0, 600.0),
        );
        assert_eq!(component.states.len(), 2);
        assert!(!component.states[&dialogs[0].id].open);

        let control = component.control_rect.unwrap().center();
        click(&context, &mut component, &dialogs, control);
        assert_eq!(component.entry_rects.len(), 2);
        component.states.get_mut(&dialogs[0].id).unwrap().open = true;
        frame(
            &context,
            &mut component,
            &dialogs,
            Vec::new(),
            egui::vec2(900.0, 600.0),
        );
        assert!(component.states[&dialogs[0].id].open);

        let button_id = ScriptDialogButtonId::from_raw(30);
        assert!(component
            .button_rects
            .contains_key(&(dialogs[0].id, button_id)));
        assert_eq!(
            button_action(dialogs[0].owner_script_id, dialogs[0].id, button_id),
            AppAction::InvokeScriptDialogButton {
                script_id: dialogs[0].owner_script_id,
                dialog_id: dialogs[0].id,
                button_id,
            }
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn content_button_click_emits_exact_typed_intent() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let dialog = simple(6, 7, "Action", 1);
        let ScriptDialogKind::Simple(content) = &dialog.kind else {
            panic!("expected simple dialog");
        };
        let mut component = ScriptDialogs::default();
        content_frame(&context, &mut component, &dialog, content, Vec::new());
        let button_id = ScriptDialogButtonId::from_raw(60);
        let position = component.button_rects[&(dialog.id, button_id)].center();
        assert!(content_frame(
            &context,
            &mut component,
            &dialog,
            content,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        )
        .is_empty());
        assert_eq!(
            content_frame(
                &context,
                &mut component,
                &dialog,
                content,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            ),
            [AppAction::InvokeScriptDialogButton {
                script_id: dialog.owner_script_id,
                dialog_id: dialog.id,
                button_id,
            }]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn markdown_link_click_emits_its_lua_callback_intent() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let dialog = markdown(7, 8, 1);
        let ScriptDialogKind::Simple(content) = &dialog.kind else {
            panic!("expected simple dialog");
        };
        let callback_id = ScriptDialogButtonId::from_raw(70);
        let mut component = ScriptDialogs::default();
        content_frame(&context, &mut component, &dialog, content, Vec::new());
        let rect = component.link_rects[&(dialog.id, callback_id)];
        let position = egui::pos2(rect.left() + 10.0, rect.center().y);
        assert!(content_frame(
            &context,
            &mut component,
            &dialog,
            content,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        )
        .is_empty());
        assert_eq!(
            content_frame(
                &context,
                &mut component,
                &dialog,
                content,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            ),
            [AppAction::InvokeScriptDialogButton {
                script_id: dialog.owner_script_id,
                dialog_id: dialog.id,
                button_id: callback_id,
            }]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn page_and_visibility_state_persist_but_new_generation_resets() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut dialogs = vec![paged(10, 1)];
        let mut component = ScriptDialogs::default();
        frame(
            &context,
            &mut component,
            &dialogs,
            Vec::new(),
            egui::vec2(900.0, 600.0),
        );
        assert!(component.next_rects.contains_key(&dialogs[0].id));
        let mut selected_page = 0;
        page_control_frame(
            &context,
            &mut component,
            dialogs[0].id,
            &mut selected_page,
            Vec::new(),
        );
        let next = component.next_rects[&dialogs[0].id].center();
        page_control_frame(
            &context,
            &mut component,
            dialogs[0].id,
            &mut selected_page,
            vec![
                egui::Event::PointerMoved(next),
                egui::Event::PointerButton {
                    pos: next,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        page_control_frame(
            &context,
            &mut component,
            dialogs[0].id,
            &mut selected_page,
            vec![
                egui::Event::PointerMoved(next),
                egui::Event::PointerButton {
                    pos: next,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(selected_page, 1);
        component.states.get_mut(&dialogs[0].id).unwrap().page = selected_page;

        component.states.get_mut(&dialogs[0].id).unwrap().open = false;
        frame(
            &context,
            &mut component,
            &dialogs,
            Vec::new(),
            egui::vec2(900.0, 600.0),
        );
        assert_eq!(component.states[&dialogs[0].id].page, 1);
        assert!(!component.states[&dialogs[0].id].open);

        dialogs[0].open_request = 2;
        frame(
            &context,
            &mut component,
            &dialogs,
            Vec::new(),
            egui::vec2(900.0, 600.0),
        );
        assert!(component.states[&dialogs[0].id].open);
        assert_eq!(component.states[&dialogs[0].id].page, 1);

        dialogs.clear();
        frame(
            &context,
            &mut component,
            &dialogs,
            Vec::new(),
            egui::vec2(900.0, 600.0),
        );
        assert!(component.states.is_empty());
        dialogs.push(paged(11, 0));
        frame(
            &context,
            &mut component,
            &dialogs,
            Vec::new(),
            egui::vec2(900.0, 600.0),
        );
        assert_eq!(component.states[&dialogs[0].id].page, 0);
        assert!(!component.states[&dialogs[0].id].open);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn script_dialog_height_stabilizes_across_frames() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let dialogs = [simple(19, 1, "Stable dialog", 1)];
        let mut component = ScriptDialogs::default();
        let window_id = egui::Id::new(("script_dialog", dialogs[0].id.raw()));
        let mut heights = Vec::new();
        for _ in 0..8 {
            frame(
                &context,
                &mut component,
                &dialogs,
                Vec::new(),
                egui::vec2(900.0, 600.0),
            );
            heights.push(
                context
                    .memory(|memory| memory.area_rect(window_id))
                    .unwrap()
                    .height(),
            );
        }
        let settled = &heights[3..];
        assert!(
            settled
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 0.1),
            "script dialog kept changing height: {heights:?}"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn simple_and_paged_windows_paint_at_minimum_and_common_sizes() {
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let context = egui::Context::default();
            crate::initialize(&context);
            let dialogs = [simple(20, 1, "Long simple dialog title", 1), paged(21, 1)];
            let mut component = ScriptDialogs::default();
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| component.show_control(ui, &dialogs));
                    component.show_windows(ui.ctx(), &dialogs, None);
                },
            );
            assert!(!output.shapes.is_empty());
            assert_eq!(component.states.len(), 2);
        }
    }
}
