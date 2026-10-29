//! Control-instance graph records and bounded credits. No local pointer crosses wire.
use crate::{
    browser::protocol_error,
    graph_transfer::{self, Counts, Id, Payload, Record},
    graph_wire, scenario,
};
use rt_runtime::{graph::activation::StageId, Epoch};
use std::sync::{atomic::AtomicU32, Arc};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct BrowserGraphClient {
    client: graph_wire::Client,
    session: u64,
    template: Box<[Payload]>,
    pub(crate) payload: [u8; graph_transfer::RECORD_BYTES],
    pub(crate) request: [u8; graph_wire::REQUEST_BYTES],
    pub(crate) response: [u8; graph_wire::RESPONSE_BYTES],
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl BrowserGraphClient {
    /// Allocating preparation of the fixture's public-API-compiled host-output bypass.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(session: u64) -> Result<Self, u32> {
        let client = graph_wire::Client::new(session).map_err(protocol_error)?;
        let (control, engine, _) = scenario::prepare_with_fault(0, Arc::new(AtomicU32::new(0)));
        let plan = control
            .compile_graph(&scenario::routes(true))
            .map_err(crate::wire::runtime_error)?;
        let mut template = vec![Payload::Begin {
            epoch: Epoch(1),
            counts: Counts {
                steps: plan.descriptor().steps.len() as u32,
                edges: plan.descriptor().connections.len() as u32,
                bindings: plan.planning().bindings.len() as u32,
            },
            planned: plan.planning().step.is_some(),
        }];
        template.extend(
            plan.descriptor()
                .steps
                .iter()
                .enumerate()
                .map(|(offset, &step)| Payload::Step {
                    offset: offset as u32,
                    step,
                }),
        );
        template.extend(
            plan.descriptor()
                .connections
                .iter()
                .enumerate()
                .map(|(offset, &edge)| Payload::Edge {
                    offset: offset as u32,
                    edge,
                }),
        );
        if let Some(step) = plan.planning().step {
            template.push(Payload::Planning(step));
        }
        template.extend(
            plan.planning()
                .bindings
                .iter()
                .enumerate()
                .map(|(offset, &binding)| Payload::Binding {
                    offset: offset as u32,
                    binding,
                }),
        );
        template.push(Payload::Seal);
        drop(control);
        let mut stopped = engine.into_stopped();
        stopped.cleanup().map_err(crate::wire::runtime_error)?;
        for _ in 0..512 {
            stopped.resources().begin_callback();
            stopped.resources().reclaim();
        }
        let usage = stopped.resource_usage();
        if (
            usage.free_pages,
            usage.free_versions,
            usage.free_snapshots,
            stopped.media_usage().references,
        ) != (256, 16, 16, 0)
        {
            return Err(10);
        }
        Ok(Self {
            client,
            session,
            template: template.into_boxed_slice(),
            payload: [0; graph_transfer::RECORD_BYTES],
            request: [0; graph_wire::REQUEST_BYTES],
            response: [0; graph_wire::RESPONSE_BYTES],
        })
    }
    pub fn payload_count(&self) -> u32 {
        self.template.len() as u32
    }
    /// The caller supplies a strictly increasing payload sequence, separate from commands.
    pub fn prepare_payload(
        &mut self,
        index: u32,
        bank_slot: u32,
        bank_generation: u32,
        epoch: u64,
        sequence: u64,
    ) -> u32 {
        crate::guarded(|| {
            let mut payload = *self
                .template
                .get(index as usize)
                .ok_or(crate::wire::Error::Slot)?;
            if let Payload::Begin { epoch: current, .. } = &mut payload {
                *current = Epoch(epoch);
            }
            self.payload = Record {
                session: self.session,
                sequence,
                id: Id {
                    slot: bank_slot,
                    generation: bank_generation,
                },
                payload,
            }
            .encode()
            .map_err(|_| crate::wire::Error::Identity)?;
            Ok(())
        })
        .map_or_else(protocol_error, |_| 0)
    }
    pub fn stage(
        &mut self,
        bank_slot: u32,
        bank_generation: u32,
        slot: u32,
        epoch: u64,
        cancelled: bool,
    ) -> u32 {
        self.prepare(
            graph_wire::Operation::Stage {
                bank: Id {
                    slot: bank_slot,
                    generation: bank_generation,
                },
                slot,
                epoch: Epoch(epoch),
            },
            cancelled,
        )
    }
    pub fn activate(
        &mut self,
        scope: u32,
        slot: u32,
        generation: u32,
        at: u64,
        graph_epoch: u64,
        topology_epoch: u64,
        cancelled: bool,
    ) -> u32 {
        let graph = match StageId::from_parts(scope, slot, generation) {
            Ok(id) => id,
            Err(_) => return 6,
        };
        self.prepare(
            graph_wire::Operation::Activate {
                graph,
                at,
                graph_epoch: Epoch(graph_epoch),
                topology_epoch: Epoch(topology_epoch),
            },
            cancelled,
        )
    }
    pub fn release(&mut self, scope: u32, slot: u32, generation: u32, cancelled: bool) -> u32 {
        let graph = match StageId::from_parts(scope, slot, generation) {
            Ok(id) => id,
            Err(_) => return 6,
        };
        self.prepare(graph_wire::Operation::Release { graph }, cancelled)
    }
    pub fn receive(&mut self) -> u32 {
        crate::guarded(|| self.client.receive(&self.response)).map_or_else(protocol_error, |_| 0)
    }
    pub fn outstanding(&self) -> u32 {
        self.client.outstanding() as u32
    }
    pub fn retained_graphs(&self) -> u32 {
        self.client.retained_graphs() as u32
    }
    pub fn payload_ptr(&self) -> usize {
        self.payload.as_ptr() as usize
    }
    pub fn request_ptr(&self) -> usize {
        self.request.as_ptr() as usize
    }
    pub fn response_ptr(&mut self) -> usize {
        self.response.as_mut_ptr() as usize
    }
}
impl BrowserGraphClient {
    fn prepare(&mut self, operation: graph_wire::Operation, cancelled: bool) -> u32 {
        match crate::guarded(|| self.client.prepare(operation, cancelled)) {
            Ok(bytes) => {
                self.request = bytes;
                0
            }
            Err(error) => protocol_error(error),
        }
    }
}
