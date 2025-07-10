use std::process::Command;

pub fn native_shell_command(command: &str) -> Command {
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
