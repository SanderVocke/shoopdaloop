//! The existing production compiler and numeric oracles, shared by all full-pipeline benches.
use crate::Result;
use rt_backend_testbench::{audit, production, rtl, spec::Scenario};
use serde_json::Value;

pub struct Reference {
    pub image: Value,
    pub fixed: Value,
    pub floating: Value,
    pub audit: Value,
}
impl Reference {
    pub fn new(scenario: &Value) -> Result<Self> {
        let typed: Scenario = serde_json::from_value(scenario.clone())?;
        typed.validate()?;
        let image = serde_json::to_value(rtl::compile(&typed)?)?;
        let fixed = serde_json::to_value(production::run::<rt_runtime::Q824>(&typed)?)?;
        let floating = serde_json::to_value(production::run::<f32>(&typed)?)?;
        let audit = serde_json::to_value(audit::run(&typed)?)?;
        assert_eq!(
            audit["result"], fixed,
            "arithmetic audit changed production behavior"
        );
        Ok(Self {
            image,
            fixed,
            floating,
            audit,
        })
    }
}
