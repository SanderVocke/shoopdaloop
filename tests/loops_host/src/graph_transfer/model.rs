use super::*;
use crate::guarded;

#[derive(Clone)]
struct BankModel {
    generation: u32,
    phase: Phase,
    epoch: Epoch,
    counts: Counts,
    planned: bool,
    script: Vec<Payload>,
    next: usize,
    accepted: Vec<Payload>,
}
impl BankModel {
    fn new() -> Self {
        Self {
            generation: 1,
            phase: Phase::Idle,
            epoch: Epoch(0),
            counts: Counts {
                steps: 0,
                edges: 0,
                bindings: 0,
            },
            planned: false,
            script: Vec::new(),
            next: 0,
            accepted: Vec::new(),
        }
    }
    fn id(&self, slot: usize) -> Id {
        Id {
            slot: slot as u32,
            generation: self.generation.max(1),
        }
    }
    fn retire_generation(&mut self) {
        self.generation = self.generation.checked_add(1).unwrap_or(0);
        self.phase = Phase::Idle;
    }
}
struct Model {
    last: u64,
    banks: [BankModel; BANKS],
    accepted_kinds: u8,
    refusals: [usize; 6],
    loans: usize,
    returns: usize,
}
fn random(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed
}
fn script(counts: Counts, planned: bool, nonce: u64) -> Vec<Payload> {
    let generation = ((nonce >> 32) as u32).max(1);
    let node = |slot| NodeId::from_parts(slot, generation).unwrap();
    let planner = PlannerId::from_parts(0, generation).unwrap();
    let mut result = Vec::new();
    for offset in 0..counts.steps {
        result.push(Payload::Step {
            offset,
            step: NodeStep {
                node: node(offset),
                input_end: counts.edges as usize,
            },
        });
    }
    for offset in 0..counts.edges {
        result.push(Payload::Edge {
            offset,
            edge: Connection {
                source: Endpoint::host(PortType::Audio, offset),
                destination: Endpoint::node(node(0), PortType::Audio, offset),
            },
        });
    }
    if planned {
        result.push(Payload::Planning(PlanningStep {
            planner: PlannerSpec {
                id: planner,
                decision_slots: 2,
                observation_capacity: 2,
                operation_bytes: 64,
            },
            before_step: counts.steps as usize,
        }));
    }
    for offset in 0..counts.bindings {
        result.push(Payload::Binding {
            offset,
            binding: DecisionBinding {
                planner,
                consumer: node(offset),
                decision_slot: offset as usize,
            },
        });
    }
    result.push(Payload::Seal);
    result
}
impl Model {
    fn ingest(
        &mut self,
        request: Record,
        malformed: bool,
        proposed: Vec<Payload>,
    ) -> Result<Phase, Error> {
        if malformed {
            return Err(Error::InvalidShape);
        }
        if request.session != 7 {
            return Err(Error::InvalidId);
        }
        if request.sequence <= self.last {
            return Err(Error::InvalidState);
        }
        let bank = &mut self.banks[request.id.slot as usize];
        if bank.generation == 0 || request.id.generation != bank.generation {
            return Err(Error::InvalidId);
        }
        let kind = match request.payload {
            Payload::Begin {
                epoch,
                counts,
                planned,
            } => {
                if bank.phase != Phase::Idle {
                    return Err(Error::Busy);
                }
                if counts.steps > 4 || counts.edges > 16 || counts.bindings > 4 {
                    return Err(Error::Capacity);
                }
                if !planned && counts.bindings != 0 {
                    return Err(Error::InvalidShape);
                }
                bank.epoch = epoch;
                bank.counts = counts;
                bank.planned = planned;
                bank.script = proposed;
                bank.next = 0;
                bank.accepted.clear();
                bank.phase = Phase::Receiving;
                0
            }
            Payload::Abort => {
                if !matches!(bank.phase, Phase::Receiving | Phase::Sealed) {
                    return Err(Error::Busy);
                }
                bank.retire_generation();
                6
            }
            payload => {
                if bank.phase != Phase::Receiving {
                    return Err(Error::Busy);
                }
                let expected = bank.script[bank.next];
                let valid = match (payload, expected) {
                    (Payload::Step { offset: a, .. }, Payload::Step { offset: b, .. })
                    | (Payload::Edge { offset: a, .. }, Payload::Edge { offset: b, .. })
                    | (Payload::Binding { offset: a, .. }, Payload::Binding { offset: b, .. }) => {
                        a == b
                    }
                    (Payload::Planning(_), Payload::Planning(_))
                    | (Payload::Seal, Payload::Seal) => true,
                    _ => false,
                };
                if !valid {
                    return Err(Error::InvalidOffset);
                }
                bank.next += 1;
                match payload {
                    Payload::Seal => {
                        bank.phase = Phase::Sealed;
                        5
                    }
                    Payload::Step { .. } => {
                        bank.accepted.push(payload);
                        1
                    }
                    Payload::Edge { .. } => {
                        bank.accepted.push(payload);
                        2
                    }
                    Payload::Planning(_) => {
                        bank.accepted.push(payload);
                        3
                    }
                    Payload::Binding { .. } => {
                        bank.accepted.push(payload);
                        4
                    }
                    _ => unreachable!(),
                }
            }
        };
        self.accepted_kinds |= 1 << kind;
        self.last = request.sequence;
        Ok(bank.phase)
    }
    fn check(
        &self,
        receiver: &Receiver,
        held: &[Option<Loan>; BANKS],
        pointers: &[[usize; 3]; BANKS],
    ) {
        assert_eq!(receiver.last_sequence(), self.last);
        for slot in 0..BANKS {
            let model = &self.banks[slot];
            let actual = &receiver.banks[slot];
            assert_eq!(actual.generation, model.generation);
            assert_eq!(actual.phase, model.phase);
            assert_eq!(actual.epoch, model.epoch);
            assert_eq!(actual.counts, model.counts);
            assert_eq!(actual.planned, model.planned);
            assert_eq!(actual.owner.is_none(), model.phase == Phase::Loaned);
            assert_eq!(held[slot].is_some(), model.phase == Phase::Loaned);
            let owner = actual
                .owner
                .as_ref()
                .unwrap_or_else(|| &held[slot].as_ref().unwrap().plan);
            assert_eq!(storage(owner), pointers[slot]);
            let steps: Vec<_> = model
                .accepted
                .iter()
                .filter_map(|payload| match payload {
                    Payload::Step { step, .. } => Some(*step),
                    _ => None,
                })
                .collect();
            let edges: Vec<_> = model
                .accepted
                .iter()
                .filter_map(|payload| match payload {
                    Payload::Edge { edge, .. } => Some(*edge),
                    _ => None,
                })
                .collect();
            let bindings: Vec<_> = model
                .accepted
                .iter()
                .filter_map(|payload| match payload {
                    Payload::Binding { binding, .. } => Some(*binding),
                    _ => None,
                })
                .collect();
            let planning = model.accepted.iter().find_map(|payload| match payload {
                Payload::Planning(step) => Some(*step),
                _ => None,
            });
            assert_eq!(owner.descriptor().steps, steps);
            assert_eq!(owner.descriptor().connections, edges);
            assert_eq!(owner.planning().bindings, bindings);
            assert_eq!(owner.planning().step, planning);
            if model.generation == 0 {
                assert_eq!(receiver.id(slot), Err(Error::Exhausted));
            } else {
                let id = model.id(slot);
                assert_eq!(receiver.id(slot), Ok(id));
                assert_eq!(receiver.phase(id), Ok(model.phase));
                if model.phase == Phase::Loaned {
                    assert_eq!(receiver.capacities(id), Err(Error::Busy));
                } else {
                    assert_eq!(
                        receiver.capacities(id),
                        Ok(Counts {
                            steps: 4,
                            edges: 16,
                            bindings: 4
                        })
                    );
                }
            }
        }
    }
}
fn storage(plan: &PreparedPlan) -> [usize; 3] {
    [
        plan.descriptor().steps.as_ptr() as usize,
        plan.descriptor().connections.as_ptr() as usize,
        plan.planning().bindings.as_ptr() as usize,
    ]
}
fn deliver(
    receiver: &mut Receiver,
    model: &mut Model,
    request: Record,
    malformed: bool,
    proposed: Vec<Payload>,
) {
    let mut bytes = request.encode().unwrap();
    if malformed {
        bytes[7] = 1;
    }
    let expected = model.ingest(request, malformed, proposed);
    let actual = guarded(|| receiver.ingest(&bytes));
    assert_eq!(actual, expected);
    if let Err(error) = expected {
        let index = match error {
            Error::InvalidShape => 0,
            Error::InvalidId => 1,
            Error::InvalidState => 2,
            Error::Busy => 3,
            Error::InvalidOffset => 4,
            Error::Capacity => 5,
            _ => panic!("unexpected model error {error:?}"),
        };
        model.refusals[index] += 1;
    }
}
fn take(receiver: &mut Receiver, model: &mut Model, held: &mut [Option<Loan>; BANKS], slot: usize) {
    let bank = &mut model.banks[slot];
    let result = guarded(|| receiver.take(bank.id(slot)));
    if bank.generation == 0 {
        assert!(matches!(result, Err(Error::InvalidId)));
    } else if bank.phase != Phase::Sealed {
        assert!(matches!(result, Err(Error::Busy)));
    } else {
        let loan = result.unwrap();
        assert_eq!(loan.id, bank.id(slot));
        assert_eq!(loan.epoch, bank.epoch);
        assert!(held[slot].is_none());
        held[slot] = Some(loan);
        bank.phase = Phase::Loaned;
        model.loans += 1;
    }
}
fn give_back(
    receiver: &mut Receiver,
    model: &mut Model,
    held: &mut [Option<Loan>; BANKS],
    slot: usize,
) {
    let loan = held[slot].take().unwrap();
    let wrong = Id {
        generation: if loan.id.generation == 1 {
            2
        } else {
            loan.id.generation - 1
        },
        ..loan.id
    };
    let (error, owner) = guarded(|| receiver.recycle(wrong, loan.plan))
        .err()
        .unwrap();
    assert_eq!(error, Error::InvalidId);
    guarded(|| receiver.recycle(loan.id, owner))
        .unwrap_or_else(|(error, _)| panic!("return {error:?}"));
    model.banks[slot].retire_generation();
    model.returns += 1;
}

#[test]
fn seeded_descriptor_bank_model_preserves_complete_tables_and_exact_owners_after_every_action() {
    assert!(crate::allocator_canaries());
    let config = rt_runtime::Config {
        max_nodes: 4,
        max_edges: 16,
        ..rt_runtime::Config::default()
    }
    .prepare()
    .unwrap();
    let mut total_kinds = 0;
    let mut total_refusals = [0; 6];
    let mut total_returns = 0;
    for (iteration, mut seed) in [0x3c91_d713_u64, 0x917f_a421, 0xc85a_7139, 0xffff_1221]
        .into_iter()
        .enumerate()
    {
        let mut receiver = Receiver::new(7, config).unwrap();
        let pointers =
            std::array::from_fn(|slot| storage(receiver.banks[slot].owner.as_ref().unwrap()));
        let mut held: [Option<Loan>; BANKS] = std::array::from_fn(|_| None);
        let mut model = Model {
            last: 0,
            banks: std::array::from_fn(|_| BankModel::new()),
            accepted_kinds: 0,
            refusals: [0; 6],
            loans: 0,
            returns: 0,
        };
        if iteration % 2 == 1 {
            receiver.banks[1].generation = u32::MAX - 1;
            model.banks[1].generation = u32::MAX - 1;
        }
        for action in 0..512_u64 {
            let bits = random(&mut seed);
            let slot = ((bits >> 32) as usize) % BANKS;
            let bank = &model.banks[slot];
            let kind = bits % 16;
            if kind < 8 && bank.generation != 0 && bank.phase == Phase::Sealed || kind == 15 {
                take(&mut receiver, &mut model, &mut held, slot);
            } else if kind < 8 && bank.phase == Phase::Loaned {
                give_back(&mut receiver, &mut model, &mut held, slot);
            } else {
                let planned = bits & 0x100 != 0;
                let counts = Counts {
                    steps: ((bits >> 9) % 3) as u32,
                    edges: ((bits >> 13) % 3) as u32,
                    bindings: if planned {
                        ((bits >> 17) % 3) as u32
                    } else {
                        0
                    },
                };
                let normal = match bank.phase {
                    Phase::Idle => Payload::Begin {
                        epoch: Epoch(bits),
                        counts,
                        planned,
                    },
                    Phase::Receiving => bank.script[bank.next],
                    Phase::Sealed | Phase::Loaned => Payload::Seal,
                };
                let mut request = Record {
                    session: 7,
                    sequence: (1_u64 << 63) + action + 1,
                    id: bank.id(slot),
                    payload: normal,
                };
                match kind {
                    8 => request.payload = Payload::Abort,
                    10 => request.session = 8,
                    11 => {
                        request.id.generation = if bank.generation == 1 {
                            2
                        } else {
                            bank.generation.saturating_sub(1).max(1)
                        }
                    }
                    12 => request.sequence = model.last.max(1),
                    13 => {
                        request.payload = Payload::Begin {
                            epoch: Epoch(u64::MAX),
                            counts: Counts {
                                steps: u32::MAX,
                                edges: 0,
                                bindings: 0,
                            },
                            planned: false,
                        }
                    }
                    14 => {
                        request.payload = Payload::Step {
                            offset: u32::MAX,
                            step: NodeStep {
                                node: NodeId::from_parts(0, 1).unwrap(),
                                input_end: usize::MAX,
                            },
                        }
                    }
                    _ => {}
                }
                let proposed = script(counts, planned, bits);
                deliver(&mut receiver, &mut model, request, kind == 9, proposed);
            }
            model.check(&receiver, &held, &pointers);
        }
        for slot in 0..BANKS {
            if held[slot].is_some() {
                give_back(&mut receiver, &mut model, &mut held, slot);
            }
            let bank = &model.banks[slot];
            if bank.generation != 0 && matches!(bank.phase, Phase::Receiving | Phase::Sealed) {
                let request = Record {
                    session: 7,
                    sequence: model.last + 1,
                    id: bank.id(slot),
                    payload: Payload::Abort,
                };
                deliver(&mut receiver, &mut model, request, false, Vec::new());
            }
            model.check(&receiver, &held, &pointers);
        }
        let id = model.banks[0].id(0);
        let counts = Counts {
            steps: 0,
            edges: 0,
            bindings: 0,
        };
        let request = Record {
            session: 7,
            sequence: model.last + 1,
            id,
            payload: Payload::Begin {
                epoch: Epoch(u64::MAX),
                counts,
                planned: false,
            },
        };
        deliver(
            &mut receiver,
            &mut model,
            request,
            false,
            script(counts, false, 0),
        );
        let last = Record {
            sequence: u64::MAX,
            payload: Payload::Abort,
            ..request
        };
        deliver(&mut receiver, &mut model, last, false, Vec::new());
        deliver(&mut receiver, &mut model, last, false, Vec::new());
        model.check(&receiver, &held, &pointers);
        assert_eq!(model.last, u64::MAX);
        assert!(model.loans > 0 && model.returns == model.loans);
        if iteration % 2 == 1 {
            assert_eq!(model.banks[1].generation, 0);
        }
        assert!(receiver.banks.iter().all(|bank| bank.owner.is_some()));
        total_kinds |= model.accepted_kinds;
        for (total, count) in total_refusals.iter_mut().zip(model.refusals) {
            *total += count;
        }
        total_returns += model.returns;
    }
    assert_eq!(total_kinds, 0x7f);
    assert!(total_refusals.iter().all(|count| *count > 0));
    assert!(total_returns > 0);
}
