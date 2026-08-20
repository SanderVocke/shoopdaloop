use egui::load::{BytesLoadResult, BytesLoader, BytesPoll, LoadError};

pub struct ScriptResourceLoader;

impl BytesLoader for ScriptResourceLoader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(ScriptResourceLoader)
    }

    fn load(&self, _context: &egui::Context, uri: &str) -> BytesLoadResult {
        match shoop_script_resources::read_resource_uri(uri) {
            Ok(Some(bytes)) => Ok(BytesPoll::Ready {
                size: None,
                bytes: bytes.into(),
                mime: uri
                    .rsplit_once('.')
                    .filter(|(_, extension)| extension.eq_ignore_ascii_case("png"))
                    .map(|_| "image/png".to_owned()),
            }),
            Ok(None) => Err(LoadError::NotSupported),
            Err(error) => Err(LoadError::Loading(error)),
        }
    }

    fn forget(&self, _uri: &str) {}

    fn forget_all(&self) {}

    fn byte_size(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_script_resources::{
        register_resource_provider, NormalizedRelativePath, RegisteredResourceProvider,
        ResourceKind, ResourceLimits, ResourceOrigin, ScriptResource, ScriptResourceBundle,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    #[shoop_wasm_test_support::shoop_test]
    fn custom_loader_returns_decodable_generation_scoped_image_bytes() {
        let entrypoint = NormalizedRelativePath::parse("main.lua").unwrap();
        let bundle = Arc::new(
            ScriptResourceBundle::new(
                entrypoint.clone(),
                BTreeMap::from([
                    (
                        entrypoint,
                        ScriptResource::new(ResourceKind::Lua, Arc::<[u8]>::from(&b"return"[..])),
                    ),
                    (
                        NormalizedRelativePath::parse("images/icon.png").unwrap(),
                        ScriptResource::new(
                            ResourceKind::Image,
                            Arc::<[u8]>::from(
                                &include_bytes!("../../../../resources/logo-small.png")[..],
                            ),
                        ),
                    ),
                ]),
                ResourceLimits::default(),
            )
            .unwrap(),
        );
        let origin = ResourceOrigin {
            scope: "loader-test".to_owned(),
            generation: 1,
        };
        register_resource_provider(&origin, RegisteredResourceProvider::Bundle(bundle)).unwrap();
        let loader = ScriptResourceLoader;
        let loaded = loader
            .load(
                &egui::Context::default(),
                "shoop-script-resource://loader-test/1/images/icon.png",
            )
            .unwrap();
        let BytesPoll::Ready { bytes, mime, .. } = loaded else {
            panic!("expected ready image bytes")
        };
        assert_eq!(mime.as_deref(), Some("image/png"));
        assert!(image::load_from_memory(bytes.as_ref()).is_ok());
    }
}
