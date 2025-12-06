use zephyr_mcumgr::MCUmgrClient;

use crate::{args::CommonArgs, errors::CliError};

pub fn run(client: &MCUmgrClient, _args: CommonArgs, argv: Vec<String>) -> Result<(), CliError> {
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
