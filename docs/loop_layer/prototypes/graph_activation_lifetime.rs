use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::{
        self,
        activation::{GraphAccess, StageId},
        GraphDescription, PortCounts,
    },
    planner::*,
    processor::{ProcessBlock, Processor},
    resource::ResourceStore,
    schedule::Action,
    transport::{Lane, Outcome},
    Config, Epoch, Error, Frame, PlannerId,
};

fn activate(mut graphs: GraphAccess<'_>, id: StageId) -> Result<Epoch, Error> {
    #[cfg(hold_graph)]
    let graph = graphs.active().unwrap();
    #[cfg(hold_candidate)]
    let candidate = graphs.candidate(id)?;
    let permit = graphs.prepare_activation(id, Epoch(1))?;
    #[cfg(hold_graph)]
    assert!(graph.planning().step.is_some());
    #[cfg(hold_candidate)]
    assert!(candidate.planning().step.is_some());
    let current = permit.current().unwrap();
    #[cfg(hold_permit)]
    let _ = permit.commit();
    assert!(current.planning().step.is_some());
    Ok(permit.commit())
}
#[cfg(escape)]
fn escape(mut graphs: GraphAccess<'_>, id: StageId) -> graph::activation::GraphActivation<'static> {
    graphs.prepare_activation(id, Epoch(1)).unwrap()
}
struct Domain;
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
impl Planner for Domain {
    type Decision = ();
    type Observation = ();
    type Operation = StageId;
    type Result = Epoch;
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 0,
            observation_capacity: 0,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[()] {
        &[]
    }
    fn plan(&mut self, context: PlanContext<'_, StageId, Epoch>) -> Result<Frame, Error> {
        Ok(context.ceiling)
    }
    fn finish(&mut self, _context: FinishContext<'_, (), StageId, Epoch>) -> Result<(), Error> {
        Ok(())
    }
    fn operation(
        &mut self,
        id: StageId,
        context: OperationContext<'_, StageId, Epoch>,
    ) -> Result<Disposition<Epoch>, Error> {
        Ok(Disposition::Complete(activate(context.graphs, id)?))
    }
}
struct Silent;
impl Processor<Domain> for Silent {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn process(&mut self, _block: &mut ProcessBlock<'_, Domain>) -> Result<(), Error> {
        Ok(())
    }
}
fn main() {
    #[cfg(forge)]
    let _ = GraphAccess::new;
    #[cfg(escape)]
    let _ = escape;
    let config = Config {
        max_nodes: 1,
        max_ports: 1,
        max_edges: 1,
        audio_inputs: 0,
        audio_outputs: 0,
        midi_inputs: 0,
        midi_outputs: 0,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 16,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        Vec::<PreparedNode<Silent, Domain>>::new(),
        Domain,
        ResourceStore::new(config).unwrap(),
        &GraphDescription::default(),
        &[],
    )
    .unwrap();
    runtime.prepare_graph_staging(1).unwrap();
    let candidate =
        graph::compile_planned(runtime.catalog(), &GraphDescription::default(), &[]).unwrap();
    control
        .submit(Lane::Normal, Command::stage_graph(0, Epoch(1), candidate))
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    runtime
        .process(
            AudioBlock {
                frame: Frame(0),
                frames: 1,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
        .unwrap();
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.outcome, Outcome::Applied);
    let Command::StageGraph {
        id: Some(id),
        plan: None,
        ..
    } = completion.request
    else {
        panic!("stage")
    };
    control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(1),
                action: Action::Planner {
                    planner: planner(),
                    operation: id,
                },
                payload: Box::new([]),
            },
        )
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    runtime
        .process(
            AudioBlock {
                frame: Frame(1),
                frames: 1,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
        .unwrap();
    assert_eq!(runtime.epoch(), Epoch(2));
    let result = control.try_execution().unwrap();
    assert_eq!(result.result, Ok(()));
    assert_eq!(result.operation_result, Some(Epoch(2)));
}
