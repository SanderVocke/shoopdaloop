//! A software [`cpal::traits::HostTrait`] for tests.
//!
//! The tests under `tests/` exercise the audio-thread path of [`crate::cpal_driver`]
//! without going through ALSA / CoreAudio / WASAPI.
//!
//! `MockHost` implements the four cpal traits -- [`HostTrait`], [`DeviceTrait`],
//! [`StreamTrait`] -- the same way a platform host would. It is also used by
//! the `CpalTest` driver type when the QML self-tests exercise CPAL virtual port
//! routing on headless CI where no real audio device exists.

#![cfg(feature = "cpal")]

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, BuildStreamError, Data, DefaultStreamConfigError, DeviceNameError, DevicesError,
    InputCallbackInfo, InputStreamTimestamp, OutputCallbackInfo, OutputStreamTimestamp,
    PauseStreamError, PlayStreamError, SampleFormat, SampleRate, StreamConfig, StreamError,
    StreamInstant, SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigRange,
    SupportedStreamConfigsError,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Mirror of `cpal::Data`'s layout, declared `#[repr(C)]` so the field offsets are
/// guaranteed and `transmute_copy` to `Data` is sound.
///
/// cpal's `Data` has private fields and no `#[repr]` attribute, so its layout is the
/// default Rust layout: for a struct of three Copy fields with the alignments
/// observed here (8, 8, 1) the source-order layout coincides with `#[repr(C)]` on
/// every platform cpal 0.16 targets. If a future release adds a `#[repr]` or
/// reorders fields, the tests will start failing loudly in the audio callback -- the
/// desired failure mode for a layout mismatch.
#[repr(C)]
struct DataLayout {
    data: *mut (),
    len: usize,
    sample_format: SampleFormat,
}

/// A host with a default output device and a default input device, both stereo at
/// 48 kHz.
#[derive(Clone)]
pub struct MockHost {
    output_device: MockDevice,
    input_device: MockDevice,
}

impl MockHost {
    pub fn new() -> Self {
        Self {
            output_device: MockDevice::output(),
            input_device: MockDevice::input(),
        }
    }
}

impl Default for MockHost {
    fn default() -> Self {
        Self::new()
    }
}

impl HostTrait for MockHost {
    type Devices = MockDevices;
    type Device = MockDevice;

    fn is_available() -> bool {
        true
    }

    fn devices(&self) -> Result<Self::Devices, DevicesError> {
        Ok(MockDevices {
            items: vec![self.output_device.clone(), self.input_device.clone()],
            index: 0,
        })
    }

    fn default_input_device(&self) -> Option<Self::Device> {
        Some(self.input_device.clone())
    }

    fn default_output_device(&self) -> Option<Self::Device> {
        Some(self.output_device.clone())
    }
}

/// Host with no default output device. The input device is real, so a duplex test
/// still fails fast on the missing output rather than silently misbehaving.
#[derive(Clone)]
pub struct MockHostNoOutput {
    input_device: MockDevice,
}

impl MockHostNoOutput {
    pub fn with_default_input() -> Self {
        Self {
            input_device: MockDevice::input(),
        }
    }
}

impl HostTrait for MockHostNoOutput {
    type Devices = MockDevices;
    type Device = MockDevice;

    fn is_available() -> bool {
        true
    }

    fn devices(&self) -> Result<Self::Devices, DevicesError> {
        Ok(MockDevices {
            items: vec![self.input_device.clone()],
            index: 0,
        })
    }

    fn default_input_device(&self) -> Option<Self::Device> {
        Some(self.input_device.clone())
    }

    fn default_output_device(&self) -> Option<Self::Device> {
        None
    }
}

/// Iterator over a host's devices. Yields each device once, in order.
pub struct MockDevices {
    items: Vec<MockDevice>,
    index: usize,
}

impl Iterator for MockDevices {
    type Item = MockDevice;

    fn next(&mut self) -> Option<MockDevice> {
        let item = self.items.get(self.index).cloned();
        self.index += 1;
        item
    }
}

#[derive(Clone)]
pub struct MockDevice {
    name: String,
    direction: Direction,
    n_channels: u16,
    sample_rate: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Input,
    Output,
}

impl MockDevice {
    fn output() -> Self {
        Self {
            name: "mock-output".into(),
            direction: Direction::Output,
            n_channels: 2,
            sample_rate: 48_000,
        }
    }

    fn input() -> Self {
        Self {
            name: "mock-input".into(),
            direction: Direction::Input,
            n_channels: 2,
            sample_rate: 48_000,
        }
    }

    fn supported_config(&self) -> SupportedStreamConfig {
        SupportedStreamConfig::new(
            self.n_channels,
            SampleRate(self.sample_rate),
            SupportedBufferSize::Range { min: 64, max: 8192 },
            SampleFormat::F32,
        )
    }

    fn frames_per_cycle(&self, cfg: &StreamConfig) -> usize {
        match cfg.buffer_size {
            BufferSize::Default => 64,
            BufferSize::Fixed(n) => (n as usize).max(1),
        }
    }
}

/// Iterator yielding the single supported output config.
pub struct MockSupportedOutputConfigs {
    yielded: bool,
}

impl Iterator for MockSupportedOutputConfigs {
    type Item = SupportedStreamConfigRange;
    fn next(&mut self) -> Option<SupportedStreamConfigRange> {
        if self.yielded {
            None
        } else {
            self.yielded = true;
            Some(SupportedStreamConfigRange::new(
                2,
                SampleRate(48_000),
                SampleRate(48_000),
                SupportedBufferSize::Range { min: 64, max: 8192 },
                SampleFormat::F32,
            ))
        }
    }
}

/// Iterator yielding the single supported input config.
pub struct MockSupportedInputConfigs {
    yielded: bool,
}

impl Iterator for MockSupportedInputConfigs {
    type Item = SupportedStreamConfigRange;
    fn next(&mut self) -> Option<SupportedStreamConfigRange> {
        if self.yielded {
            None
        } else {
            self.yielded = true;
            Some(SupportedStreamConfigRange::new(
                2,
                SampleRate(48_000),
                SampleRate(48_000),
                SupportedBufferSize::Range { min: 64, max: 8192 },
                SampleFormat::F32,
            ))
        }
    }
}

impl DeviceTrait for MockDevice {
    type SupportedInputConfigs = MockSupportedInputConfigs;
    type SupportedOutputConfigs = MockSupportedOutputConfigs;
    type Stream = MockStream;

    fn name(&self) -> Result<String, DeviceNameError> {
        Ok(self.name.clone())
    }

    fn supported_input_configs(
        &self,
    ) -> Result<Self::SupportedInputConfigs, SupportedStreamConfigsError> {
        Ok(MockSupportedInputConfigs { yielded: false })
    }

    fn supported_output_configs(
        &self,
    ) -> Result<Self::SupportedOutputConfigs, SupportedStreamConfigsError> {
        Ok(MockSupportedOutputConfigs { yielded: false })
    }

    fn default_input_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError> {
        if self.direction == Direction::Input {
            Ok(self.supported_config())
        } else {
            Err(DefaultStreamConfigError::StreamTypeNotSupported)
        }
    }

    fn default_output_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError> {
        if self.direction == Direction::Output {
            Ok(self.supported_config())
        } else {
            Err(DefaultStreamConfigError::StreamTypeNotSupported)
        }
    }

    /// Builds an output stream backed by a thread that fires the data callback at a
    /// steady rate.
    ///
    /// Allocates a `Vec<f32>` buffer once, then in each cycle constructs a fresh
    /// [`cpal::Data`] pointing into it and hands it to the callback. The callback is
    /// the one provided by the typed `build_output_stream` default, which calls
    /// `data.as_slice_mut::<f32>()` and dispatches to the user's `&mut [f32]`
    /// callback -- so the production code's path is exercised exactly.
    fn build_output_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        mut data_callback: D,
        _error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        D: FnMut(&mut Data, &OutputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        assert_eq!(
            sample_format,
            SampleFormat::F32,
            "MockHost only drives the engine with f32 samples"
        );

        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0;
        let frames_per_cycle = self.frames_per_cycle(config);
        let buffer_len = frames_per_cycle * channels;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let cycle_interval = Duration::from_secs_f64(frames_per_cycle as f64 / sample_rate as f64);

        let info = OutputCallbackInfo::new(OutputStreamTimestamp {
            callback: StreamInstant::new(0, 0),
            playback: StreamInstant::new(0, 0),
        });

        let thread = std::thread::Builder::new()
            .name("engine-cpal-mock-output".to_string())
            .spawn(move || {
                shoop_tracing::prewarm_realtime_thread("engine-cpal-mock-output");
                // Allocated once outside the loop so the audio path does not allocate.
                let mut buffer: Vec<f32> = vec![0.0; buffer_len];
                while !stop_thread.load(Ordering::Relaxed) {
                    let mut data = unsafe { make_data(&mut buffer, sample_format) };
                    data_callback(&mut data, &info);
                    std::thread::sleep(cycle_interval);
                }
            })
            .expect("spawn CPAL mock output thread");

        Ok(MockStream {
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// Builds an input stream backed by a thread that fires the data callback at a
    /// steady rate.
    ///
    /// The mock's input data is silence. That is what a real OS host would deliver
    /// for an unplugged capture device, so a test that wires the input into the
    /// engine's ring will see real underrun accounting rather than synthetic
    /// activity.
    fn build_input_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        mut data_callback: D,
        _error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        D: FnMut(&Data, &InputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        assert_eq!(
            sample_format,
            SampleFormat::F32,
            "MockHost only feeds the engine f32 samples"
        );

        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0;
        let frames_per_cycle = self.frames_per_cycle(config);
        let buffer_len = frames_per_cycle * channels;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let cycle_interval = Duration::from_secs_f64(frames_per_cycle as f64 / sample_rate as f64);

        let info = InputCallbackInfo::new(InputStreamTimestamp {
            callback: StreamInstant::new(0, 0),
            capture: StreamInstant::new(0, 0),
        });

        let thread = std::thread::Builder::new()
            .name("engine-cpal-mock-input".to_string())
            .spawn(move || {
                shoop_tracing::prewarm_realtime_thread("engine-cpal-mock-input");
                let buffer: Vec<f32> = vec![0.0; buffer_len];
                while !stop_thread.load(Ordering::Relaxed) {
                    let data = unsafe { make_data_const(&buffer, sample_format) };
                    data_callback(&data, &info);
                    std::thread::sleep(cycle_interval);
                }
            })
            .expect("spawn CPAL mock input thread");

        Ok(MockStream {
            stop: Some(stop),
            thread: Some(thread),
        })
    }
}

/// Construct an output [`cpal::Data`] pointing into a mutable f32 buffer.
///
/// `cpal::Data` has private fields and no public constructor in 0.16, so the only
/// way to get one is to lay out the fields ourselves and `transmute_copy`. The
/// `DataLayout` struct above mirrors the field order with `#[repr(C)]` so the
/// layouts match.
///
/// SAFETY: the caller must guarantee:
/// - `buffer` lives at least as long as the returned `Data` is used
/// - `buffer.len()` matches the number of samples the consumer expects
/// - `sample_format` matches the buffer's sample type
unsafe fn make_data(buffer: &mut [f32], sample_format: SampleFormat) -> Data {
    debug_assert!(sample_format == SampleFormat::F32);
    let layout = DataLayout {
        data: buffer.as_mut_ptr() as *mut (),
        len: buffer.len(),
        sample_format,
    };
    // `Data` is POD (no Drop, all fields Copy), so transmuting from a same-sized
    // `#[repr(C)]` value with matching field layout is sound as long as DataLayout
    // matches Data's actual layout -- see the comment on `DataLayout`.
    std::mem::transmute_copy::<DataLayout, Data>(&layout)
}

/// Construct an input [`cpal::Data`] pointing into an immutable f32 buffer.
///
/// See [`make_data`] for the layout contract.
unsafe fn make_data_const(buffer: &[f32], sample_format: SampleFormat) -> Data {
    debug_assert!(sample_format == SampleFormat::F32);
    let layout = DataLayout {
        data: buffer.as_ptr() as *const () as *mut (),
        len: buffer.len(),
        sample_format,
    };
    std::mem::transmute_copy::<DataLayout, Data>(&layout)
}

/// A stream backed by a single thread that fires the data callback at a steady rate.
///
/// `play`/`pause` are no-ops: the thread is already running, because the callback is
/// what does the work. Dropping the stream tells the thread to stop and joins it, so
/// the engine never outlives the audio thread.
pub struct MockStream {
    stop: Option<Arc<AtomicBool>>,
    thread: Option<JoinHandle<()>>,
}

impl StreamTrait for MockStream {
    fn play(&self) -> Result<(), PlayStreamError> {
        Ok(())
    }
    fn pause(&self) -> Result<(), PauseStreamError> {
        Ok(())
    }
}

impl Drop for MockStream {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(thread) = self.thread.take() {
            // The thread sleeps at the cycle interval, so the join is bounded by it.
            // If it cannot finish, the test has hit a real deadlock; we swallow the
            // error rather than panic in Drop.
            let _ = thread.join();
        }
    }
}
