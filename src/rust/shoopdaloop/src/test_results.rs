use serde;
use serde::Serialize;

#[derive(Serialize)]
pub enum ResultStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Serialize)]
pub struct TestFnResult {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@classname")]
    class_name: String,
    #[serde(flatten)]
    status: ResultStatus,
}

#[derive(Serialize)]
pub struct TestCaseResults {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "testcase")]
    test_fn_results: Vec<TestFnResult>,
}

#[derive(Serialize)]
#[serde(rename = "testsuites")]
pub struct TestResults {
    #[serde(rename = "testsuite")]
    test_case_results: Vec<TestCaseResults>,
}

impl TestResults {
    pub fn serialize_xml<W>(&self) -> Result<String, anyhow::Error> {
        serde_xml_rs::to_string(self).map_err(|e| e.into())
    }
}
