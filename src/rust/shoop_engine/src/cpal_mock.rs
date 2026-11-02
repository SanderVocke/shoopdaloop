//! A software [`cpal::traits::HostTrait`] for tests.
//!
//! The tests under `tests/` exercise the audio-thread path of [`crate::cpal_driver`]
//! without going through ALSA / CoreAudio / WASAPI.
//!
//! `MockHost` implements the cpal host, device, and stream traits as a platform
//! host would. The `CpalTest` driver uses it for virtual-port routing on headless
//! CI where no real audio device exists.

#![cfg(feature = "cpal")]

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, Data, DeviceDescription, DeviceDescriptionBuilder, DeviceDirection, DeviceId,
    DeviceType, Error, ErrorKind, InputCallbackInfo, InputStreamTimestamp, InterfaceType,
    OutputCallbackInfo, OutputStreamTimestamp, SampleFormat, StreamConfig, StreamInstant,
    SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigRange,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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

    fn devices(&self) -> Result<Self::Devices, Error> {
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

    fn devices(&self) -> Result<Self::Devices, Error> {
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MockDevice {
    name: String,
    direction: Direction,
    n_channels: u16,
    sample_rate: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Direction {
    Input,
    Output,
}

impl std::fmt::Display for MockDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
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
            self.sample_rate,
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
                48_000,
                48_000,
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
                48_000,
                48_000,
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

    fn description(&self) -> Result<DeviceDescription, Error> {
        let direction = match self.direction {
            Direction::Input => DeviceDirection::Input,
            Direction::Output => DeviceDirection::Output,
        };
        Ok(DeviceDescriptionBuilder::new(&self.name)
            .device_type(DeviceType::Virtual)
            .interface_type(InterfaceType::Virtual)
            .direction(direction)
            .build())
    }

    fn id(&self) -> Result<DeviceId, Error> {
        Ok(DeviceId::new(cpal::default_host().id(), &self.name))
    }

    fn supported_input_configs(&self) -> Result<Self::SupportedInputConfigs, Error> {
        Ok(MockSupportedInputConfigs { yielded: false })
    }

    fn supported_output_configs(&self) -> Result<Self::SupportedOutputConfigs, Error> {
        Ok(MockSupportedOutputConfigs { yielded: false })
    }

    fn default_input_config(&self) -> Result<SupportedStreamConfig, Error> {
        if self.direction == Direction::Input {
            Ok(self.supported_config())
        } else {
            Err(Error::new(ErrorKind::UnsupportedOperation))
        }
    }

    fn default_output_config(&self) -> Result<SupportedStreamConfig, Error> {
        if self.direction == Direction::Output {
            Ok(self.supported_config())
        } else {
            Err(Error::new(ErrorKind::UnsupportedOperation))
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
        config: StreamConfig,
        sample_format: SampleFormat,
        mut data_callback: D,
        _error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<Self::Stream, Error>
    where
        D: FnMut(&mut Data, &OutputCallbackInfo) + Send + 'static,
        E: FnMut(Error) + Send + 'static,
    {
        assert_eq!(
            sample_format,
            SampleFormat::F32,
            "MockHost only drives the engine with f32 samples"
        );

        let channels = config.channels as usize;
        let sample_rate = config.sample_rate;
        let frames_per_cycle = self.frames_per_cycle(&config);
        let buffer_len = frames_per_cycle * channels;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let cycle_interval = Duration::from_secs_f64(frames_per_cycle as f64 / sample_rate as f64);

        let clock_origin = Instant::now();

        let thread = std::thread::Builder::new()
            .name("engine-cpal-mock-output".to_string())
            .spawn(move || {
                shoop_tracing::prewarm_realtime_thread("engine-cpal-mock-output");
                // Allocated once outside the loop so the audio path does not allocate.
                let mut buffer: Vec<f32> = vec![0.0; buffer_len];
                while !stop_thread.load(Ordering::Relaxed) {
                    let now = stream_instant(clock_origin);
                    let info = OutputCallbackInfo::new(OutputStreamTimestamp {
                        callback: now,
                        playback: now,
                    });
                    let mut data = unsafe { make_data(&mut buffer, sample_format) };
                    data_callback(&mut data, &info);
                    std::thread::sleep(cycle_interval);
                }
            })
            .expect("spawn CPAL mock output thread");

        Ok(MockStream {
            stop: Some(stop),
            thread: Some(thread),
            frames_per_cycle: frames_per_cycle as cpal::FrameCount,
            clock_origin,
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
        config: StreamConfig,
        sample_format: SampleFormat,
        mut data_callback: D,
        _error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<Self::Stream, Error>
    where
        D: FnMut(&Data, &InputCallbackInfo) + Send + 'static,
        E: FnMut(Error) + Send + 'static,
    {
        assert_eq!(
            sample_format,
            SampleFormat::F32,
            "MockHost only feeds the engine f32 samples"
        );

        let channels = config.channels as usize;
        let sample_rate = config.sample_rate;
        let frames_per_cycle = self.frames_per_cycle(&config);
        let buffer_len = frames_per_cycle * channels;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let cycle_interval = Duration::from_secs_f64(frames_per_cycle as f64 / sample_rate as f64);

        let clock_origin = Instant::now();

        let thread = std::thread::Builder::new()
            .name("engine-cpal-mock-input".to_string())
            .spawn(move || {
                shoop_tracing::prewarm_realtime_thread("engine-cpal-mock-input");
                let buffer: Vec<f32> = vec![0.0; buffer_len];
                while !stop_thread.load(Ordering::Relaxed) {
                    let now = stream_instant(clock_origin);
                    let info = InputCallbackInfo::new(InputStreamTimestamp {
                        callback: now,
                        capture: now,
                    });
                    let data = unsafe { make_data_const(&buffer, sample_format) };
                    data_callback(&data, &info);
                    std::thread::sleep(cycle_interval);
                }
            })
            .expect("spawn CPAL mock input thread");

        Ok(MockStream {
            stop: Some(stop),
            thread: Some(thread),
            frames_per_cycle: frames_per_cycle as cpal::FrameCount,
            clock_origin,
        })
    }
}

/// # Safety
/// The buffer must outlive every use of the returned data.
unsafe fn make_data(buffer: &mut [f32], sample_format: SampleFormat) -> Data {
    assert_eq!(sample_format, SampleFormat::F32);
    Data::from_parts(buffer.as_mut_ptr().cast(), buffer.len(), sample_format)
}

/// # Safety
/// The buffer must outlive every use of the returned data, which must remain read-only.
unsafe fn make_data_const(buffer: &[f32], sample_format: SampleFormat) -> Data {
    assert_eq!(sample_format, SampleFormat::F32);
    Data::from_parts(
        buffer.as_ptr().cast_mut().cast(),
        buffer.len(),
        sample_format,
    )
}

fn stream_instant(origin: Instant) -> StreamInstant {
    let elapsed = origin.elapsed();
    StreamInstant::new(elapsed.as_secs(), elapsed.subsec_nanos())
}

/// A stream backed by a single thread that fires the data callback at a steady rate.
///
/// `play`/`pause` are no-ops: the thread is already running, because the callback is
/// what does the work. Dropping the stream tells the thread to stop and joins it, so
/// the engine never outlives the audio thread.
pub struct MockStream {
    stop: Option<Arc<AtomicBool>>,
    thread: Option<JoinHandle<()>>,
    frames_per_cycle: cpal::FrameCount,
    clock_origin: Instant,
}

impl StreamTrait for MockStream {
    fn buffer_size(&self) -> Result<cpal::FrameCount, Error> {
        Ok(self.frames_per_cycle)
    }

    fn now(&self) -> StreamInstant {
        stream_instant(self.clock_origin)
    }

    fn play(&self) -> Result<(), Error> {
        Ok(())
    }
    fn pause(&self) -> Result<(), Error> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[shoop_wasm_test_support::shoop_test]
    fn mock_device_identity_is_stable_and_distinguishes_directions() {
        let output = MockDevice::output();
        let input = MockDevice::input();
        let devices =
            std::collections::HashSet::from([output.clone(), output.clone(), input.clone()]);
        assert_eq!(devices.len(), 2);
        assert_eq!(output.to_string(), "mock-output");
        assert_eq!(input.to_string(), "mock-input");
        assert_eq!(output.id().unwrap(), output.clone().id().unwrap());
        assert_ne!(output.id().unwrap(), input.id().unwrap());
        assert_eq!(
            output.default_input_config().unwrap_err().kind(),
            ErrorKind::UnsupportedOperation
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn mock_streams_report_callback_size_and_share_the_callback_clock() {
        for buffer_size in [BufferSize::Default, BufferSize::Fixed(128)] {
            for direction in [Direction::Input, Direction::Output] {
                let device = match direction {
                    Direction::Input => MockDevice::input(),
                    Direction::Output => MockDevice::output(),
                };
                let mut config = device.supported_config().config();
                config.buffer_size = buffer_size;
                let expected_frames = device.frames_per_cycle(&config);
                let (tx, rx) = mpsc::channel();
                let stream = match direction {
                    Direction::Input => device.build_input_stream(
                        config,
                        move |data: &[f32], info| {
                            assert!(data.iter().all(|sample| *sample == 0.0));
                            let _ = tx.send((data.len(), info.timestamp().callback));
                        },
                        |error| panic!("mock input stream: {error}"),
                        None,
                    ),
                    Direction::Output => device.build_output_stream(
                        config,
                        move |data: &mut [f32], info| {
                            data.fill(0.5);
                            let _ = tx.send((data.len(), info.timestamp().callback));
                        },
                        |error| panic!("mock output stream: {error}"),
                        None,
                    ),
                }
                .unwrap();
                stream.play().unwrap();
                assert_eq!(stream.buffer_size().unwrap() as usize, expected_frames);
                let (samples, first) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
                let (_, second) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(samples, expected_frames * 2);
                assert!(second >= first);
                let now = stream.now();
                assert!(now >= second);
                assert!(now.duration_since(second) < Duration::from_secs(5));
            }
        }
    }
}
