//! Single-issue metadata interpreter. There is no sample-buffer or media-store access.

use alloc::vec::Vec;

use crate::{
    fixed::Ramp,
    jobs::{BufferId, Job, JobQueue},
    storage::ViewId,
    Error,
};

#[derive(Clone, Debug)]
pub struct Program {
    pub instructions: Vec<Instruction>,
    pub quantum: u32,
    pub jobs_per_quantum: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Address(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Register(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Word(u64),
    Ramp(Ramp),
}

impl Value {
    pub fn word(self) -> Result<u64, Error> {
        match self {
            Self::Word(value) => Ok(value),
            _ => Err(Error::InvalidState),
        }
    }

    pub fn ramp(self) -> Result<Ramp, Error> {
        match self {
            Self::Ramp(value) => Ok(value),
            _ => Err(Error::InvalidState),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobTemplate {
    Literal(Job),
    ReadView {
        view: Register,
        position: Register,
        length: Register,
        enabled: Register,
        destination: BufferId,
    },
    Gain {
        ramp: Register,
        source: BufferId,
        destination: BufferId,
    },
    Capture {
        view: Register,
        enabled: Register,
        source: BufferId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Instruction {
    Load {
        address: Address,
        destination: Register,
    },
    Store {
        source: Register,
        address: Address,
    },
    AdvancePosition {
        position: Register,
        length: Register,
        enabled: Register,
    },
    AdvanceRamp {
        ramp: Register,
    },
    Emit(JobTemplate),
}

pub struct StateProcessor {
    memory: Vec<Value>,
    registers: [Value; 4],
}

impl StateProcessor {
    pub fn new(memory: Vec<Value>) -> Self {
        Self {
            memory,
            registers: [Value::Word(0); 4],
        }
    }

    pub fn prepare(capacity: usize) -> Self {
        Self::new(Vec::with_capacity(capacity))
    }

    pub fn reset(&mut self, values: &[Value]) -> Result<(), Error> {
        if values.len() > self.memory.capacity() {
            return Err(Error::InvalidState);
        }
        self.memory.clear();
        self.memory.extend_from_slice(values);
        Ok(())
    }

    pub fn read(&self, address: Address) -> Result<Value, Error> {
        self.memory
            .get(address.0)
            .copied()
            .ok_or(Error::InvalidState)
    }

    pub fn write(&mut self, address: Address, value: Value) -> Result<(), Error> {
        *self.memory.get_mut(address.0).ok_or(Error::InvalidState)? = value;
        Ok(())
    }

    fn register(&self, register: Register) -> Result<Value, Error> {
        self.registers
            .get(register.0)
            .copied()
            .ok_or(Error::InvalidState)
    }

    fn set_register(&mut self, register: Register, value: Value) -> Result<(), Error> {
        *self
            .registers
            .get_mut(register.0)
            .ok_or(Error::InvalidState)? = value;
        Ok(())
    }

    pub fn run(&mut self, program: &Program, queue: &mut JobQueue) -> Result<(), Error> {
        // Refuse a new quantum before changing state if its jobs cannot fit.
        if !queue.is_empty() || queue.remaining() < program.jobs_per_quantum {
            return Err(Error::QueueFull);
        }
        self.registers.fill(Value::Word(0));
        for &instruction in &program.instructions {
            match instruction {
                Instruction::Load {
                    address,
                    destination,
                } => {
                    self.set_register(destination, self.read(address)?)?;
                }
                Instruction::Store { source, address } => {
                    self.write(address, self.register(source)?)?
                }
                Instruction::AdvancePosition {
                    position,
                    length,
                    enabled,
                } => {
                    if self.register(enabled)?.word()? != 0 {
                        let length = self.register(length)?.word()?;
                        if length == 0 {
                            return Err(Error::InvalidState);
                        }
                        let next = (u128::from(self.register(position)?.word()?)
                            + u128::from(program.quantum))
                            % u128::from(length);
                        self.set_register(position, Value::Word(next as u64))?;
                    }
                }
                Instruction::AdvanceRamp { ramp } => {
                    let mut value = self.register(ramp)?.ramp()?;
                    value.advance(program.quantum);
                    self.set_register(ramp, Value::Ramp(value))?;
                }
                Instruction::Emit(template) => {
                    if let Some(job) = self.resolve(template)? {
                        queue.push(job)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn resolve(&self, template: JobTemplate) -> Result<Option<Job>, Error> {
        let word = |register| self.register(register)?.word();
        let view_id = |register| -> Result<ViewId, Error> {
            Ok(ViewId(
                usize::try_from(word(register)?).map_err(|_| Error::InvalidView)?,
            ))
        };
        Ok(Some(match template {
            JobTemplate::Literal(job) => job,
            JobTemplate::ReadView {
                view,
                position,
                length,
                enabled,
                destination,
            } => {
                if word(enabled)? == 0 {
                    Job::Silence { destination }
                } else {
                    Job::ReadView {
                        view: view_id(view)?,
                        position: word(position)?,
                        length: word(length)?,
                        destination,
                    }
                }
            }
            JobTemplate::Gain {
                ramp,
                source,
                destination,
            } => Job::Gain {
                ramp: self.register(ramp)?.ramp()?,
                source,
                destination,
            },
            JobTemplate::Capture {
                view,
                enabled,
                source,
            } => {
                if word(enabled)? == 0 {
                    return Ok(None);
                }
                Job::Capture {
                    view: view_id(view)?,
                    source,
                }
            }
        }))
    }
}
