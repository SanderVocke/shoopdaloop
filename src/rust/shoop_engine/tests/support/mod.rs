use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub struct TraceAttempt {
    client: tracy_client::Client,
}

impl TraceAttempt {
    pub fn active(&self) -> bool {
        let _ = &self.client;
        true
    }
}

fn field(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    assert!(bytes.len() <= 4096);
    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn take_field(data: &[u8], offset: &mut usize) -> String {
    let length = u16::from_be_bytes(data[*offset..*offset + 2].try_into().unwrap()) as usize;
    *offset += 2;
    let value = std::str::from_utf8(&data[*offset..*offset + length])
        .unwrap()
        .to_owned();
    *offset += length;
    value
}

fn request(kind: u16, operation: &[u8]) -> Vec<u8> {
    let endpoint = std::env::var("TRACY_COLLECTOR_ENDPOINT").unwrap();
    let token = std::env::var("TRACY_COLLECTOR_TOKEN").unwrap();
    let mut payload = Vec::new();
    field(&mut payload, &token);
    payload.extend_from_slice(operation);
    let mut frame = b"TCOL".to_vec();
    frame.extend_from_slice(&1_u16.to_be_bytes());
    frame.extend_from_slice(&kind.to_be_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    let mut stream = TcpStream::connect(endpoint).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(&frame).unwrap();
    let mut header = [0_u8; 12];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(&header[..4], b"TCOL");
    let length = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;
    let mut response = vec![0; length];
    stream.read_exact(&mut response).unwrap();
    let status = u16::from_be_bytes(response[..2].try_into().unwrap());
    let mut offset = 2;
    let message = take_field(&response, &mut offset);
    assert_eq!(status, 0, "collector error {status}: {message}");
    response[offset..].to_vec()
}

/// Activates only in the dedicated nextest trace lane. Discovery and ordinary
/// cargo tests never contact the collector or initialize Tracy.
pub fn startup() -> Option<TraceAttempt> {
    if std::env::var("SHOOP_TRACY_NEXTEST").as_deref() != Ok("1") {
        return None;
    }
    let attempt_id = std::env::var("NEXTEST_ATTEMPT_ID").ok()?;
    let run_id = std::env::var("TRACY_COLLECTOR_RUN_ID").unwrap();
    let binary_id = std::env::var("NEXTEST_BINARY_ID").unwrap();
    let test_name = std::env::var("NEXTEST_TEST_NAME").unwrap();
    let attempt = std::env::var("NEXTEST_ATTEMPT")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    let stress = format!(
        "{}/{}",
        std::env::var("NEXTEST_STRESS_CURRENT").unwrap_or_default(),
        std::env::var("NEXTEST_STRESS_TOTAL").unwrap_or_default()
    );
    let mut registration = Vec::new();
    field(&mut registration, &run_id);
    field(&mut registration, &attempt_id);
    field(&mut registration, &binary_id);
    field(&mut registration, &test_name);
    registration.extend_from_slice(&attempt.saturating_sub(1).to_be_bytes());
    field(&mut registration, &stress);
    let response = request(3, &registration);
    let mut offset = 0;
    let session_id = take_field(&response, &mut offset);
    let port = u16::from_be_bytes(response[offset..offset + 2].try_into().unwrap());

    std::env::set_var("TRACY_PORT", port.to_string());
    let client = tracy_client::Client::start();
    client.message("ShoopDaLoop collector startup", 0);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let mut status = Vec::new();
        field(&mut status, &session_id);
        let response = request(4, &status);
        let mut offset = 0;
        let state = take_field(&response, &mut offset);
        let handshake = take_field(&response, &mut offset);
        if state == "capturing" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "collector state={state}, handshake={handshake}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    shoop_tracing::set_tracing_enabled(true);
    shoop_tracing::set_engine_detail_enabled(
        std::env::var("SHOOP_TRACY_DETAIL").as_deref() == Ok("1"),
    );
    let marker = format!("shoop-nextest:{test_name}:attempt:{attempt}:id:{attempt_id}");
    for _ in 0..50 {
        client.message(&marker, 0);
        std::thread::sleep(Duration::from_millis(20));
    }
    Some(TraceAttempt { client })
}
