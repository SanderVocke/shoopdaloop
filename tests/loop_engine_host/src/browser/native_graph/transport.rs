use super::super::{allocation_guard, BrowserBridge};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread::JoinHandle;

enum Input {
    Primitive([u8; 64]),
    Payload([u8; 96]),
    Graph([u8; 96]),
    Export([u8; 96]),
    ExportResponse,
    Storage,
    Render(u32),
    PrimitiveResponse,
    GraphResponse,
    Counts,
    Stop,
}

pub(in crate::browser) struct Rendered {
    pub result: u32,
    pub left: [f32; 128],
    pub right: [f32; 128],
    pub heap: allocation_guard::Counts,
    pub frames: u64,
    pub callbacks: u64,
}

pub(in crate::browser) struct Stopped {
    pub journal: Vec<u8>,
    pub semantic: Vec<u8>,
    pub recovered: [u32; 4],
}

enum Output {
    Result(u32),
    Rendered(Rendered),
    PrimitiveResponse(Option<[u8; 96]>),
    GraphResponse(Option<[u8; 128]>),
    Counts([u32; 3]),
    ExportResponse(Option<[u8; crate::export_wire::RESPONSE_BYTES]>),
    Storage([u32; 6]),
    Stopped(Stopped),
}

fn dispatch(bridge: &mut BrowserBridge, input: Input) -> Output {
    assert!(!allocation_guard::is_rendering());
    match input {
        Input::Primitive(bytes) => {
            bridge.request = bytes;
            Output::Result(bridge.ingest())
        }
        Input::Payload(bytes) => {
            bridge.graph.as_mut().unwrap().payload = bytes;
            Output::Result(bridge.ingest_graph_payload())
        }
        Input::Graph(bytes) => {
            bridge.graph.as_mut().unwrap().request = bytes;
            Output::Result(bridge.ingest_graph())
        }
        Input::Export(bytes) => {
            bridge.exports.as_mut().unwrap().request = bytes;
            Output::Result(bridge.ingest_export())
        }
        Input::ExportResponse => Output::ExportResponse(match bridge.poll_export_response() {
            0 => None,
            1 => Some(bridge.exports.as_ref().unwrap().response),
            code => panic!("export response fault {code}"),
        }),
        Input::Storage => Output::Storage([
            bridge.export_resource_bytes(),
            bridge.export_read_bytes(),
            crate::export_wire::REQUEST_BYTES as u32,
            crate::export_wire::RESPONSE_BYTES as u32,
            crate::export_wire::BANKS as u32,
            crate::export_wire::CREDITS as u32,
        ]),
        Input::Render(frames) => {
            let result = bridge.render(frames);
            Output::Rendered(Rendered {
                result,
                left: bridge.left,
                right: bridge.right,
                heap: bridge.heap,
                frames: bridge.frame(),
                callbacks: bridge.callbacks(),
            })
        }
        Input::PrimitiveResponse => Output::PrimitiveResponse(match bridge.poll_response() {
            0 => None,
            1 => Some(bridge.response),
            code => panic!("primitive response fault {code}"),
        }),
        Input::GraphResponse => Output::GraphResponse(match bridge.poll_graph_response() {
            0 => None,
            1 => Some(bridge.graph.as_ref().unwrap().response),
            code => panic!("graph response fault {code}"),
        }),
        Input::Counts => Output::Counts([
            bridge.outstanding(),
            bridge.retained_graphs(),
            bridge.retained_exports(),
        ]),
        Input::Stop => match bridge.stop() {
            0 => Output::Stopped(Stopped {
                journal: bridge.journal[..bridge.journal_len].to_vec(),
                semantic: bridge.semantic[..bridge.semantic_len].to_vec(),
                recovered: bridge.recovered,
            }),
            code => Output::Result(code),
        },
    }
}

enum Backend {
    Direct(Box<BrowserBridge>),
    Threaded {
        input: Option<SyncSender<Input>>,
        output: Receiver<Output>,
        worker: Option<JoinHandle<()>>,
    },
}

pub(in crate::browser) struct Host(Backend);

impl Host {
    pub fn new(session: u64, threaded: bool) -> Self {
        Self::prepare(session, threaded, false)
    }
    pub fn new_exports(session: u64, threaded: bool) -> Self {
        Self::prepare(session, threaded, true)
    }
    fn prepare(session: u64, threaded: bool, exports: bool) -> Self {
        assert!(!allocation_guard::is_rendering());
        if !threaded {
            return Self(Backend::Direct(Box::new(if exports {
                BrowserBridge::new_exports(session).unwrap()
            } else {
                BrowserBridge::new_graphs(session, 0).unwrap()
            })));
        }
        let (input, inputs) = sync_channel(1);
        let (outputs, output) = sync_channel(1);
        let worker = std::thread::spawn(move || {
            let mut bridge = if exports {
                BrowserBridge::new_exports(session).unwrap()
            } else {
                BrowserBridge::new_graphs(session, 0).unwrap()
            };
            while let Ok(input) = inputs.recv() {
                let result = dispatch(&mut bridge, input);
                assert!(!allocation_guard::is_rendering());
                if outputs.send(result).is_err() {
                    break;
                }
            }
        });
        Self(Backend::Threaded {
            input: Some(input),
            output,
            worker: Some(worker),
        })
    }

    fn exchange(&mut self, input: Input) -> Output {
        assert!(!allocation_guard::is_rendering());
        match &mut self.0 {
            Backend::Direct(bridge) => dispatch(bridge, input),
            Backend::Threaded {
                input: tx, output, ..
            } => {
                tx.as_ref().unwrap().send(input).unwrap();
                output.recv().unwrap()
            }
        }
    }

    pub fn primitive(&mut self, bytes: [u8; 64]) -> u32 {
        match self.exchange(Input::Primitive(bytes)) {
            Output::Result(code) => code,
            _ => unreachable!(),
        }
    }

    pub fn payload(&mut self, bytes: [u8; 96]) -> u32 {
        match self.exchange(Input::Payload(bytes)) {
            Output::Result(code) => code,
            _ => unreachable!(),
        }
    }

    pub fn graph(&mut self, bytes: [u8; 96]) -> u32 {
        match self.exchange(Input::Graph(bytes)) {
            Output::Result(code) => code,
            _ => unreachable!(),
        }
    }

    pub fn render(&mut self, frames: u32) -> Rendered {
        match self.exchange(Input::Render(frames)) {
            Output::Rendered(rendered) => rendered,
            _ => unreachable!(),
        }
    }

    pub fn primitive_response(&mut self) -> Option<[u8; 96]> {
        match self.exchange(Input::PrimitiveResponse) {
            Output::PrimitiveResponse(bytes) => bytes,
            _ => unreachable!(),
        }
    }

    pub fn graph_response(&mut self) -> Option<[u8; 128]> {
        match self.exchange(Input::GraphResponse) {
            Output::GraphResponse(bytes) => bytes,
            _ => unreachable!(),
        }
    }

    pub fn export(&mut self, bytes: [u8; 96]) -> u32 {
        match self.exchange(Input::Export(bytes)) {
            Output::Result(code) => code,
            _ => unreachable!(),
        }
    }
    pub fn export_response(&mut self) -> Option<[u8; crate::export_wire::RESPONSE_BYTES]> {
        match self.exchange(Input::ExportResponse) {
            Output::ExportResponse(bytes) => bytes,
            _ => unreachable!(),
        }
    }
    pub fn storage(&mut self) -> [u32; 6] {
        match self.exchange(Input::Storage) {
            Output::Storage(storage) => storage,
            _ => unreachable!(),
        }
    }
    pub fn counts(&mut self) -> [u32; 3] {
        match self.exchange(Input::Counts) {
            Output::Counts(counts) => counts,
            _ => unreachable!(),
        }
    }

    pub fn stop(&mut self) -> Result<Stopped, u32> {
        match self.exchange(Input::Stop) {
            Output::Result(code) => Err(code),
            Output::Stopped(stopped) => Ok(stopped),
            _ => unreachable!(),
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        assert!(!allocation_guard::is_rendering());
        if let Backend::Threaded { input, worker, .. } = &mut self.0 {
            drop(input.take());
            let joined = worker.take().unwrap().join();
            if !std::thread::panicking() {
                joined.unwrap();
            }
        }
    }
}
