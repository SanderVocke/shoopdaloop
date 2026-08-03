use shoop_engine::content_snapshot::{ContentMutation, ContentSnapshotRuntime};
use std::hint::black_box;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const CHANNELS: usize = 32;
const SAMPLES: usize = 48_000;
const READS: usize = 2_000;
const WRITES: usize = 2_000;
const BLOCK: usize = 64;

fn per_operation(elapsed: Duration, operations: usize) -> f64 {
    elapsed.as_secs_f64() * 1_000_000.0 / operations as f64
}

fn main() {
    let source = vec![0.25_f32; SAMPLES];
    let legacy: Vec<_> = (0..CHANNELS).map(|_| Mutex::new(source.clone())).collect();

    let runtime = ContentSnapshotRuntime::new();
    let mut writers = Vec::with_capacity(CHANNELS);
    let mut readers = Vec::with_capacity(CHANNELS);
    for _ in 0..CHANNELS {
        let (mut writer, control, reader) = runtime.create_audio_channel(1_024, 64);
        let prepared = control
            .prepare(&source, ContentMutation::Loading)
            .expect("prepare initial generation");
        assert!(writer.install_prepared(prepared));
        writers.push(writer);
        readers.push(reader);
    }
    let start = Instant::now();
    while readers.iter().any(|reader| reader.try_current().is_err()) {
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::yield_now();
    }

    let start = Instant::now();
    for index in 0..READS {
        let copy = legacy[index % CHANNELS]
            .lock()
            .expect("legacy lock")
            .clone();
        black_box(copy);
    }
    let legacy_read = start.elapsed();

    let start = Instant::now();
    for index in 0..READS {
        black_box(readers[index % CHANNELS].latest().snapshot);
    }
    let snapshot_read = start.elapsed();

    let start = Instant::now();
    for index in 0..WRITES {
        let mut content = legacy[index % CHANNELS].lock().expect("legacy lock");
        content.copy_from_slice(&source);
        black_box(&*content);
    }
    let legacy_write = start.elapsed();

    let update = [0.75_f32; BLOCK];
    let start = Instant::now();
    for index in 0..WRITES {
        let writer = &mut writers[index % CHANNELS];
        assert!(writer.begin_mutation(ContentMutation::Recording));
        while writer
            .publish_range((index * BLOCK) % SAMPLES, &update, SAMPLES, true)
            .is_none()
        {
            thread::yield_now();
        }
        writer.finish_mutation(false);
    }
    let snapshot_write = start.elapsed();

    println!("channels={CHANNELS} samples_per_channel={SAMPLES}");
    println!(
        "legacy_mutex_full_copy_read_us={:.3}",
        per_operation(legacy_read, READS)
    );
    println!(
        "snapshot_manifest_read_us={:.3}",
        per_operation(snapshot_read, READS)
    );
    println!(
        "legacy_mutex_full_copy_write_us={:.3}",
        per_operation(legacy_write, WRITES)
    );
    println!(
        "snapshot_bounded_block_publish_us={:.3}",
        per_operation(snapshot_write, WRITES)
    );
    println!(
        "one_full_generation_mib={:.3}",
        (CHANNELS * SAMPLES * size_of::<f32>()) as f64 / (1024.0 * 1024.0)
    );
}
