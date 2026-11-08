//! The "shell" module's helper: run a command and capture its output.


/// Runs a command through the operating system's shell and returns its standard
/// output. A command that fails (non-zero exit) is a catchable run-time error
/// carrying its error output.
pub(super) fn shell_run(command: &str) -> Result<String, String> {
    let mut c = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = std::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    let out = c.output().map_err(|e| format!("shell.run: cannot start the command ({})", e))?;
    if !out.status.success() {
        let code = out.status.code().map_or_else(|| "stopped by a signal".to_string(), |n| format!("exit code {}", n));
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("shell.run: the command failed ({}): {}", code, err.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
