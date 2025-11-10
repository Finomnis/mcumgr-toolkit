# Zephyr MCUmgr Client

[![Crates.io](https://img.shields.io/crates/v/zephyr-mcumgr)](https://crates.io/crates/zephyr-mcumgr)
[![PyPI - Version](https://img.shields.io/pypi/v/zephyr_mcumgr)](https://pypi.org/project/zephyr-mcumgr/)
[![Crates.io](https://img.shields.io/crates/d/zephyr-mcumgr)](https://crates.io/crates/zephyr-mcumgr)
[![License](https://img.shields.io/crates/l/zephyr-mcumgr)](https://github.com/Finomnis/zephyr-mcumgr-client/blob/main/LICENSE-MIT)
[![Build Status](https://img.shields.io/github/actions/workflow/status/Finomnis/zephyr-mcumgr-client/ci.yml?branch=main)](https://github.com/Finomnis/zephyr-mcumgr-client/actions/workflows/ci.yml?query=branch%3Amain)
[![docs.rs](https://img.shields.io/docsrs/zephyr-mcumgr)](https://docs.rs/zephyr-mcumgr)

This library provides a Rust-based Python API for Zephyr's [MCUmgr protocol](https://docs.zephyrproject.org/latest/services/device_mgmt/mcumgr.html).

It might be compatible with other MCUmgr/SMP-based systems, but it is developed with Zephyr in mind.

Its primary design goals are:
- Completeness
  - cover all use cases of Zephyr's MCUmgr
- Performance
  - use static memory and large buffers to prioritize performance
    over memory footprint
  - see further down for more information regarding performance
    optimizations required on Zephyr side


## Usage Example

```python no_run
from zephyr_mcumgr import MCUmgrClient

client = MCUmgrClient.new_from_serial("COM42")
client.set_timeout_ms(500)
client.use_auto_frame_size()

print(client.os_echo("Hello world!"))
```

```none
Hello world!
```

The API of `MCUmgrClient` closely follows the one of its [Rust counterpart](https://docs.rs/zephyr-mcumgr/0.0.1/zephyr_mcumgr/client/struct.MCUmgrClient.html).

## Performance

Zephyr's default buffer sizes are quite small and reduce the read/write performance drastically.

The central most important setting is [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40). Its default of 384 bytes is very limiting, both for performance and as limiting factor of large responses, like `os_task_statistics()` or some shell commands.

Be aware that changing this value also requires an increase of `MCUMGR_TRANSPORT_WORKQUEUE_STACK_SIZE`, otherwise I got overflow crashes.

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
