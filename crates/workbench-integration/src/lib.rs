use std::process::Command;



const WORKBENCH_INSTALL_PATH = "C:\\Users\\Arnav\\Projects\\gpui_test\\crates\\workbench-integration\\islandora_workbench";


pub fn run_command_capture_stdout(program: &str, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new(program).args(args).output()?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn 
