# Zephyr MCUmgr Client

[![Crates.io](https://img.shields.io/crates/v/zephyr-mcumgr)](https://crates.io/crates/zephyr-mcumgr)
[![Crates.io](https://img.shields.io/crates/d/zephyr-mcumgr)](https://crates.io/crates/zephyr-mcumgr)
[![License](https://img.shields.io/crates/l/zephyr-mcumgr)](https://github.com/Finomnis/zephyr-mcumgr-client/blob/main/LICENSE-MIT)
[![Build Status](https://img.shields.io/github/actions/workflow/status/Finomnis/zephyr-mcumgr-client/ci.yml?branch=main)](https://github.com/Finomnis/zephyr-mcumgr-client/actions/workflows/ci.yml?query=branch%3Amain)
[![docs.rs](https://img.shields.io/docsrs/zephyr-mcumgr)](https://docs.rs/zephyr-mcumgr)

This crate provides a full Rust based software suite for Zephyr's [MCUmgr protocol](https://docs.zephyrproject.org/latest/services/device_mgmt/mcumgr.html).

It might be compatible with other MCUmgr/SMP based systems, but it is developed with Zephyr in mind.

Specifically, it provides:

- A Rust library that supports all Zephyr MCUmgr commands
- A CLI tool that allows most of the commands to be run via command line
- A Python interface for the library


## Usage Example

```rust no_run
use zephyr_mcumgr::MCUmgrClient;

fn main() {
    let serial = serialport::new("COM42", 115200)
        .timeout(std::time::Duration::from_millis(500))
        .open()
        .unwrap();

    let mut client = MCUmgrClient::from_serial(serial);
    client.use_auto_frame_size().unwrap();

    println!("{:?}", client.os_echo("Hello world!").unwrap());
}
```
```none
"Hello world!"
```

## Using as a library

To use this library in your project, enter your project directory and run:
```bash
cargo add zephyr-mcumgr
```

## Installing as command line tool

```bash
cargo install zephyr-mcumgr-cli
```

## Contributions

Contributions are welcome!

I primarily wrote this crate for myself, so any ideas for improvements are greatly appreciated.
