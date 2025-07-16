pub enum CrashHandlingMessageType {
    // Recursively set values in the metadata JSON.
    SetJson = 0,
    // After a crash dump request, this is used to post additional
    // information about the crash
    AdditionalCrashAttachment = 1,
}

pub struct CrashHandlingMessage {
    pub message_type: CrashHandlingMessageType,
    pub message: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AdditionalCrashAttachment {
    pub id: String,
    pub contents: String,
}
