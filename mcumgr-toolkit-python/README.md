# MCUmgr Client for Zephyr

[![Crates.io](https://img.shields.io/crates/v/mcumgr-toolkit)](https://crates.io/crates/mcumgr-toolkit)
[![PyPI - Version](https://img.shields.io/pypi/v/mcumgr_toolkit)](https://pypi.org/project/mcumgr-toolkit)
[![PyPI - Downloads](https://img.shields.io/pypi/dw/mcumgr-toolkit)](https://pypi.org/project/mcumgr-toolkit)
[![License](https://img.shields.io/crates/l/mcumgr-toolkit)](https://github.com/Finomnis/mcumgr-toolkit/blob/main/LICENSE-MIT)
[![Build Status](https://img.shields.io/github/actions/workflow/status/Finomnis/mcumgr-toolkit/ci.yml?branch=main)](https://github.com/Finomnis/mcumgr-toolkit/actions/workflows/ci.yml?query=branch%3Amain)
[![Docs Status](https://img.shields.io/github/actions/workflow/status/Finomnis/mcumgr-toolkit/python-docs.yml?branch=main&label=docs)](https://finomnis.github.io/mcumgr-toolkit)

This library provides a Rust-based Python API for Zephyr's [MCUmgr protocol](https://docs.zephyrproject.org/latest/services/device_mgmt/mcumgr.html).

It might be compatible with other MCUmgr/SMP-based systems, but it is developed with Zephyr in mind.

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

Connect to a serial port:

```python no_run
from mcumgr_toolkit import MCUmgrClient

with MCUmgrClient.serial("/dev/ttyACM0") as client:
    client.use_auto_frame_size()

    print(client.os_echo("Hello world!"))
```

```none
Hello world!
```

Or a USB-based serial port:

```python no_run
from mcumgr_toolkit import MCUmgrClient

with MCUmgrClient.usb_serial("2fe3:0004") as client:
    client.use_auto_frame_size()

    print(client.os_echo("Hello world!"))
```

```none
Hello world!
```

For more information, take a look at the [API reference](https://finomnis.github.io/mcumgr-toolkit).

## Performance

Zephyr's default buffer sizes are quite small and reduce the read/write performance drastically.

The central most important setting is [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40). Its default of 384 bytes is very limiting, both for performance and as cutoff for large responses, like `os_task_statistics()` or some shell commands.

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
