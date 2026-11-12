use rt_backend_testbench::{production, rtl, spec::*};
use std::io::{self, Write};

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
            "scenario":schemars::schema_for!(Scenario),"result":schemars::schema_for!(Result),"image":schemars::schema_for!(rtl::Image)});
        println!(
            "{}",
            serde_json::to_string_pretty(&schemas).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let command = args.first().map(String::as_str).unwrap_or("run");
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
            _ => Err("roundtrip requires --kind scenario|image|result".into()),
        };
    }
    if !matches!(command, "run" | "compile" | "validate") {
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
        serde_json::to_writer(&mut stdout, &value).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    }
    Ok(())
}
