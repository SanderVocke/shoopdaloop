use crate::{bits::Bits, sim::Sim};

pub struct Config {
    pub prefix: String,
    pub selector: String,
}
impl Config {
    pub fn new(prefix: &str, selector: &str) -> Self {
        Self {
            prefix: prefix.into(),
            selector: selector.into(),
        }
    }
    pub fn access(
        &self,
        sim: &mut Sim<'_>,
        selector: u64,
        address: u64,
        value: Option<&Bits>,
        error: bool,
    ) -> Bits {
        let name = |suffix: &str| format!("{}_{}", self.prefix, suffix);
        let valid = name("valid");
        let ready = name("ready");
        let response = name("response_valid");
        let response_error = name("response_error");
        let read = name("read_data");
        sim.model.set_all(&[
            (&valid, 1),
            (&self.selector, selector),
            (&name("address"), address),
            (&name("write"), u64::from(value.is_some())),
        ]);
        sim.model
            .set_bits(&name("write_data"), value.unwrap_or(&Bits::default()));
        let mut accepted = false;
        for _ in 0..100_000 {
            let (before, _) = sim.tick(&[&ready]);
            if before.get(&ready) != 0 {
                accepted = true;
                break;
            }
        }
        assert!(accepted, "{} configuration admission timeout", self.prefix);
        sim.model.set(&valid, 0);
        sim.until(
            100_000,
            &format!("{} configuration response", self.prefix),
            |m| m.get(&response) != 0,
        );
        assert_eq!(
            sim.model.get(&response_error) != 0,
            error,
            "{} configuration response at {address}",
            self.prefix
        );
        let result = sim.model.bits(&read);
        if value.is_some() || error {
            assert!(
                result.0.iter().all(|&n| n == 0),
                "write/error response leaked data"
            );
        }
        sim.idle(1);
        assert_eq!(
            sim.model.get(&response),
            0,
            "configuration response failed to pulse"
        );
        result
    }
}
