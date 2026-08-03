use std::process::Command;

pub fn native_shell_command(command: &str) -> Command {
    let _span = tracing::debug_span!(
        "app.common.native_shell_command",
        command_length = command.len()
    )
    .entered();
    let mut result: Command;
    if cfg!(target_os = "windows") {
        result = Command::new("cmd");
        result.args(["/C", command]);
    } else {
        result = Command::new("sh");
        result.args(["-c", command]);
    }
    result
}
