use workbench_integration::run_command_capture_stdout;

fn main() {
    let out = if cfg!(windows) {
        run_command_capture_stdout("cmd", &["/C", "echo", "hello from workbench-integration"])
    } else {
        run_command_capture_stdout("echo", &["hello from workbench-integration"])
    };

    match out {
        Ok(stdout) => print!("{stdout}"),
        Err(e) => eprintln!("command failed: {e}"),
    }
}
