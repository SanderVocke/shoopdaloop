use shoop_app_api::*;
use shoop_script_resources::{
    NormalizedRelativePath, ResourceKind, ResourceLimits, ScriptResource, ScriptResourceBundle,
};
use std::{collections::BTreeMap, sync::Arc};

const SOURCE: &str = "shoop_announce_api_version(1, 0); local d=require('shoop_dialog'); d.simple('Browser P16 Help', {d.markdown_file('help/readme.md')})";
const IMAGE: &[u8] = include_bytes!("../../../../../resources/logo-small.png");

pub(super) fn fixture(bundle: &mut shoop_session::SessionBundle) -> Result<(), String> {
    let resources = BTreeMap::from([
        (
            NormalizedRelativePath::parse("main.lua").unwrap(),
            ScriptResource::new(ResourceKind::Lua, SOURCE.as_bytes()),
        ),
        (
            NormalizedRelativePath::parse("help/readme.md").unwrap(),
            ScriptResource::new(
                ResourceKind::Markdown,
                &b"# Portable\n\n![icon](images/icon.png)"[..],
            ),
        ),
        (
            NormalizedRelativePath::parse("help/images/icon.png").unwrap(),
            ScriptResource::new(ResourceKind::Image, IMAGE),
        ),
    ]);
    let resources = ScriptResourceBundle::new(
        NormalizedRelativePath::parse("main.lua").unwrap(),
        resources,
        ResourceLimits::default(),
    )
    .map_err(|error| error.to_string())?;
    bundle.document.scripts.push(shoop_session::ScriptDocument {
        id: 77,
        name: "browser-portable.lua".into(),
        entrypoint: "main.lua".into(),
        enabled: true,
    });
    bundle.scripts.insert(77, Arc::new(resources));
    Ok(())
}

pub(super) fn script(snapshot: &AppSnapshot) -> Result<&ScriptState, String> {
    snapshot
        .scripting
        .scripts
        .iter()
        .find(|script| script.name == "browser-portable.lua")
        .ok_or_else(|| "browser session script missing".into())
}

pub(super) fn stopped(snapshot: &AppSnapshot) -> Result<(), String> {
    let script = script(snapshot)?;
    if !script.enabled
        || script.lifecycle != ScriptLifecycle::Inactive
        || snapshot
            .scripting
            .dialogs
            .iter()
            .any(|dialog| dialog.name == "Browser P16 Help")
    {
        return Err("browser load started its enabled script".into());
    }
    Ok(())
}

pub(super) fn image(snapshot: &AppSnapshot) -> Result<bool, String> {
    let Some(dialog) = snapshot
        .scripting
        .dialogs
        .iter()
        .find(|dialog| dialog.name == "Browser P16 Help")
    else {
        return Ok(false);
    };
    let ScriptDialogKind::Simple(content) = &dialog.kind else {
        return Err("invalid browser help dialog".into());
    };
    let ScriptDialogElement::Markdown {
        resource_base_uri: Some(base),
        ..
    } = &content.elements[0]
    else {
        return Err("browser help provider missing".into());
    };
    let bytes = shoop_script_resources::read_resource_uri(&format!("{base}images/icon.png"))?;
    if bytes.as_deref() != Some(IMAGE) {
        return Err("browser help image changed or missing".into());
    }
    Ok(true)
}
