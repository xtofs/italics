use std::ffi::OsStr;
use std::process::{Command, Output};

pub fn run_cmd<P, I, S>(program: P, args: I) -> std::io::Result<Output>
where
    P: AsRef<OsStr>,
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(program);
    cmd.args(args);
    println!("running {:?}", cmd.as_shell_string());
    let output = cmd.output()?;

    if !output.stdout.is_empty() {
        println!("  stdout: {}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        println!("  stderr: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(output)
}

macro_rules! run_cmd {
    ($prog:expr $(, $arg:expr )* $(,)?) => {{
        let args: Vec<std::ffi::OsString> = vec![$(std::ffi::OsString::from($arg)),*];
        crate::cmd::run_cmd($prog, args)
    }};
}
trait CommandExt {
    fn as_shell_string(&self) -> String;
}

impl CommandExt for std::process::Command {
    fn as_shell_string(&self) -> String {
        let program = self.get_program().to_string_lossy();
        let args = self
            .get_args()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>();
        std::iter::once(program)
            .chain(args)
            .collect::<Vec<_>>()
            .join(" ")
    }
}
