//! A single outstanding request/response port, reusable across metadata services.
use crate::{bits::Bits, sim::Sim};
#[derive(Clone)]
pub struct Request {
    prefix: String,
    pub limit: u64,
}
impl Request {
    pub fn new(s: &mut Sim<'_>, prefix: &str) -> Self {
        let name = |n| format!("{prefix}{n}");
        s.agent(crate::protocol::ReadyValid::new(
            &name("response_valid"),
            &name("response_ready"),
            &[&name("response_data"), &name("response_error")],
        ));
        Self {
            prefix: prefix.into(),
            limit: 100_000,
        }
    }
    pub fn get_prefix(&self) -> &str {
        &self.prefix
    }
    pub fn set(&self, s: &mut Sim<'_>, values: &[(&str, u64)]) {
        for &(n, v) in values {
            s.model.set(&format!("{}{n}", self.prefix), v);
        }
    }
    pub fn get(&self, s: &Sim<'_>, name: &str) -> u64 {
        s.model.get(&format!("{}{name}", self.prefix))
    }
    pub fn transaction(
        &self,
        s: &mut Sim<'_>,
        fields: &[(&str, u64)],
        error: u64,
        hold: u64,
    ) -> Bits {
        self.set(s, fields);
        self.set(s, &[("request_valid", 1), ("response_ready", 0)]);
        let ready = format!("{}request_ready", self.prefix);
        let mut admitted = false;
        for _ in 0..self.limit {
            if s.tick(&[&ready]).0.get(&ready) != 0 {
                admitted = true;
                break;
            }
        }
        assert!(admitted, "{} request admission timeout", self.prefix);
        self.set(s, &[("request_valid", 0)]);
        let valid = format!("{}response_valid", self.prefix);
        s.until(self.limit, "service response", |m| m.get(&valid) != 0);
        let value = s.model.bits(&format!("{}response_data", self.prefix));
        assert_eq!(
            self.get(s, "response_error"),
            error,
            "{} response error, fields {fields:?}",
            self.prefix
        );
        s.idle(hold);
        self.set(s, &[("response_ready", 1)]);
        s.idle(1);
        self.set(s, &[("response_ready", 0)]);
        value
    }
}
