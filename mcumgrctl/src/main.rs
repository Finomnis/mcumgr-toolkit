pub fn main() -> miette::Result<()> {
    mcumgrctl::cli_main(mcumgrctl::no_custom_transports)
}
