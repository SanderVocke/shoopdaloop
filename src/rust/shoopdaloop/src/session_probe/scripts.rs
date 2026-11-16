use super::*;
use crate::application::ApplicationHandle;
use shoop_egui::*;

const SOURCE: &str = "shoop_announce_api_version(1, 0); local d=require('shoop_dialog'); d.simple('Packaged Help', {d.markdown_file('help/readme.md')})";
const HELP: &str = "# Portable\n\n![icon](images/icon.png)";
const IMAGE: &[u8] = include_bytes!("../../../../../resources/logo-small.png");

pub(super) fn include(handle: &ApplicationHandle, root: &std::path::Path) -> Result<()> {
    let directory = root.join("script-originals");
    std::fs::create_dir_all(directory.join("help/images"))?;
    let source = directory.join("portable.lua");
    std::fs::write(&source, SOURCE)?;
    std::fs::write(directory.join("help/readme.md"), HELP)?;
    std::fs::write(directory.join("help/images/icon.png"), IMAGE)?;
    handle.dispatch(AppIntent::AddScriptSource {
        name: "Machine-local".into(),
        source: "shoop_announce_api_version(1, 0)".into(),
        kind: ScriptKind::User,
        enabled: false,
    })?;
    handle.dispatch(AppIntent::AddEphemeralScript {
        name: "portable.lua".into(),
        source: SOURCE.into(),
        source_path: Some(source.to_string_lossy().into_owned()),
    })?;
    let state = file_wait(handle, |state| state.scripting.scripts.len() == 2)?;
    let id = state
        .scripting
        .scripts
        .iter()
        .find(|script| script.kind == ScriptKind::Ephemeral)
        .unwrap()
        .id;
    handle.dispatch(AppIntent::ConvertScriptKind {
        script_id: id,
        kind: ScriptKind::Session,
    })?;
    file_wait(handle, |state| {
        state
            .scripting
            .scripts
            .iter()
            .any(|script| script.id == id && script.kind == ScriptKind::Session)
    })?;
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

pub(super) fn verify_archive(bundle: &shoop_session::SessionBundle) -> Result<()> {
    use shoop_script_resources::NormalizedRelativePath;
    ensure!(
        bundle.document.scripts.len() == 1,
        "project did not include exactly its session script"
    );
    let resources = &bundle.scripts[&bundle.document.scripts[0].id];
    ensure!(
        resources.resources.len() == 3,
        "script companions were not archived"
    );
    ensure!(
        resources.entrypoint_resource().bytes.as_ref() == SOURCE.as_bytes(),
        "script source changed"
    );
    ensure!(
        resources
            .get(&NormalizedRelativePath::parse("help/readme.md")?)
            .unwrap()
            .bytes
            .as_ref()
            == HELP.as_bytes(),
        "script help changed"
    );
    ensure!(
        resources
            .get(&NormalizedRelativePath::parse("help/images/icon.png")?)
            .unwrap()
            .bytes
            .as_ref()
            == IMAGE,
        "script image changed"
    );
    Ok(())
}

pub(super) fn explicit_start(handle: &ApplicationHandle) -> Result<()> {
    let loaded = handle.snapshot();
    let script = loaded
        .scripting
        .scripts
        .iter()
        .find(|script| script.kind == ScriptKind::Session)
        .ok_or_else(|| anyhow!("loaded script missing"))?;
    ensure!(
        script.enabled
            && script.lifecycle == ScriptLifecycle::Inactive
            && loaded.scripting.dialogs.is_empty(),
        "load started a script"
    );
    ensure!(
        loaded
            .scripting
            .scripts
            .iter()
            .any(|script| script.name == "Machine-local" && script.kind == ScriptKind::User),
        "load erased machine-local scripts"
    );
    handle.dispatch(AppIntent::RestartScript {
        script_id: script.id,
    })?;
    let started = file_wait(handle, |state| {
        state
            .scripting
            .dialogs
            .iter()
            .any(|dialog| dialog.name == "Packaged Help")
    })?;
    let dialog = started
        .scripting
        .dialogs
        .iter()
        .find(|dialog| dialog.name == "Packaged Help")
        .unwrap();
    let ScriptDialogKind::Simple(content) = &dialog.kind else {
        anyhow::bail!("unexpected help dialog");
    };
    let ScriptDialogElement::Markdown {
        resource_base_uri: Some(base),
        ..
    } = &content.elements[0]
    else {
        anyhow::bail!("missing resource provider");
    };
    ensure!(
        shoop_script_resources::read_resource_uri(&format!("{base}images/icon.png"))
            .map_err(anyhow::Error::msg)?
            .is_some_and(|bytes| bytes.as_ref() == IMAGE),
        "loaded help image could not be read"
    );
    handle.dispatch(AppIntent::StopScript {
        script_id: script.id,
    })?;
    file_wait(handle, |state| state.scripting.dialogs.is_empty())?;
    Ok(())
}
