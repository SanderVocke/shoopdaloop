use rt_fpga_testbench::pipeline::run_pipeline_case;
use rt_fpga_testbench::transitions::{run_transition_case, run_transition_oracle};
use rt_fpga_testbench::{run_audio_case, run_case, schema, Document, Payload};
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["schema"] {
        println!("{}", serde_json::to_string_pretty(&schema())?);
        return Ok(());
    }
    if args.len() != 1 || !["roundtrip", "compile", "trace", "check"].contains(&args[0].as_str()) {
        return Err(
            "usage: fpga-reference {schema|roundtrip|compile|trace|check}; JSON on stdin".into(),
        );
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let document = Document::from_json(&input)?;
    let payload = match (args[0].as_str(), document.payload) {
        ("roundtrip", payload) => payload,
        ("compile", Payload::Graph(graph)) => Payload::Compiled(
            rt_fpga_reference::compiler::compile(&graph)
                .map_err(|error| format!("compile failed: {error:?}"))?,
        ),
        ("trace", Payload::Case(case)) => Payload::Trace(run_case(case)),
        ("trace", Payload::PipelineCase(case)) => Payload::PipelineTrace(
            run_pipeline_case(case)
                .map_err(|error| format!("pipeline initialization failed: {error:?}"))?,
        ),
        ("check", Payload::PipelineTrace(trace)) => {
            if run_pipeline_case(trace.case.clone())
                .map_err(|error| format!("pipeline initialization failed: {error:?}"))?
                != trace
            {
                return Err("trace differs from shared-loop pipeline results".into());
            }
            Payload::PipelineTrace(trace)
        }
        ("trace", Payload::AudioCase(case)) => Payload::AudioTrace(
            run_audio_case(case)
                .map_err(|error| format!("audio initialization failed: {error:?}"))?,
        ),
        ("compile", Payload::TransitionPlan(plan)) => Payload::TransitionPlanImage(
            rt_fpga_reference::transitions::TransitionPlan::compile(plan.description, plan.limits)
                .map_err(|error| format!("transition compile failed: {error:?}"))?
                .image()
                .clone(),
        ),
        ("trace", Payload::TransitionCase(case)) => Payload::TransitionTrace(
            run_transition_case(case)
                .map_err(|error| format!("transition trace failed: {error:?}"))?,
        ),
        ("check", Payload::TransitionTrace(trace)) => {
            let model = run_transition_case(trace.case.clone())
                .map_err(|error| format!("transition model failed: {error:?}"))?;
            let oracle = run_transition_oracle(trace.case.clone())
                .map_err(|error| format!("transition oracle failed: {error:?}"))?;
            if model != trace || oracle != trace {
                return Err(
                    "transition trace differs from hardware model or native resolver results"
                        .into(),
                );
            }
            Payload::TransitionTrace(trace)
        }
        ("check", Payload::AudioTrace(trace)) => {
            if run_audio_case(trace.case.clone())
                .map_err(|error| format!("audio initialization failed: {error:?}"))?
                != trace
            {
                return Err("trace differs from Rust state/job processor results".into());
            }
            Payload::AudioTrace(trace)
        }
        ("check", Payload::Trace(trace)) => {
            if run_case(trace.case.clone()) != trace {
                return Err("trace differs from Rust state processor results".into());
            }
            Payload::Trace(trace)
        }
        _ => return Err("payload kind does not match command".into()),
    };
    println!("{}", Document::new(payload).to_json()?);
    Ok(())
}
