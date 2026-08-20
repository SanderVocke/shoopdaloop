use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::anyhow;
use omnilua::{Function, Lua, Table, Value};
use shoop_app_api::{
    ScriptDialogButtonId, ScriptDialogContent, ScriptDialogElement, ScriptDialogId,
    ScriptDialogKind, ScriptDialogMarkdownLink, ScriptDialogRichTextStyle, ScriptDialogState,
    ScriptId,
};

use crate::api_version::ApiVersionState;
use crate::file::ScriptFileReader;
use crate::{install_compatibility_value, runtime_error};

const ELEMENT_KIND: &str = "__shoop_dialog_element_kind";

#[derive(Default)]
pub struct DialogIdSource {
    next_dialog: Cell<u64>,
    next_button: Cell<u64>,
}

impl DialogIdSource {
    fn dialog(&self) -> ScriptDialogId {
        ScriptDialogId::from_raw(next_id(&self.next_dialog))
    }

    fn button(&self) -> ScriptDialogButtonId {
        ScriptDialogButtonId::from_raw(next_id(&self.next_button))
    }
}

fn next_id(next: &Cell<u64>) -> u64 {
    let id = next.get().max(1);
    next.set(id.saturating_add(1));
    id
}

struct RuntimeDialog {
    id: ScriptDialogId,
    name: String,
    kind: ScriptDialogKind,
    open_request: u64,
    callbacks: BTreeMap<ScriptDialogButtonId, Function>,
}

#[derive(Default)]
pub struct DialogRegistry {
    dialogs: RefCell<Vec<RuntimeDialog>>,
}

impl DialogRegistry {
    pub fn has_dialogs(&self) -> bool {
        !self.dialogs.borrow().is_empty()
    }

    pub fn states(&self, script_id: ScriptId, script_name: &str) -> Vec<ScriptDialogState> {
        self.dialogs
            .borrow()
            .iter()
            .map(|dialog| ScriptDialogState {
                id: dialog.id,
                owner_script_id: script_id,
                owner_script_name: script_name.to_owned(),
                name: dialog.name.clone(),
                kind: dialog.kind.clone(),
                open_request: dialog.open_request,
            })
            .collect()
    }

    pub fn invoke(
        &self,
        dialog_id: ScriptDialogId,
        button_id: ScriptDialogButtonId,
    ) -> anyhow::Result<()> {
        let callback = self
            .dialogs
            .borrow()
            .iter()
            .find(|dialog| dialog.id == dialog_id)
            .and_then(|dialog| dialog.callbacks.get(&button_id))
            .cloned()
            .ok_or_else(|| anyhow!("stale or unknown script dialog button"))?;
        callback
            .call::<_, ()>(())
            .map_err(|error| anyhow!(error.to_string()))
    }
}

pub fn install_dialog_api(
    lua: &Lua,
    run_sandboxed: &Function,
    versions: Rc<ApiVersionState>,
    ids: Rc<DialogIdSource>,
    registry: Rc<DialogRegistry>,
    mark_listening: Rc<dyn Fn()>,
    files: Rc<ScriptFileReader>,
) -> anyhow::Result<()> {
    let module = (|| -> omnilua::Result<Table> {
        let module = lua.create_table()?;

        let versions_ = Rc::clone(&versions);
        module.set(
            "rich_text",
            lua.create_function(move |lua, (text, style): (String, Option<Table>)| {
                versions_.require_announced()?;
                let style = parse_style(style)?;
                let element = lua.create_table()?;
                element.set(ELEMENT_KIND, "rich_text")?;
                element.set("text", text)?;
                element.set("strong", style.strong)?;
                element.set("italics", style.italics)?;
                element.set("monospace", style.monospace)?;
                element.set("underline", style.underline)?;
                element.set("strikethrough", style.strikethrough)?;
                Ok(element)
            })?,
        )?;

        let versions_ = Rc::clone(&versions);
        let files_ = Rc::clone(&files);
        module.set(
            "markdown",
            lua.create_function(move |lua, (text, links): (String, Option<Table>)| {
                versions_.require_announced()?;
                let element = lua.create_table()?;
                element.set(ELEMENT_KIND, "markdown")?;
                element.set("text", text)?;
                element.set("links", links)?;
                element.set("resource_base_uri", files_.base_uri(None)?)?;
                Ok(element)
            })?,
        )?;

        let versions_ = Rc::clone(&versions);
        module.set(
            "markdown_file",
            lua.create_function(move |lua, (path, links): (String, Option<Table>)| {
                versions_.require_announced()?;
                let text = files.read_utf8(&path)?;
                let resource_base_uri = files.base_uri(Some(&path))?;
                let element = lua.create_table()?;
                element.set(ELEMENT_KIND, "markdown")?;
                element.set("text", text)?;
                element.set("links", links)?;
                element.set("resource_base_uri", resource_base_uri)?;
                Ok(element)
            })?,
        )?;

        let versions_ = Rc::clone(&versions);
        module.set(
            "button",
            lua.create_function(move |lua, (label, callback): (String, Option<Function>)| {
                versions_.require_announced()?;
                if label.trim().is_empty() {
                    return Err(runtime_error("dialog button label must not be empty"));
                }
                let element = lua.create_table()?;
                element.set(ELEMENT_KIND, "button")?;
                element.set("label", label)?;
                element.set("callback", callback)?;
                Ok(element)
            })?,
        )?;

        let versions_ = Rc::clone(&versions);
        let ids_ = Rc::clone(&ids);
        let registry_ = Rc::clone(&registry);
        let listening_ = Rc::clone(&mark_listening);
        module.set(
            "simple",
            lua.create_function(move |_, (name, elements): (String, Vec<Table>)| {
                versions_.require_announced()?;
                validate_name(&name)?;
                let (content, callbacks) = parse_content(elements, &ids_)?;
                register_dialog(
                    &registry_,
                    &ids_,
                    name,
                    ScriptDialogKind::Simple(content),
                    callbacks,
                )?;
                listening_();
                Ok(())
            })?,
        )?;

        let versions_ = Rc::clone(&versions);
        let ids_ = Rc::clone(&ids);
        let registry_ = Rc::clone(&registry);
        let listening_ = Rc::clone(&mark_listening);
        module.set(
            "paged",
            lua.create_function(move |_, (name, pages): (String, Vec<Vec<Table>>)| {
                versions_.require_announced()?;
                validate_name(&name)?;
                if pages.is_empty() {
                    return Err(runtime_error("paged dialog must contain at least one page"));
                }
                let mut contents = Vec::with_capacity(pages.len());
                let mut callbacks = BTreeMap::new();
                for page in pages {
                    let (content, page_callbacks) = parse_content(page, &ids_)?;
                    contents.push(content);
                    callbacks.extend(page_callbacks);
                }
                register_dialog(
                    &registry_,
                    &ids_,
                    name,
                    ScriptDialogKind::Paged(contents.into()),
                    callbacks,
                )?;
                listening_();
                Ok(())
            })?,
        )?;

        let versions_ = Rc::clone(&versions);
        let registry_ = Rc::clone(&registry);
        module.set(
            "open",
            lua.create_function(move |_, name: String| {
                versions_.require_announced()?;
                let mut dialogs = registry_.dialogs.borrow_mut();
                let dialog = dialogs
                    .iter_mut()
                    .find(|dialog| dialog.name == name)
                    .ok_or_else(|| runtime_error(format!("unknown script dialog {name:?}")))?;
                dialog.open_request = dialog.open_request.saturating_add(1);
                Ok(())
            })?,
        )?;

        Ok(module)
    })()
    .map_err(|error| anyhow!("could not install shoop_dialog API: {error}"))?;
    install_compatibility_value(run_sandboxed, "__shoop_dialog", module)
}

fn parse_style(style: Option<Table>) -> omnilua::Result<ScriptDialogRichTextStyle> {
    let Some(style) = style else {
        return Ok(Default::default());
    };
    let mut parsed = ScriptDialogRichTextStyle::default();
    for pair in style.pairs()? {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            return Err(runtime_error("dialog rich-text style keys must be strings"));
        };
        let key = key.to_str()?;
        let Value::Boolean(value) = value else {
            return Err(runtime_error(format!(
                "dialog rich-text style {key:?} must be boolean"
            )));
        };
        match key.as_str() {
            "strong" => parsed.strong = value,
            "italics" => parsed.italics = value,
            "monospace" => parsed.monospace = value,
            "underline" => parsed.underline = value,
            "strikethrough" => parsed.strikethrough = value,
            _ => {
                return Err(runtime_error(format!(
                    "unknown dialog rich-text style {key:?}"
                )))
            }
        }
    }
    Ok(parsed)
}

fn validate_name(name: &str) -> omnilua::Result<()> {
    if name.trim().is_empty() {
        Err(runtime_error("dialog name must not be empty"))
    } else {
        Ok(())
    }
}

fn parse_content(
    elements: Vec<Table>,
    ids: &DialogIdSource,
) -> omnilua::Result<(
    ScriptDialogContent,
    BTreeMap<ScriptDialogButtonId, Function>,
)> {
    if elements.is_empty() {
        return Err(runtime_error(
            "dialog content must contain at least one element",
        ));
    }
    let mut parsed = Vec::with_capacity(elements.len());
    let mut callbacks = BTreeMap::new();
    for element in elements {
        let kind: String = element
            .get(ELEMENT_KIND)
            .map_err(|_| runtime_error("invalid dialog element"))?;
        match kind.as_str() {
            "rich_text" => parsed.push(ScriptDialogElement::RichText {
                text: element.get("text")?,
                style: ScriptDialogRichTextStyle {
                    strong: element.get("strong")?,
                    italics: element.get("italics")?,
                    monospace: element.get("monospace")?,
                    underline: element.get("underline")?,
                    strikethrough: element.get("strikethrough")?,
                },
            }),
            "markdown" => {
                let text = element.get("text")?;
                let definitions: Option<Table> = element.get("links")?;
                let mut definitions_by_destination = BTreeMap::new();
                if let Some(definitions) = definitions {
                    for pair in definitions.pairs()? {
                        let (destination, callback) = pair?;
                        let Value::String(destination) = destination else {
                            return Err(runtime_error(
                                "dialog markdown link destinations must be strings",
                            ));
                        };
                        let destination = destination.to_str()?;
                        if destination.trim().is_empty() {
                            return Err(runtime_error(
                                "dialog markdown link destination must not be empty",
                            ));
                        }
                        let Value::Function(callback) = callback else {
                            return Err(runtime_error(format!(
                                "dialog markdown link {destination:?} callback must be a function"
                            )));
                        };
                        definitions_by_destination.insert(destination, callback);
                    }
                }
                let mut links = Vec::with_capacity(definitions_by_destination.len());
                for (destination, callback) in definitions_by_destination {
                    let callback_id = ids.button();
                    callbacks.insert(callback_id, callback);
                    links.push(ScriptDialogMarkdownLink {
                        destination,
                        callback_id,
                    });
                }
                parsed.push(ScriptDialogElement::Markdown {
                    text,
                    links: Arc::from(links),
                    resource_base_uri: element
                        .get::<_, Option<String>>("resource_base_uri")?
                        .map(Arc::from),
                });
            }
            "button" => {
                let callback: Option<Function> = element.get("callback")?;
                let id = callback.as_ref().map(|_| ids.button());
                if let (Some(id), Some(callback)) = (id, callback) {
                    callbacks.insert(id, callback);
                }
                parsed.push(ScriptDialogElement::Button {
                    id,
                    label: element.get("label")?,
                });
            }
            _ => return Err(runtime_error("invalid dialog element")),
        }
    }
    Ok((
        ScriptDialogContent {
            elements: Arc::from(parsed),
        },
        callbacks,
    ))
}

fn register_dialog(
    registry: &DialogRegistry,
    ids: &DialogIdSource,
    name: String,
    kind: ScriptDialogKind,
    callbacks: BTreeMap<ScriptDialogButtonId, Function>,
) -> omnilua::Result<()> {
    let mut dialogs = registry.dialogs.borrow_mut();
    if dialogs.iter().any(|dialog| dialog.name == name) {
        return Err(runtime_error(format!(
            "script dialog {name:?} is already defined"
        )));
    }
    let id = ids.dialog();
    dialogs.push(RuntimeDialog {
        id,
        name,
        kind,
        open_request: 0,
        callbacks,
    });
    Ok(())
}
