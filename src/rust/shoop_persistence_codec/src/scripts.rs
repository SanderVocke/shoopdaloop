use anyhow::{anyhow, Result};
use shoop_script_resources::session::{SessionScriptData, SessionScripts};
use shoop_session::{ScriptDocument, SessionBundle};

pub fn take(bundle: &mut SessionBundle) -> Result<SessionScripts> {
    let scripts = SessionScripts(
        bundle
            .document
            .scripts
            .iter()
            .map(|script| {
                Ok(SessionScriptData {
                    document_id: script.id,
                    name: script.name.clone(),
                    enabled: script.enabled,
                    bundle: bundle
                        .scripts
                        .get(&script.id)
                        .cloned()
                        .ok_or_else(|| anyhow!("missing script resource bundle"))?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    );
    validate(&scripts)?;
    bundle.document.scripts.clear();
    bundle.scripts.clear();
    Ok(scripts)
}

pub fn validate(scripts: &SessionScripts) -> Result<()> {
    scripts.validate().map_err(|error| anyhow!(error))?;
    if !scripts.0.is_empty() {
        let lua = omnilua::Lua::new();
        for script in &scripts.0 {
            let source = std::str::from_utf8(&script.bundle.entrypoint_resource().bytes)?;
            lua.load(source)
                .set_name(&script.name)
                .into_function()
                .map_err(|error| anyhow!("could not compile {}: {error}", script.name))?;
        }
    }
    Ok(())
}

pub fn attach(bundle: &mut SessionBundle, scripts: SessionScripts) -> Result<()> {
    scripts.validate().map_err(|error| anyhow!(error))?;
    for script in scripts.0 {
        bundle.document.scripts.push(ScriptDocument {
            id: script.document_id,
            name: script.name,
            entrypoint: script.bundle.entrypoint.to_string(),
            enabled: script.enabled,
        });
        bundle.scripts.insert(script.document_id, script.bundle);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        codec::{Decoded, Value, Work},
        worker::{self, Command, Response},
    };
    use shoop_app_api::IoTaskKind;
    use shoop_app_protocol::{
        persistence::{Key, Target},
        persistence_transfer::UploadStream,
        Revision, TransferId,
    };
    use shoop_script_resources::{
        transfer, NormalizedRelativePath, ResourceKind, ResourceLimits, ScriptResource,
        ScriptResourceBundle,
    };
    use shoop_session::*;
    use shoop_wasm_test_support::shoop_test;
    use std::{collections::BTreeMap, sync::Arc};

    pub(crate) fn fixture() -> SessionScripts {
        SessionScripts(vec![SessionScriptData {
            document_id: 77,
            name: "portable.lua".into(),
            enabled: true,
            bundle: Arc::new(
                ScriptResourceBundle::new(
                    NormalizedRelativePath::parse("main.lua").unwrap(),
                    BTreeMap::from([
                        (
                            NormalizedRelativePath::parse("main.lua").unwrap(),
                            ScriptResource::new(
                                ResourceKind::Lua,
                                &b"error('must not execute on load')"[..],
                            ),
                        ),
                        (
                            NormalizedRelativePath::parse("help.md").unwrap(),
                            ScriptResource::new(
                                ResourceKind::Markdown,
                                vec![b'x'; transfer::CHUNK_BYTES * 3 + 1],
                            ),
                        ),
                        (
                            NormalizedRelativePath::parse("icon.png").unwrap(),
                            ScriptResource::new(
                                ResourceKind::Image,
                                &include_bytes!("../../../../resources/logo-small.png")[..],
                            ),
                        ),
                    ]),
                    ResourceLimits::default(),
                )
                .unwrap(),
            ),
        }])
    }
    #[shoop_test]
    fn complete_archives_split_scripts_without_executing_or_uploading_them() {
        let scripts = fixture();
        let Value::Image(image) = (Work::Prepare {
            decoded: Decoded::Session(SessionBundle::new(SessionDocument::empty(1000))),
            mapping: Vec::new(),
            update_length: true,
        })
        .run()
        .unwrap() else {
            panic!("musical image")
        };
        let Value::File { bytes, .. } = (Work::EncodeSession {
            image,
            scripts: scripts.clone(),
        })
        .run()
        .unwrap() else {
            panic!("archive")
        };
        let Decoded::Session(bundle) = (match (Work::Decode {
            kind: IoTaskKind::LoadSession,
            bytes,
            rate: 1000,
        })
        .run()
        .unwrap()
        {
            Value::Decoded(decoded) => decoded,
            _ => panic!("decoded session"),
        }) else {
            panic!("session bundle")
        };
        assert_eq!(bundle.document.scripts[0].id, 77);
        let Value::PreparedSession {
            image,
            scripts: restored,
        } = (Work::Prepare {
            decoded: Decoded::Session(bundle),
            mapping: Vec::new(),
            update_length: true,
        })
        .run()
        .unwrap()
        else {
            panic!("prepared additions")
        };
        assert_eq!(restored, scripts);
        assert!(crate::document::to_bundle(&image)
            .unwrap()
            .scripts
            .is_empty());
        assert_eq!(image.payloads.len(), 0);
    }
    #[shoop_test]
    fn invalid_lua_refuses_before_a_musical_candidate_is_returned() {
        let mut scripts = fixture();
        scripts.0[0].bundle =
            Arc::new(ScriptResourceBundle::source_only("main.lua", &b"function("[..]).unwrap());
        let mut bundle = SessionBundle::new(SessionDocument::empty(1000));
        attach(&mut bundle, scripts).unwrap();
        assert!((Work::Prepare {
            decoded: Decoded::Session(bundle),
            mapping: Vec::new(),
            update_length: true
        })
        .run()
        .is_err());
    }
    #[shoop_test]
    fn worker_streams_client_additions_separately_from_musical_images() {
        let expected = fixture();
        let Value::Image(image) = (Work::Prepare {
            decoded: Decoded::Session(SessionBundle::new(SessionDocument::empty(1000))),
            mapping: Vec::new(),
            update_length: true,
        })
        .run()
        .unwrap() else {
            panic!("image")
        };
        let mut stream = UploadStream::new(
            image,
            Key {
                transfer: TransferId(1),
                epoch: 1,
            },
            Target::Session {
                revision: Revision(1),
            },
        )
        .unwrap();
        let mut worker = worker::Worker::default();
        assert!(matches!(
            worker.receive(Command::BeginImage {
                description: stream.description(),
                kind: worker::kind(IoTaskKind::SaveSession).unwrap(),
                wav: false,
                standard: false,
                channels: Vec::new()
            }),
            Response::Ack
        ));
        let mut scripts = transfer::Source::new(expected.clone()).unwrap();
        while let Some(part) = scripts.next_part() {
            let encoded = worker::encode(&Command::ScriptPart(part)).unwrap();
            assert!(encoded.len() <= worker::MAX_RECORD);
            assert!(matches!(
                worker.receive(worker::decode(&encoded).unwrap()),
                Response::Ack
            ));
        }
        let mut declared = None;
        while let Some(part) = stream.next_request().unwrap() {
            match worker.receive(Command::Image(part)) {
                Response::Ack => (),
                Response::File { bytes, .. } => declared = Some(bytes),
                other => panic!("{other:?}"),
            }
        }
        let mut bytes = Vec::new();
        loop {
            match worker.receive(Command::Next) {
                Response::FileChunk {
                    offset,
                    bytes: chunk,
                } => {
                    assert_eq!(offset, bytes.len() as u64);
                    bytes.extend(chunk);
                }
                Response::Done => break,
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(Some(bytes.len() as u64), declared);
        let mut bundle = decode_session(&bytes).unwrap();
        assert_eq!(take(&mut bundle).unwrap(), expected);
        worker.receive(Command::BeginFile {
            kind: worker::kind(IoTaskKind::LoadSession).unwrap(),
            bytes: bytes.len() as u64,
            rate: 1000,
        });
        for (index, chunk) in bytes.chunks(worker::FILE_CHUNK).enumerate() {
            assert!(matches!(
                worker.receive(Command::FileChunk {
                    offset: (index * worker::FILE_CHUNK) as u64,
                    bytes: chunk.to_vec()
                }),
                Response::Ack
            ));
        }
        let Response::Decoded(summary) = worker.receive(Command::FinishFile) else {
            panic!("decoded summary")
        };
        assert!(matches!(
            worker.receive(Command::Prepare {
                handle: summary.handle,
                mapping: Vec::new(),
                update_length: true
            }),
            Response::Image(_)
        ));
        let mut assembly = transfer::Assembly::default();
        loop {
            match worker.receive(Command::NextImageBatch) {
                Response::ScriptPart(part) => assembly.apply(part).unwrap(),
                Response::ImageBatch { done: true, .. } => break,
                Response::ImageBatch { .. } => (),
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(assembly.finish().unwrap(), expected);
    }
    #[shoop_test]
    fn client_resource_counts_are_independent_of_musical_transfer_objects() {
        let mut resources = BTreeMap::new();
        resources.insert(
            NormalizedRelativePath::parse("main.lua").unwrap(),
            ScriptResource::new(ResourceKind::Lua, &b"return 1"[..]),
        );
        for index in 0..5000 {
            resources.insert(
                NormalizedRelativePath::parse(format!("help/{index}.md")).unwrap(),
                ScriptResource::new(ResourceKind::Markdown, &b""[..]),
            );
        }
        let scripts = SessionScripts(vec![SessionScriptData {
            document_id: 1,
            name: "many-resources.lua".into(),
            enabled: false,
            bundle: Arc::new(
                ScriptResourceBundle::new(
                    NormalizedRelativePath::parse("main.lua").unwrap(),
                    resources,
                    ResourceLimits::default(),
                )
                .unwrap(),
            ),
        }]);
        let Value::Image(image) = (Work::Prepare {
            decoded: Decoded::Session(SessionBundle::new(SessionDocument::empty(1000))),
            mapping: Vec::new(),
            update_length: true,
        })
        .run()
        .unwrap() else {
            panic!("image")
        };
        let Value::File { bytes, .. } = (Work::EncodeSession { image, scripts }).run().unwrap()
        else {
            panic!("archive")
        };
        let Value::Decoded(decoded) = (Work::Decode {
            kind: IoTaskKind::LoadSession,
            bytes,
            rate: 1000,
        })
        .run()
        .unwrap() else {
            panic!("decoded")
        };
        let Value::PreparedSession { image, scripts } = (Work::Prepare {
            decoded,
            mapping: Vec::new(),
            update_length: true,
        })
        .run()
        .unwrap() else {
            panic!("prepared")
        };
        assert!(image.payloads.is_empty());
        assert_eq!(scripts.0[0].bundle.resources.len(), 5001);
    }
}
