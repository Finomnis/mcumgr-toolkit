# Zephyr MCUmgr Client

[![Crates.io](https://img.shields.io/crates/v/zephyr-mcumgr)](https://crates.io/crates/zephyr-mcumgr)
[![PyPI - Version](https://img.shields.io/pypi/v/zephyr_mcumgr)](https://pypi.org/project/zephyr-mcumgr/)
[![Crates.io](https://img.shields.io/crates/d/zephyr-mcumgr)](https://crates.io/crates/zephyr-mcumgr)
[![License](https://img.shields.io/crates/l/zephyr-mcumgr)](https://github.com/Finomnis/zephyr-mcumgr-client/blob/main/LICENSE-MIT)
[![Build Status](https://img.shields.io/github/actions/workflow/status/Finomnis/zephyr-mcumgr-client/ci.yml?branch=main)](https://github.com/Finomnis/zephyr-mcumgr-client/actions/workflows/ci.yml?query=branch%3Amain)
[![docs.rs](https://img.shields.io/docsrs/zephyr-mcumgr)](https://docs.rs/zephyr-mcumgr)
[![Coverage Status](https://img.shields.io/codecov/c/github/Finomnis/zephyr-mcumgr-client)](https://app.codecov.io/github/Finomnis/zephyr-mcumgr-client)

This crate provides a full Rust-based software suite for Zephyr's [MCUmgr protocol](https://docs.zephyrproject.org/latest/services/device_mgmt/mcumgr.html).

It might be compatible with other MCUmgr/SMP-based systems, but it is developed with Zephyr in mind.

Specifically, it provides:

- A [Rust library](https://crates.io/crates/zephyr-mcumgr) that supports all Zephyr MCUmgr commands
- A [CLI tool](https://crates.io/crates/zephyr-mcumgr-cli) that allows most of the commands to be run via command line
- A [Python interface](https://pypi.org/project/zephyr-mcumgr/) for the library

Its primary design goals are:
- Completeness
  - cover all use cases of Zephyr's MCUmgr
  - for implementation progress, see this [tracking issue](https://github.com/Finomnis/zephyr-mcumgr-client/issues/32)
- Performance
  - use static memory and large buffers to prioritize performance
    over memory footprint
  - see further down for more information regarding performance
    optimizations required on Zephyr side


## Usage Example

```rust no_run
use zephyr_mcumgr::MCUmgrClient;
use std::time::Duration;

fn main() {
    let serial = serialport::new("COM42", 115200).open().unwrap();

    let mut client = MCUmgrClient::new_from_serial(serial);
    client.set_timeout(Duration::from_millis(500));
    client.use_auto_frame_size().unwrap();

    println!("{:?}", client.os_echo("Hello world!").unwrap());
}
```

```none
"Hello world!"
```

## Usage as a library

To use this library in your project, enter your project directory and run:

```none
cargo add zephyr-mcumgr
```

## Installation as command line tool

```none
cargo install zephyr-mcumgr-cli
```

### Usage example

```none
$ zephyr-mcumgr --serial COM42 os echo "Hello world!"
Hello world!
```

Or as a [raw command](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#echo-command):

```none
$ zephyr-mcumgr --serial COM42 raw read 0 0 '{"d":"Hello World!"}'
{
  "r": "Hello World!"
}
```

## Performance

Zephyr's default buffer sizes are quite small and reduce the read/write performance drastically.

The central most important setting is [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40). Its default of 384 bytes is very limiting, both for performance and as cutoff for large responses, like `os task_statistics` or some shell commands.

Be aware that changing this value also requires an increase of `MCUMGR_TRANSPORT_WORKQUEUE_STACK_SIZE` to prevent overflow crashes.

In practice, I found that the following values work quite well (on i.MX RT1060)
and give me 410 KB/s read and 120 KB/s write throughput, which is an order of magnitude faster than the default settings.

```kconfig
CONFIG_MCUMGR_TRANSPORT_NETBUF_SIZE=4096
CONFIG_MCUMGR_TRANSPORT_WORKQUEUE_STACK_SIZE=8192
```

If the experience differs on other chips, please open an issue and let me know.

## Contributions

Contributions are welcome!

I primarily wrote this crate for myself, so any ideas for improvements are greatly appreciated.
