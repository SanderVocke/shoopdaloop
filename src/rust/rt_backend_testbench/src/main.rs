use rt_backend_testbench::{audit, production, rtl, spec::*, transition_fixture as tp};
use std::io::{self, Write};
mod transition_service;

fn main() {
    if let Err(error) = execute() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn execute() -> std::result::Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "schema") {
        let schemas = serde_json::json!({"format_version":FORMAT_VERSION,
            "scenario":schemars::schema_for!(Scenario),"result":schemars::schema_for!(Result),"image":schemars::schema_for!(rtl::Image),"audit":schemars::schema_for!(audit::Report),"transition_case":schemars::schema_for!(tp::TransitionCase),"transition_image":schemars::schema_for!(tp::TransitionPlanImage),"transition_trace":schemars::schema_for!(tp::TransitionTrace)});
        println!(
            "{}",
            serde_json::to_string_pretty(&schemas).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let command = args.first().map(String::as_str).unwrap_or("run");
    if command == "transition-service" {
        return transition_service::run();
    }
    if matches!(command, "transition-run" | "transition-compile") {
        let stdin = io::stdin();
        for source in
            serde_json::Deserializer::from_reader(stdin.lock()).into_iter::<tp::TransitionCase>()
        {
            let case = source.map_err(|e| e.to_string())?;
            if case.format_version != FORMAT_VERSION {
                return Err("unsupported format_version".into());
            }
            let value = if command == "transition-run" {
                serde_json::to_value(tp::run_transition_oracle(case).map_err(|e| format!("{e:?}"))?)
            } else {
                serde_json::to_value(tp::compile(&case.plan).map_err(|e| format!("{e:?}"))?)
            }
            .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string(&value).map_err(|e| e.to_string())?
            );
        }
        return Ok(());
    }
    if command == "roundtrip" {
        let kind = if args.get(1).map(String::as_str) == Some("--kind") {
            args.get(2).map(String::as_str)
        } else {
            None
        };
        return match kind {
            Some("scenario") => roundtrip::<Scenario>(),
            Some("image") => roundtrip::<rtl::Image>(),
            Some("result") => roundtrip::<Result>(),
            Some("audit") => roundtrip::<audit::Report>(),
            Some("transition_case") => roundtrip::<tp::TransitionCase>(),
            Some("transition_image") => roundtrip::<tp::TransitionPlanImage>(),
            Some("transition_trace") => roundtrip::<tp::TransitionTrace>(),
            _ => Err("roundtrip requires --kind scenario|image|result|audit|transition_case|transition_image|transition_trace".into()),
        };
    }
    if !matches!(command, "run" | "compile" | "validate" | "audit") {
        return Err("usage: rt-backend-testbench run [--numeric float|q824] | compile | validate | schema; input is JSONL".into());
    }
    let float = match args.get(1).map(String::as_str) {
        None => false,
        Some("--numeric") => match args.get(2).map(String::as_str) {
            Some("float") => true,
            Some("q824") => false,
            _ => return Err("expected --numeric float|q824".into()),
        },
        _ => return Err("unknown option".into()),
    };
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for (index, scenario) in serde_json::Deserializer::from_reader(stdin.lock())
        .into_iter::<Scenario>()
        .enumerate()
    {
        let scenario = scenario.map_err(|e| format!("document {}: {e}", index + 1))?;
        scenario
            .validate()
            .map_err(|e| format!("line {}: {e}", index + 1))?;
        let output = match command {
            "compile" => serde_json::to_value(rtl::compile(&scenario)?),
            "audit" => serde_json::to_value(audit::run(&scenario)?),
            "validate" => serde_json::to_value(scenario),
            _ => serde_json::to_value(if float {
                production::run::<f32>(&scenario)?
            } else {
                production::run::<rt_runtime::Q824>(&scenario)?
            }),
        }
        .map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut stdout, &output).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
        stdout.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn roundtrip<T: serde::de::DeserializeOwned + serde::Serialize>() -> std::result::Result<(), String>
{
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for document in serde_json::Deserializer::from_reader(stdin.lock()).into_iter::<T>() {
        let value = document.map_err(|e| e.to_string())?;
        let version = serde_json::to_value(&value).map_err(|e| e.to_string())?;
        if version["format_version"].as_u64() != Some(u64::from(FORMAT_VERSION)) {
            return Err("unsupported format_version".into());
        }
        serde_json::to_writer(&mut stdout, &value).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    }
    Ok(())
}
