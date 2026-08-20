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
