use indicatif::MultiProgress;

use crate::{args::CommonArgs, client::Client, errors::CliError};

pub fn run(
    client: &Client,
    _multiprogress: &MultiProgress,
    _args: CommonArgs,
    argv: Vec<String>,
) -> Result<(), CliError> {
    let client = client.get()?;
    let (returncode, output) = client.shell_execute(&argv)?;
    println!("{output}");
    if returncode < 0 {
        return Err(CliError::ShellExitCode(returncode));
    } else if returncode > 0 {
        println!();
        println!("Exit code: {returncode}")
    }
    Ok(())
}
