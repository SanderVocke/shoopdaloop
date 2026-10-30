//! A software CPAL host for tests.
//!
//! `MockHost` implements the CPAL host, device and stream traits as a platform host
//! would, so the driver's real callback path runs without a sound card. Streams are
//! either driven by the test (`MockStreamHandle::render`, `capture`) or, on a
//! threaded host, by a thread per stream at the buffer period. Every data callback
//! runs inside a heap measurement; `heap_operations` reports what callbacks did.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, BuildStreamError, Data, DefaultStreamConfigError, DeviceDescription,
    DeviceDescriptionBuilder, DeviceDirection, DeviceId, DeviceIdError, DeviceNameError,
    DevicesError, InputCallbackInfo, InputStreamTimestamp, OutputCallbackInfo,
    OutputStreamTimestamp, PauseStreamError, PlayStreamError, SampleFormat, StreamConfig,
    StreamError, StreamInstant, SupportedBufferSize, SupportedStreamConfig,
    SupportedStreamConfigRange, SupportedStreamConfigsError,
};
use shoop_wasm_test_support::heap::measure;

type OutputCallback = Box<dyn FnMut(&mut Data, &OutputCallbackInfo) + Send>;
type InputCallback = Box<dyn FnMut(&Data, &InputCallbackInfo) + Send>;
type ErrorCallback = Box<dyn FnMut(StreamError) + Send>;

enum Callback {
    Output(OutputCallback),
    Input(InputCallback),
}

#[derive(Clone)]
pub struct MockHost {
    devices: Vec<MockDevice>,
    default_output: Option<usize>,
    default_input: Option<usize>,
}

impl MockHost {
    /// Devices in order; the first output and input devices are the defaults.
    pub fn new(devices: Vec<MockDevice>) -> Self {
        let default_output = devices.iter().position(|device| device.0.output);
        let default_input = devices.iter().position(|device| !device.0.output);
        Self {
            devices,
            default_output,
            default_input,
        }
    }
}

impl HostTrait for MockHost {
    type Devices = std::vec::IntoIter<MockDevice>;
    type Device = MockDevice;

    fn is_available() -> bool {
        true
    }
    fn devices(&self) -> Result<Self::Devices, DevicesError> {
        Ok(self.devices.clone().into_iter())
    }
    fn default_input_device(&self) -> Option<MockDevice> {
        self.default_input.map(|index| self.devices[index].clone())
    }
    fn default_output_device(&self) -> Option<MockDevice> {
        self.default_output.map(|index| self.devices[index].clone())
    }
}

struct DeviceInner {
    name: String,
    output: bool,
    channels: u16,
    sample_rate: u32,
    buffer: SupportedBufferSize,
    /// Streams are driven by threads at the buffer period.
    threaded: bool,
    /// The sample a threaded input stream captures.
    level: f32,
    streams: Mutex<Vec<Arc<MockStreamHandle>>>,
}

#[derive(Clone)]
pub struct MockDevice(Arc<DeviceInner>);

impl MockDevice {
    pub fn output(name: &str, channels: u16) -> Self {
        Self::new(name, true, channels)
    }
    pub fn input(name: &str, channels: u16) -> Self {
        Self::new(name, false, channels)
    }
    fn new(name: &str, output: bool, channels: u16) -> Self {
        Self(Arc::new(DeviceInner {
            name: name.into(),
            output,
            channels,
            sample_rate: 48_000,
            buffer: SupportedBufferSize::Range { min: 16, max: 4096 },
            threaded: false,
            level: 0.,
            streams: Mutex::new(Vec::new()),
        }))
    }
    pub fn with_buffer_range(self, min: u32, max: u32) -> Self {
        self.change(|inner| inner.buffer = SupportedBufferSize::Range { min, max })
    }
    pub fn threaded(self, level: f32) -> Self {
        self.change(|inner| {
            inner.threaded = true;
            inner.level = level;
        })
    }
    fn change(self, edit: impl FnOnce(&mut DeviceInner)) -> Self {
        let mut inner = Arc::try_unwrap(self.0)
            .ok()
            .expect("configure before sharing");
        edit(&mut inner);
        Self(Arc::new(inner))
    }

    /// The most recently built stream.
    pub fn stream(&self) -> Option<Arc<MockStreamHandle>> {
        self.0.streams.lock().unwrap().last().cloned()
    }

    fn config(&self) -> SupportedStreamConfig {
        SupportedStreamConfig::new(
            self.0.channels,
            self.0.sample_rate,
            self.0.buffer,
            SampleFormat::F32,
        )
    }
    fn range(&self) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(
            self.0.channels,
            self.0.sample_rate,
            self.0.sample_rate,
            self.0.buffer,
            SampleFormat::F32,
        )
    }
    fn build(
        &self,
        output: bool,
        config: &StreamConfig,
        format: SampleFormat,
        callback: Callback,
        error: ErrorCallback,
    ) -> Result<MockStream, BuildStreamError> {
        if output != self.0.output
            || format != SampleFormat::F32
            || config.channels == 0
            || config.channels > self.0.channels
            || config.sample_rate != self.0.sample_rate
        {
            return Err(BuildStreamError::StreamConfigNotSupported);
        }
        let handle = Arc::new(MockStreamHandle {
            channels: usize::from(config.channels),
            period: match config.buffer_size {
                BufferSize::Fixed(frames) => frames as usize,
                BufferSize::Default => 256,
            },
            sample_rate: config.sample_rate,
            level: self.0.level,
            callback: Mutex::new(Some(callback)),
            error: Mutex::new(Some(error)),
            heap: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
        });
        self.0.streams.lock().unwrap().push(handle.clone());
        Ok(MockStream {
            handle,
            threaded: self.0.threaded,
            thread: Mutex::new(None),
        })
    }
}

impl DeviceTrait for MockDevice {
    type SupportedInputConfigs = std::vec::IntoIter<SupportedStreamConfigRange>;
    type SupportedOutputConfigs = std::vec::IntoIter<SupportedStreamConfigRange>;
    type Stream = MockStream;

    fn description(&self) -> Result<DeviceDescription, DeviceNameError> {
        Ok(DeviceDescriptionBuilder::new(&self.0.name)
            .direction(if self.0.output {
                DeviceDirection::Output
            } else {
                DeviceDirection::Input
            })
            .build())
    }
    fn id(&self) -> Result<DeviceId, DeviceIdError> {
        Ok(DeviceId(cpal::default_host().id(), self.0.name.clone()))
    }
    fn supported_input_configs(
        &self,
    ) -> Result<Self::SupportedInputConfigs, SupportedStreamConfigsError> {
        Ok(if self.0.output {
            vec![]
        } else {
            vec![self.range()]
        }
        .into_iter())
    }
    fn supported_output_configs(
        &self,
    ) -> Result<Self::SupportedOutputConfigs, SupportedStreamConfigsError> {
        Ok(if self.0.output {
            vec![self.range()]
        } else {
            vec![]
        }
        .into_iter())
    }
    fn default_input_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError> {
        if self.0.output {
            Err(DefaultStreamConfigError::StreamTypeNotSupported)
        } else {
            Ok(self.config())
        }
    }
    fn default_output_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError> {
        if self.0.output {
            Ok(self.config())
        } else {
            Err(DefaultStreamConfigError::StreamTypeNotSupported)
        }
    }
    fn build_input_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        data_callback: D,
        error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<MockStream, BuildStreamError>
    where
        D: FnMut(&Data, &InputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        self.build(
            false,
            config,
            sample_format,
            Callback::Input(Box::new(data_callback)),
            Box::new(error_callback),
        )
    }
    fn build_output_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        data_callback: D,
        error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<MockStream, BuildStreamError>
    where
        D: FnMut(&mut Data, &OutputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        self.build(
            true,
            config,
            sample_format,
            Callback::Output(Box::new(data_callback)),
            Box::new(error_callback),
        )
    }
}

/// The test's view of a stream: its callbacks, while the stream exists.
pub struct MockStreamHandle {
    pub channels: usize,
    period: usize,
    sample_rate: u32,
    level: f32,
    callback: Mutex<Option<Callback>>,
    error: Mutex<Option<ErrorCallback>>,
    heap: AtomicUsize,
    calls: AtomicUsize,
    stop: AtomicBool,
}

fn output_info() -> OutputCallbackInfo {
    OutputCallbackInfo::new(OutputStreamTimestamp {
        callback: StreamInstant::new(0, 0),
        playback: StreamInstant::new(0, 0),
    })
}

fn input_info() -> InputCallbackInfo {
    InputCallbackInfo::new(InputStreamTimestamp {
        callback: StreamInstant::new(0, 0),
        capture: StreamInstant::new(0, 0),
    })
}

impl MockStreamHandle {
    /// Run the output callback for `frames` and return the interleaved samples.
    pub fn render(&self, frames: usize) -> Vec<f32> {
        let mut buffer = vec![f32::NAN; frames * self.channels];
        self.render_into(&mut buffer);
        buffer
    }

    fn render_into(&self, buffer: &mut [f32]) {
        let mut callback = self.callback.lock().unwrap();
        let Some(Callback::Output(callback)) = callback.as_mut() else {
            panic!("not a live output stream");
        };
        let info = output_info();
        // SAFETY: the buffer outlives `data`, holds f32 samples and has `len` of them.
        let mut data = unsafe {
            Data::from_parts(buffer.as_mut_ptr().cast(), buffer.len(), SampleFormat::F32)
        };
        let ((), counts) = measure(|| callback(&mut data, &info));
        self.count(counts);
    }

    /// Run the input callback with interleaved `samples`.
    pub fn capture(&self, samples: &[f32]) {
        let mut callback = self.callback.lock().unwrap();
        let Some(Callback::Input(callback)) = callback.as_mut() else {
            panic!("not a live input stream");
        };
        let info = input_info();
        // SAFETY: the samples outlive `data`, are f32 and are only read.
        let data = unsafe {
            Data::from_parts(
                samples.as_ptr().cast_mut().cast(),
                samples.len(),
                SampleFormat::F32,
            )
        };
        let ((), counts) = measure(|| callback(&data, &info));
        self.count(counts);
    }

    /// Report a stream error as the backend would.
    pub fn fail(&self, error: StreamError) {
        let mut callback = self.error.lock().unwrap();
        (callback.as_mut().expect("a live stream"))(error);
    }

    fn count(&self, counts: shoop_wasm_test_support::heap::Counts) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.heap.fetch_add(
            counts.allocations + counts.reallocations + counts.deallocations,
            Ordering::Relaxed,
        );
    }

    /// Heap operations inside data callbacks so far.
    pub fn heap_operations(&self) -> usize {
        self.heap.load(Ordering::Relaxed)
    }
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
    pub fn is_live(&self) -> bool {
        self.callback.lock().unwrap().is_some()
    }

    fn run(self: Arc<Self>) {
        let interval = Duration::from_secs_f64(self.period as f64 / f64::from(self.sample_rate));
        let mut buffer = vec![self.level; self.period * self.channels];
        while !self.stop.load(Ordering::Acquire) {
            let output = matches!(*self.callback.lock().unwrap(), Some(Callback::Output(_)));
            if output {
                self.render_into(&mut buffer);
            } else {
                buffer.fill(self.level);
                self.capture(&buffer);
            }
            std::thread::sleep(interval);
        }
    }
}

pub struct MockStream {
    handle: Arc<MockStreamHandle>,
    threaded: bool,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl StreamTrait for MockStream {
    fn play(&self) -> Result<(), PlayStreamError> {
        let mut thread = self.thread.lock().unwrap();
        if self.threaded && thread.is_none() {
            let handle = self.handle.clone();
            *thread = Some(std::thread::spawn(move || handle.run()));
        }
        Ok(())
    }
    fn pause(&self) -> Result<(), PauseStreamError> {
        Ok(())
    }
}

impl Drop for MockStream {
    fn drop(&mut self) {
        self.handle.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
        let callback = self.handle.callback.lock().unwrap().take();
        drop(callback);
        self.handle.error.lock().unwrap().take();
    }
}
