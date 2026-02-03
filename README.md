# MCUmgr Client for Zephyr

[![Crates.io](https://img.shields.io/crates/v/mcumgr-toolkit)](https://crates.io/crates/mcumgr-toolkit)
[![PyPI - Version](https://img.shields.io/pypi/v/mcumgr-toolkit)](https://pypi.org/project/mcumgr-toolkit/)
[![Crates.io](https://img.shields.io/crates/d/mcumgr-toolkit)](https://crates.io/crates/mcumgr-toolkit)
[![License](https://img.shields.io/crates/l/mcumgr-toolkit)](https://github.com/Finomnis/mcumgr-toolkit/blob/main/LICENSE-MIT)
[![Build Status](https://img.shields.io/github/actions/workflow/status/Finomnis/mcumgr-toolkit/ci.yml?branch=main)](https://github.com/Finomnis/mcumgr-toolkit/actions/workflows/ci.yml?query=branch%3Amain)
[![docs.rs](https://img.shields.io/docsrs/mcumgr-toolkit)](https://docs.rs/mcumgr-toolkit)
[![Coverage Status](https://img.shields.io/codecov/c/github/Finomnis/mcumgr-toolkit)](https://app.codecov.io/github/Finomnis/mcumgr-toolkit)

This crate provides a full Rust-based software suite for Zephyr's [MCUmgr protocol](https://docs.zephyrproject.org/latest/services/device_mgmt/mcumgr.html).

It might be compatible with other MCUmgr/SMP-based systems, but it is developed with Zephyr in mind.

Specifically, it provides:

- A [Rust library](https://crates.io/crates/mcumgr-toolkit) that supports all Zephyr MCUmgr commands
- A [CLI tool](https://crates.io/crates/mcumgr-toolkit-cli) that allows most of the commands to be run via command line
- A [Python interface](https://pypi.org/project/mcumgr-toolkit/) for the library

Its primary design goals are:
- Completeness
  - cover all use cases of Zephyr's MCUmgr
  - for implementation progress, see this [tracking issue](https://github.com/Finomnis/mcumgr-toolkit/issues/32)
- Performance
  - use static memory and large buffers to prioritize performance
    over memory footprint
  - see further down for more information regarding performance
    optimizations required on Zephyr side


## Usage Example

```rust no_run
use mcumgr_toolkit::MCUmgrClient;
use std::time::Duration;

fn main() {
    let serial = serialport::new("COM42", 115200).open().unwrap();

    let mut client = MCUmgrClient::new_from_serial(serial);
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
cargo add mcumgr-toolkit
```

## Installation as command line tool

```none
cargo install mcumgr-toolkit-cli
```

### Usage examples

Send an echo over a serial connection:

```none
$ mcumgrctl --serial COM42 os echo "Hello world!"
Hello world!
```

Perform a firmware update:

```none
$ mcumgrctl --serial COM42 firmware update zephyr.signed.encrypted.bin
Detecting bootloader ...
Found bootloader: MCUboot
Parsing firmware image ...
Querying device state ...
Update: 1.2.3.4-f0a745b8 -> 1.2.3.5-79f50793
Uploading new firmware ...
Activating new firmware ...
Triggering device reboot ...
Success.
Device should reboot with new firmware.
```

Omit the command to run a simple connection test:

```none
$ mcumgrctl --serial COM42
Device alive and responsive.
```

Connect to a USB serial port using USB VID/PID:

```none
$ mcumgrctl --usb-serial 2fe3:0004
Device alive and responsive.
```

Or without an identifier to list all available ports:

```none
$ mcumgrctl --usb-serial

Available USB serial ports:

 - 2fe3:0004:0 (/dev/ttyACM0) - Zephyr Project CDC ACM serial backend
```

You can even use a Regex if you want:

```none
$ mcumgrctl --usb-serial "2fe3:.*"
Device alive and responsive.
```

> [!TIP]
> `2fe3:0004` is the default VID/PID of Zephyr samples.

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
