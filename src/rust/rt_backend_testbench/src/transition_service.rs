use rt_fpga_adapter::{
    image::TransitionImage, looping::TpReply, transition_service::TransitionService,
};
use serde::Deserialize;
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Configure {
        image: TransitionImage,
    },
    Begin {
        opcode: u8,
        index: u32,
        words: [u32; 20],
    },
    Finish {
        reply: TpReply,
    },
    Observe {
        opcode: u8,
        reply: TpReply,
    },
    Static {
        words: [u32; 20],
    },
    Disconnect,
}

pub fn run() -> Result<(), String> {
    let mut service = None;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let input: Command =
            serde_json::from_str(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let output = if let Command::Configure { image } = input {
            service = Some(TransitionService::new(&image).map_err(|e| e.to_string())?);
            serde_json::json!({"ok":true,"metadata_bytes":service.as_ref().expect("configured").metadata_bytes()})
        } else {
            let service = service.as_mut().ok_or("service is not configured")?;
            match input {
                Command::Begin {
                    opcode,
                    index,
                    words,
                } => {
                    let start = std::time::Instant::now();
                    let result = service.begin(opcode, index, words);
                    let service_ns = start.elapsed().as_nanos() as u64;
                    match result {
                        Ok(transfer) => {
                            serde_json::json!({"transfer":transfer,"service_ns":service_ns})
                        }
                        Err(error) => serde_json::json!({"error":error,"service_ns":service_ns}),
                    }
                }
                Command::Finish { reply } => {
                    let start = std::time::Instant::now();
                    let result = service.finish(reply);
                    let service_ns = start.elapsed().as_nanos() as u64;
                    match result {
                        Ok(reply) => serde_json::json!({"reply":reply,"service_ns":service_ns}),
                        Err(error) => {
                            serde_json::json!({"fault":error.to_string(),"service_ns":service_ns})
                        }
                    }
                }
                Command::Observe { opcode, reply } => {
                    service.observe(opcode, &reply);
                    serde_json::json!({"ok":true})
                }
                Command::Static { words } => match service.prepare_static(words) {
                    Ok(transfer) => serde_json::json!({"transfer":transfer}),
                    Err(error) => serde_json::json!({"error":error}),
                },
                Command::Disconnect => {
                    let _ = service.disconnected::<()>();
                    serde_json::json!({"ok":true})
                }
                Command::Configure { .. } => unreachable!(),
            }
        };
        serde_json::to_writer(&mut stdout, &output).map_err(|e| e.to_string())?;
        writeln!(&mut stdout).map_err(|e| e.to_string())?;
        stdout.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
