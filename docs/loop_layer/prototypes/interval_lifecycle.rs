//! Compile/run with rustc; a type/lifetime experiment, not the runtime API.
#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidInterval,
    InvalidBinding,
    ObservationFull,
}

pub trait Planner {
    type Decision: Copy;
    type Observation: Copy + Default;
    type Operation: Copy;

    fn apply(&mut self, at: u64, operation: Self::Operation) -> Result<(), Error>;
    fn plan(&mut self, at: u64, ceiling: u64) -> Result<u64, Error>;
    fn decisions(&self) -> &[Self::Decision];
    fn finish(&mut self, end: u64, observations: &[Self::Observation]) -> Result<(), Error>;
}

pub struct ObservationWriter<'a, O: Copy> {
    values: &'a mut Vec<O>,
    remaining: usize,
}

impl<O: Copy> ObservationWriter<'_, O> {
    pub fn push(&mut self, value: O) -> Result<(), Error> {
        if self.remaining == 0 || self.values.len() == self.values.capacity() {
            return Err(Error::ObservationFull);
        }
        self.values.push(value);
        self.remaining -= 1;
        Ok(())
    }
}

pub struct PlannedBlock<'a, L: Planner> {
    pub at: u64,
    pub end: u64,
    pub decision: &'a L::Decision,
    pub observations: ObservationWriter<'a, L::Observation>,
    pub output: &'a mut [u32],
}

pub trait Consumer<L: Planner> {
    fn process(&mut self, block: &mut PlannedBlock<'_, L>) -> Result<(), Error>;
}

pub struct Executor<L: Planner, P> {
    planner: L,
    consumers: Vec<P>,
    bindings: Vec<usize>,
    observations: Vec<L::Observation>,
    frame: u64,
    callback_starts: usize,
    intervals: usize,
}

impl<L: Planner, P: Consumer<L>> Executor<L, P> {
    pub fn prepare(planner: L, consumers: Vec<P>, bindings: Vec<usize>) -> Result<Self, Error> {
        if consumers.len() != bindings.len()
            || bindings
                .iter()
                .any(|&slot| slot >= planner.decisions().len())
        {
            return Err(Error::InvalidBinding);
        }
        let mut observations = vec![L::Observation::default(); consumers.len()];
        for value in &mut observations {
            *std::hint::black_box(value) = L::Observation::default();
        }
        observations.clear();
        Ok(Self {
            planner,
            consumers,
            bindings,
            observations,
            frame: 0,
            callback_starts: 0,
            intervals: 0,
        })
    }

    pub fn process(&mut self, output: &mut [u32]) -> Result<(), Error> {
        if output.is_empty() {
            return Ok(());
        }
        let base = self.frame;
        let ceiling = base
            .checked_add(output.len() as u64)
            .ok_or(Error::InvalidInterval)?;
        self.callback_starts += 1;
        output.fill(0);
        while self.frame < ceiling {
            let at = self.frame;
            let end = self.planner.plan(at, ceiling)?;
            if end <= at || end > ceiling {
                return Err(Error::InvalidInterval);
            }
            let decisions = self.planner.decisions();
            self.observations.clear();
            for (consumer, &binding) in self.consumers.iter_mut().zip(&self.bindings) {
                let decision = decisions.get(binding).ok_or(Error::InvalidBinding)?;
                let mut block = PlannedBlock {
                    at,
                    end,
                    decision,
                    observations: ObservationWriter {
                        values: &mut self.observations,
                        remaining: 1,
                    },
                    output: &mut output[(at - base) as usize..(end - base) as usize],
                };
                consumer.process(&mut block)?;
            }
            self.planner.finish(end, &self.observations)?;
            self.frame = end;
            self.intervals += 1;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
pub struct Observation {
    pub frames: u64,
    pub sum: u32,
}

pub struct SyntheticPlanner {
    decisions: [u32; 2],
    dense: bool,
    scale: u32,
    observed_frames: u64,
    observed_sum: u32,
}

impl SyntheticPlanner {
    pub fn new(dense: bool) -> Self {
        Self {
            decisions: [1, 3],
            dense,
            scale: 1,
            observed_frames: 0,
            observed_sum: 0,
        }
    }
}

impl Planner for SyntheticPlanner {
    type Decision = u32;
    type Observation = Observation;
    type Operation = u32;

    fn apply(&mut self, _at: u64, scale: u32) -> Result<(), Error> {
        self.scale = scale;
        Ok(())
    }

    fn plan(&mut self, at: u64, ceiling: u64) -> Result<u64, Error> {
        self.decisions = [if at < 5 { 1 } else { 2 }, 3].map(|value| value * self.scale);
        Ok(if self.dense {
            (at + 1).min(ceiling)
        } else if at < 5 {
            5.min(ceiling)
        } else {
            ceiling
        })
    }

    fn decisions(&self) -> &[u32] {
        &self.decisions
    }

    fn finish(&mut self, _end: u64, observations: &[Observation]) -> Result<(), Error> {
        for observation in observations {
            self.observed_frames += observation.frames;
            self.observed_sum += observation.sum;
        }
        Ok(())
    }
}

pub struct ScaleConsumer;

impl Consumer<SyntheticPlanner> for ScaleConsumer {
    fn process(&mut self, block: &mut PlannedBlock<'_, SyntheticPlanner>) -> Result<(), Error> {
        let mut sum = 0;
        for (offset, sample) in block.output.iter_mut().enumerate() {
            let contribution = (block.at as u32 + offset as u32) * *block.decision;
            *sample += contribution;
            sum += contribution;
        }
        block.observations.push(Observation {
            frames: block.end - block.at,
            sum,
        })
    }
}

fn run(partitions: &[usize], dense: bool, reversed: bool) -> (Vec<u32>, usize, usize) {
    let mut planner = SyntheticPlanner::new(dense);
    planner.apply(0, 2).unwrap();
    let bindings = if reversed { vec![1, 0] } else { vec![0, 1] };
    let mut executor =
        Executor::prepare(planner, vec![ScaleConsumer, ScaleConsumer], bindings).unwrap();
    let mut output = vec![0; partitions.iter().sum()];
    let mut offset = 0;
    for &frames in partitions {
        executor
            .process(&mut output[offset..offset + frames])
            .unwrap();
        offset += frames;
    }
    assert_eq!(executor.planner.observed_frames, output.len() as u64 * 2);
    assert_eq!(executor.planner.observed_sum, output.iter().sum());
    (output, executor.callback_starts, executor.intervals)
}

#[cfg(compile_fail_reuse)]
fn forbidden_reuse() {
    let mut planner = SyntheticPlanner::new(false);
    planner.plan(0, 10).unwrap();
    let decision = &planner.decisions()[0];
    planner.plan(5, 10).unwrap();
    std::hint::black_box(*decision);
}

fn main() {
    let expected: Vec<u32> = (0..128)
        .map(|frame| frame * if frame < 5 { 8 } else { 10 })
        .collect();
    assert_eq!(run(&[128], false, false), (expected.clone(), 1, 2));
    assert_eq!(
        run(&[0, 3, 2, 0, 7, 116], false, true),
        (expected.clone(), 4, 4)
    );
    assert_eq!(run(&[1; 128], false, false), (expected.clone(), 128, 128));
    assert_eq!(run(&[128], true, true), (expected, 1, 128));
    assert!(matches!(
        Executor::prepare(SyntheticPlanner::new(false), vec![ScaleConsumer], vec![2]),
        Err(Error::InvalidBinding)
    ));
    let mut values = Vec::with_capacity(1);
    let mut writer = ObservationWriter {
        values: &mut values,
        remaining: 1,
    };
    writer.push(3u32).unwrap();
    assert_eq!(writer.push(4), Err(Error::ObservationFull));
    assert_eq!(values, [3]);
    #[cfg(compile_fail_reuse)]
    forbidden_reuse();
    println!("planner prototype: 4 partition/density/order cases, invalid binding, observation refusal passed");
}
