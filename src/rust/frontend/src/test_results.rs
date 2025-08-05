use serde;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub enum ResultStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Serialize, Debug)]
pub struct TestFnResult {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@classname")]
    pub class_name: String,
    #[serde(flatten)]
    pub status: ResultStatus,
}

#[derive(Serialize, Debug, Default)]
pub struct TestCaseResults {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "testcase")]
    pub test_fn_results: Vec<TestFnResult>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename = "testsuites")]
pub struct TestResults {
    #[serde(rename = "testsuite")]
    pub test_case_results: Vec<TestCaseResults>,
}

impl TestResults {
    pub fn serialize_xml(&self) -> Result<String, anyhow::Error> {
        quick_xml::se::to_string(self).map_err(|e| e.into())
    }
}
