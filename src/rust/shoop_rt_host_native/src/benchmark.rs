use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

const CAPACITY: usize = 8192;
const FIELDS: usize = 18;

struct Queue {
    slots: Box<[[AtomicU64; FIELDS]]>,
    read: AtomicUsize,
    write: AtomicUsize,
    dropped: AtomicU64,
    stop: AtomicBool,
}

pub(crate) struct Recorder {
    queue: Arc<Queue>,
    worker: Option<JoinHandle<()>>,
}

impl Recorder {
    pub fn prepare(
        description: &shoop_rt_engine::EngineDescription,
    ) -> std::io::Result<Option<Self>> {
        let Some(path) = std::env::var_os("SHOOP_AUDIO_BENCH_OUTPUT") else {
            return Ok(None);
        };
        let file = File::create(path)?;
        let queue = Arc::new(Queue {
            slots: (0..CAPACITY)
                .map(|_| std::array::from_fn(|_| AtomicU64::new(0)))
                .collect(),
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        });
        let reader = queue.clone();
        let mode = description.audio_render_mode.name();
        let memory = description
            .history_capacity()
            .map_err(|_| std::io::Error::other("audio memory layout"))?;
        let sample_rate = description.sample_rate;
        let quantum = description.quantum;
        let worker = std::thread::Builder::new()
            .name("audio-benchmark-writer".into())
            .spawn(move || {
                let mut output = BufWriter::new(file);
                let _ = writeln!(output, "# mode={mode} sample_rate={sample_rate} quantum={quantum}");
                let _ = writeln!(output, "# memory={memory:?}");
                let _ = writeln!(output, "frame,frames,elapsed_ns,late_ns,resource_copy_bytes,resource_steps,nodes_processed,render_fault,forwarded,materialized,sample_reads,sample_writes,arena_peak_samples,index_allocated,index_reclaimed,shared_payloads,payload_live,index_live");
                loop {
                    let mut read = reader.read.load(Ordering::Relaxed);
                    let write = reader.write.load(Ordering::Acquire);
                    while read != write {
                        let values = reader.slots[read % CAPACITY]
                            .each_ref()
                            .map(|value| value.load(Ordering::Relaxed));
                        for (i,value) in values.iter().enumerate() {
                            if i>0 { let _ = write!(output, ","); }
                            let _ = write!(output,"{value}");
                        }
                        let _ = writeln!(output);

                        read = read.wrapping_add(1);
                        reader.read.store(read, Ordering::Release);
                    }
                    if reader.stop.load(Ordering::Acquire) {
                        if read == reader.write.load(Ordering::Acquire) {
                            break;
                        }
                    } else {
                        let _ = output.flush();
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
                let _ = writeln!(output, "# dropped={}", reader.dropped.load(Ordering::Relaxed));
                let _ = output.flush();
            })?;
        Ok(Some(Self {
            queue,
            worker: Some(worker),
        }))
    }

    pub fn record(&self, values: [u64; FIELDS]) {
        let write = self.queue.write.load(Ordering::Relaxed);
        if write.wrapping_sub(self.queue.read.load(Ordering::Acquire)) == CAPACITY {
            self.queue.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        for (target, value) in self.queue.slots[write % CAPACITY].iter().zip(values) {
            target.store(value, Ordering::Relaxed);
        }
        self.queue
            .write
            .store(write.wrapping_add(1), Ordering::Release);
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.queue.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
