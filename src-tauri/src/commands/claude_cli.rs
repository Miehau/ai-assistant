use std::process::Command;

#[tauri::command(rename_all = "snake_case")]
pub fn is_claude_cli_installed() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
