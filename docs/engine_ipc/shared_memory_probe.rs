use iceoryx2::prelude::*;
use std::io::{BufRead, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let node = NodeBuilder::new().create::<ipc::Service>()?;
    let name: ServiceName = args[1].as_str().try_into()?;
    let service = node.service_builder(&name).publish_subscribe::<[u8;1280]>()
        .enable_safe_overflow(false).subscriber_max_buffer_size(2)
        .max_publishers(1).max_subscribers(1).max_nodes(2).open_or_create()?;
    if args.len() > 2 {
        let subscriber = service.subscriber_builder().create()?;
        println!("ready"); std::io::stdout().flush()?;
        let mut input = String::new(); std::io::stdin().lock().read_line(&mut input)?;
        for i in 0..2 {
            let sample = subscriber.receive()?.ok_or("missing record")?;
            assert_eq!(sample.payload()[0], i);
        }
        assert!(subscriber.receive()?.is_none());
        println!("received"); std::io::stdout().flush()?;
    } else {
        let publisher = service.publisher_builder().backpressure_strategy(BackpressureStrategy::DiscardData).create()?;
        let mut child = std::process::Command::new(std::env::current_exe()?).args([&args[1],"child"])
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn()?;
        let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new(); output.read_line(&mut line)?; assert_eq!(line.trim(),"ready");
        for i in 0..3 { let count = publisher.send_copy([i;1280])?; assert_eq!(count, if i<2 {1} else {0}); }
        writeln!(child.stdin.as_mut().unwrap(),"read")?;
        line.clear(); output.read_line(&mut line)?; assert_eq!(line.trim(),"received");
        assert!(child.wait()?.success()); println!("two-process fixed records and non-overwriting capacity verified");
    }
    Ok(())
}
