//! Initial empty mono-loop graphs with fixed stereo Master routing.
use crate::*;

#[derive(Clone, Copy, Debug)]
pub struct LoopDescription {
    pub id: Identity,
    pub source: Option<u32>,
    pub input: bool,
    pub output: bool,
    pub gain: f32,
    pub repeat_sync: bool,
}
pub struct GraphDescription {
    pub quantum: u32,
    pub loops: Vec<LoopDescription>,
}
impl GraphDescription {
    pub fn compile(&self) -> Result<LogicalStateProgram, CompileError> {
        self.compile_with_limits(ProgramLimits::default())
    }
    pub fn compile_with_limits(
        &self,
        limits: ProgramLimits,
    ) -> Result<LogicalStateProgram, CompileError> {
        if self.loops.is_empty() {
            return Err(CompileError::InvalidProgram);
        }
        let n =
            u32::try_from(self.loops.len()).map_err(|_| CompileError::Capacity(Resource::Loops))?;
        // These are the minimum instructions for any routing of these loops.
        if u64::from(n) * 4 + 4 > u64::from(limits.instructions) {
            return Err(CompileError::Capacity(Resource::Instructions));
        }
        let outputs = self.loops.iter().filter(|l| l.output).count() as u32;
        let jobs = n
            .checked_mul(4)
            .and_then(|v| v.checked_add(outputs))
            .and_then(|v| v.checked_add(4))
            .ok_or(CompileError::Capacity(Resource::Jobs))?;
        let buffers = n
            .checked_mul(2)
            .and_then(|v| v.checked_add(outputs))
            .and_then(|v| v.checked_add(2))
            .ok_or(CompileError::Capacity(Resource::Buffers))?;
        let mut mixes = 0;
        let mut sum = 1;
        let mut loops = Vec::new();
        let mut bindings = Vec::new();
        let mut initial_state = Vec::new();
        let mut audio_pass = vec![
            Operation::Input {
                port: 0,
                destination: 0,
            },
            Operation::Silence { destination: 1 },
        ];
        for (i, l) in self.loops.iter().enumerate() {
            if !l.gain.is_finite() || !(0.0..=1.0).contains(&l.gain) {
                return Err(CompileError::InvalidProgram);
            }
            let source = l
                .source
                .map(|slot| {
                    self.loops
                        .iter()
                        .find(|s| s.id.slot == slot)
                        .map(|s| s.id)
                        .ok_or(CompileError::InvalidProgram)
                })
                .transpose()?;
            loops.push(Loop {
                id: l.id,
                length: 0,
                sync_source: source,
                repeat_sync: l.repeat_sync,
            });
            initial_state.push(InitialValue::Ramp(Ramp {
                start: (f64::from(l.gain) * f64::from(1 << 24)) as i32,
                target: (f64::from(l.gain) * f64::from(1 << 24)) as i32,
                duration: 0,
                elapsed: 0,
            }));
            let view = n + i as u32;
            let playback = 2 + 2 * i as u32;
            let meter = playback + 1;
            audio_pass.extend([
                Operation::Playback {
                    loop_id: l.id,
                    view,
                    destination: playback,
                },
                Operation::Gain {
                    ramp: i as u32,
                    source: playback,
                    destination: meter,
                },
                Operation::Capture {
                    loop_id: l.id,
                    view,
                    source: u32::from(!l.input),
                },
            ]);
            if l.output {
                let destination = 2 + 2 * n + mixes;
                mixes += 1;
                audio_pass.push(Operation::Mix {
                    left: sum,
                    right: meter,
                    destination,
                });
                sum = destination;
            }
            bindings.push(LoopBinding {
                id: l.id,
                gain: Some(i as u32),
                view: Some(view),
                meter: Some(meter),
            });
        }
        initial_state.extend((0..n).map(InitialValue::MediaView));
        audio_pass.extend([
            Operation::Output {
                source: sum,
                port: 0,
            },
            Operation::Output {
                source: sum,
                port: 1,
            },
        ]);
        let mut sync_pass = Vec::new();
        while sync_pass.len() < loops.len() {
            let next = loops
                .iter()
                .filter(|l| !sync_pass.contains(&l.id))
                .filter(|l| {
                    l.sync_source
                        .is_none_or(|s| s == l.id || sync_pass.contains(&s))
                })
                .min_by_key(|l| (l.id.slot, l.id.generation));
            let Some(next) = next else {
                return Err(CompileError::InvalidProgram);
            };
            sync_pass.push(next.id);
        }
        let program = LogicalStateProgram {
            quantum: self.quantum,
            chunk_frames: 1,
            sync_pass,
            audio_pass,
            initial_state,
            loops,
            bindings,
            jobs_per_quantum: jobs,
            resources: Resources {
                queue_capacity: jobs,
                buffers,
                history_slots: 0,
                input_ports: 1,
                output_ports: 2,
                events: 4 * n,
            },
        };
        program.validate_logical(limits)?;
        Ok(program)
    }
}
