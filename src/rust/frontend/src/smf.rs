use backend_bindings::MidiEvent;
use json;

// TODO: very confusing name. This was meant as "simple MIDI format", a MIDI
// file format that just uses JSON to save timestamped midi messages for
// ShoopDaLoop. not to be confused with "standard MIDI format".
pub type Smf = String;

pub fn to_smf<'a>(
    events: impl Iterator<Item = &'a MidiEvent>,
    total_length: usize,
    samplerate: usize,
) -> Smf {
    let (recorded_msgs, state_msgs): (Vec<&'a MidiEvent>, Vec<&'a MidiEvent>) =
        events.partition(|e| e.time >= 0);
    let recorded_msgs = recorded_msgs
        .iter()
        .map(|e| {
            json::object! {
                time: e.time as f64 / samplerate as f64,
                data: e.data.clone()
            }
        })
        .collect::<Vec<_>>();
    let state_msgs = state_msgs
        .iter()
        .map(|e| json::JsonValue::from(e.data.clone()))
        .collect::<Vec<_>>();
    let length_s = total_length as f64 / samplerate as f64;
    let json_value = json::object! {
        messages: recorded_msgs,
        start_state: state_msgs,
        length: length_s,
    };
    return json::stringify_pretty(json_value, 2);
}

pub struct ParseSmfResult {
    pub n_frames: usize,
    pub state_msgs: Vec<Vec<u8>>,
    pub recorded_msgs: Vec<MidiEvent>,
    pub sample_rate: usize,
}

pub fn parse_smf(smf: &str, target_sample_rate: usize) -> Result<ParseSmfResult, anyhow::Error> {
    let mut conversion_error: Option<anyhow::Error> = None;

    let mut event_from_json = |event: &json::JsonValue| -> MidiEvent {
        match || -> Result<MidiEvent, anyhow::Error> {
            match event {
                json::JsonValue::Object(object) => {
                    let time = object
                        .get("time")
                        .ok_or(anyhow::anyhow!("No time field"))?
                        .as_f64()
                        .ok_or(anyhow::anyhow!("time is not an f64"))?;
                    let data: Vec<u8> = object
                        .get("data")
                        .ok_or(anyhow::anyhow!("No data field"))?
                        .members()
                        .map(|byte| match byte.as_u8() {
                            Some(byte) => byte,
                            None => {
                                conversion_error = Some(anyhow::anyhow!("invalid byte"));
                                0
                            }
                        })
                        .collect();
                    Ok(MidiEvent {
                        time: (time * (target_sample_rate as f64)).round() as i32,
                        data: data,
                    })
                }
                _ => Err(anyhow::anyhow!("event is not a JSON object")),
            }
        }() {
            Ok(m) => m,
            Err(e) => {
                conversion_error = Some(e);
                MidiEvent {
                    time: 0,
                    data: Vec::default(),
                }
            }
        }
    };

    let json_value = json::parse(smf)?;
    if let json::JsonValue::Object(object) = json_value {
        let recorded_msgs = object
            .get("messages")
            .ok_or(anyhow::anyhow!("missing 'messages' field"))?
            .members()
            .map(|v| event_from_json(v))
            .collect::<Vec<_>>();
        let state_msgs = object
            .get("start_state")
            .ok_or(anyhow::anyhow!("missing 'start_state' field"))?
            .members()
            .map(|v| {
                v.members()
                    .map(|byte| match byte.as_u8() {
                        Some(byte) => byte,
                        None => {
                            conversion_error = Some(anyhow::anyhow!("invalid byte"));
                            0
                        }
                    })
                    .collect()
            })
            .collect::<Vec<_>>();
        let length_s = object
            .get("length")
            .ok_or(anyhow::anyhow!("missing 'length' field"))?
            .as_f64()
            .ok_or(anyhow::anyhow!("'length' is not a float"))?;

        if let Some(e) = conversion_error {
            return Err(anyhow::anyhow!(
                "Failed to convert MIDI message(s). Last error: {e}"
            ));
        }

        return Ok(ParseSmfResult {
            n_frames: (length_s * (target_sample_rate as f64)).round() as usize,
            state_msgs: state_msgs,
            recorded_msgs: recorded_msgs,
            sample_rate: target_sample_rate,
        });
    } else {
        return Err(anyhow::anyhow!("Not a top-level JSON object"));
    }
}
