# Zephyr simulated integration tests

Run from the repository root:

```sh
cargo test -p mcumgr-toolkit --locked --test zephyr_integration
```

The suite contains 83 tests, with additional cases in table-driven loops. It
calls all 44 transport-independent public `MCUmgrClient` methods, primarily
through `MCUmgrClient::new_from_transport`. Tests use the real client,
connection, command serializers, response deserializers, and default
`Transport::send_frame` / `receive_frame` implementations. There are no added
dependencies, production changes, network requests, sleeps, hardware, or
external services required at test runtime.

## Reference revisions and independence

The crate API was taken from `Finomnis/mcumgr-toolkit` **main**, commit
`40713d88731362b903f085f7a8be64bd23ce4205` (version 0.17.1).

The protocol oracle is **Zephyr upstream main**, commit
[`6f1ab6c070f29bb7657a43cd1e59767bfe497ab1`](https://github.com/zephyrproject-rtos/zephyr/tree/6f1ab6c070f29bb7657a43cd1e59767bfe497ab1).
It is not the Zephyr 4.3 release. Pinning the fetched upstream commit keeps
tests reproducible as upstream changes.

`device.rs`, `wire.rs`, and `firmware.rs` do not import `mcumgr_toolkit`.
They define their own wire fields, group/command IDs, response construction,
device errors, and state transitions from the Zephyr C sources. The crate
source was consulted for public signatures, types, documented API contracts,
and the transport interface, not for expected protocol responses. Existing
crate test helpers are not reused. Only the adapter in `mod.rs` imports the
client and transport trait/error types.

`ciborium::Value` is used as a generic CBOR reader and scalar encoder; the
simulator never serializes or deserializes a crate command/response type.
Response containers are independently encoded as indefinite maps/lists, like
a non-canonical zcbor build. Literal CBOR responses, IEEE CRC32's `123456789`
vector, and SHA256's `abc` vector provide additional independent anchors.
Generic third-party CBOR, datetime, and SHA256 primitives are shared
dependencies; this suite does not claim to independently verify those
libraries' implementations.

### Source map

Paths below are relative to the pinned Zephyr repository. `references.json`
records immutable URLs and SHA256 digests of the source files consulted.

| Behavior | Zephyr source and relevant symbols |
| --- | --- |
| Header, opcodes, IDs, generic errors | `include/zephyr/mgmt/mcumgr/mgmt/mgmt_defines.h`, `include/zephyr/mgmt/mcumgr/smp/smp.h`, `subsys/mgmt/mcumgr/smp/src/smp.c`: `smp_make_rsp_hdr`, `smp_build_err_rsp`, `smp_add_cmd_err` |
| OS, RTC, stack units, mpools, bootloader queries | `subsys/mgmt/mcumgr/grp/os_mgmt/src/os_mgmt.c`: `os_mgmt_group_handlers`, `os_mgmt_taskstat_encode_stack_info`, `os_mgmt_mpstat_read`, `os_mgmt_datetime_*`, `os_mgmt_info`, `os_mgmt_bootloader_info` |
| Image schemas, erase, slot info, SHA verification | `subsys/mgmt/mcumgr/grp/img_mgmt/src/img_mgmt.c`: `img_mgmt_read_info`, `img_mgmt_erase`, `img_mgmt_slot_info`, `img_mgmt_upload` |
| Image state and activation | `subsys/mgmt/mcumgr/grp/img_mgmt/src/img_mgmt_state.c`: `img_mgmt_state_encode_slot`, `img_mgmt_state_read`, `img_mgmt_state_write` |
| Upload validation and resume offsets | `subsys/mgmt/mcumgr/grp/img_mgmt/src/zephyr_img_mgmt.c`: `img_mgmt_upload_inspect` |
| Files, checksum ranges, optional zero offset | `subsys/mgmt/mcumgr/grp/fs_mgmt/src/fs_mgmt.c`: `fs_mgmt_file_download`, `fs_mgmt_file_upload`, `fs_mgmt_file_hash_checksum`, `fs_mgmt_close_opened_file` |
| Checksum algorithms and output types | `subsys/mgmt/mcumgr/grp/fs_mgmt/src/fs_mgmt_hash_checksum_crc32.c`, `fs_mgmt_hash_checksum_sha256.c` |
| Runtime settings and persistence | `subsys/mgmt/mcumgr/grp/settings_mgmt/src/settings_mgmt.c`: `settings_mgmt_read`, `settings_mgmt_write`, `settings_mgmt_delete`, `settings_mgmt_commit`, `settings_mgmt_load`, `settings_mgmt_save` |
| Statistics | `subsys/mgmt/mcumgr/grp/stat_mgmt/src/stat_mgmt.c`: `stat_mgmt_show`, `stat_mgmt_list` |
| Shell return code separate from SMP errors | `subsys/mgmt/mcumgr/grp/shell_mgmt/src/shell_mgmt.c`: `shell_mgmt_exec`; adjacent `Kconfig` |
| Enumeration, omitted end flag, empty filters | `subsys/mgmt/mcumgr/grp/enum_mgmt/src/enum_mgmt.c`: `enum_mgmt_single`, `enum_mgmt_details`, `enum_mgmt_cb_details` |
| Storage partition erase | `subsys/mgmt/mcumgr/grp/zephyr_basic/src/basic_mgmt.c`: `storage_erase_handler` |
| Group-specific error numbers and command IDs | Each group's header under `include/zephyr/mgmt/mcumgr/grp/` |

MCUboot fixtures use `boot/bootutil/include/bootutil/image.h` at
[`aa32eaaad41cc345e6f6e6633368a4766fe23c25`](https://github.com/mcu-tools/mcuboot/blob/aa32eaaad41cc345e6f6e6633368a4766fe23c25/boot/bootutil/include/bootutil/image.h),
the MCUboot revision selected by this Zephyr commit's `west.yml`. Fixtures
contain a valid 32-byte image header and SHA256 TLV. They deliberately
distinguish the image-identity hash from the SHA256 of the entire upload.
They are unsigned parser/transport fixtures, not bootable production images.

## Architecture and modeled configuration

- `mod.rs`: in-memory raw-frame transport, isolated shared inspection handle,
  transcript, ordered fault schedule, timeout forwarding, and frame limits.
- `wire.rs`: independent SMP header parsing/construction, generic CBOR values,
  and Zephyr error envelopes.
- `device.rs`: stateful command dispatch and application fixture state.
- `firmware.rs`: independently assembled MCUboot fixtures and checksum helpers.
- `../zephyr_cases/`: assertions against the public client API.

Every test starts with a fresh device. `Handle::inspect` and `Handle::edit`
let tests examine side effects or seed a device before making client calls.
`Handle::requests` exposes raw headers, CBOR bytes and decoded payloads.
Faults can target a particular `(group, command)` after a chosen number of
matching requests. Request loss and response loss differ: losing a response
still executes the device operation. This matters for upload resumption and
duplicate shell side effects. A 10,000-request budget turns accidental client
transfer loops into failures instead of hanging CI.

The default device has all eight exposed management groups enabled, an RTC
with millisecond support, signed task priorities, stack/runtime statistics,
memory-pool statistics, modern shell `ret` responses, and original-style
generic SMP `rc` errors alongside SMP v2 group errors. It models two image
slots with swap-using-scratch behavior, 64 KiB slots with a 512-byte footer,
SHA256 upload verification, and a 37-byte filesystem download chunk. Image
state can omit false fields, and a single-image device omits `image`.
Profiles exercise absent task/detail fields, multi-image devices,
`no-downgrade`, optional hash verification, and legacy success `rc=0`.

Application-specific fixtures are intentionally explicit: a byte-valued
settings handler, an in-memory filesystem, named statistics, simple shell
commands, a flash storage partition, and deterministic OS information. Their
values are fixture choices; their wire schemas and command semantics come
from Zephyr. Reset executes a synchronous modeled boot transition so image
activation, confirmation and reversion can be asserted deterministically.

This is a behavioral simulator, not an execution of Zephyr C or a complete
RTOS/flash emulator. It covers the command paths used by these public API
tests. It does not emulate every Kconfig combination, filesystem failure,
flash alignment rule, signature verification, physical transport, or
MCUboot mode. Rare device failures are injected as protocol responses.
Hardware constructors (`new_from_serial`, `new_from_usb_serial`,
`new_from_udp`, `new_from_ble`, `new_from_ble_with_scan_callback`), discovery,
UART framing/CRC, UDP networking, and BLE GATT behavior are outside this
suite's boundary. Existing backend tests remain separate. Standalone utility
APIs are exercised only where reached through `MCUmgrClient`; this is not a
claim of every branch in every exported crate module being covered.

## API coverage

| Client methods | Test file / principal cases |
| --- | --- |
| `new_from_transport` | All fixtures; direct construction and concurrent use in `protocol.rs` |
| `set_frame_size`, `use_auto_frame_size` | `files.rs`: device/transport cap, header + CBOR overhead, too-small frames |
| `set_timeout` | `os.rs`: forwarded duration and configuration failure |
| `set_retries` | `protocol.rs`: zero/one/three retries, transient and permanent errors |
| `check_connection` | `os.rs`: real echo and incorrect echo rejection |
| `os_echo` | `os.rs`, `protocol.rs`: empty/unicode/NUL text, framing, CBOR, concurrency |
| `os_task_statistics`, `os_memory_pool_statistics` | `os.rs`: exact fields, optional statistics, stack-word conversion |
| `os_set_datetime`, `os_get_datetime` | `os.rs`: leap day, milliseconds, no fraction, unset RTC |
| `os_system_reset` | `os.rs`, `images.rs`: force, boot modes, invalid mode, state transitions |
| `os_mcumgr_parameters`, `os_application_info` | `os.rs`: fields, omitted/empty/combined/all/invalid formats |
| `os_bootloader_info` | `os.rs`: MCUboot mode, omitted downgrade flag, unknown bootloader |
| `image_get_state`, `image_set_state` | `images.rs`: compact/full state, identity hash, test/permanent boot, confirm/revert, invalid hashes |
| `image_upload` | `images.rs`: bytes, optional checksum/image, first-chunk metadata, limits, resume, lost replies, progress/cancel, upgrade-only, corruption |
| `image_erase`, `image_slot_info` | `images.rs`: default/explicit/protected slots, two image pairs, optional upload ID |
| `stats_list_groups`, `stats_get_group_data` | `management.rs`: names, values, missing group |
| `settings_read`, `settings_read_ext`, `settings_write` | `management.rs`: binary/empty values, optional/zero/limited maximum sizes, invalid names |
| `settings_delete`, `settings_commit`, `settings_load`, `settings_save` | `management.rs`: side effects, subtree/all persistence, reload, missing key |
| `fs_file_upload`, `fs_file_download` | `files.rs`: chunk/CBOR boundaries, empty files, overwrite, partial IO, IO failures, premature EOF, progress/cancel, retries, malformed offsets/lengths |
| `fs_file_status`, `fs_file_checksum`, `fs_supported_checksum_types`, `fs_file_close` | `files.rs`: exact size, fixed checksum vectors/ranges, both output types, missing/empty files, close |
| `shell_execute` | `management.rs`: unicode arguments/output, negative return, empty command, opt-in retries/duplicate side effects |
| `enum_get_group_count`, `enum_get_group_ids`, `enum_get_group_id`, `enum_iter_group_ids`, `enum_get_group_details` | `management.rs`: count/list/index/lazy iteration, end/errors, filters, empty filters, optional details |
| `zephyr_erase_storage` | `management.rs`: flash and persistent state change |
| `raw_command` | `protocol.rs`: user-owned command type, typed commands, high group IDs, both opcodes, raw resume exchange |
| `firmware_update` | `firmware_update.rs`: end-to-end transfer/activation/reboot, options, identity hash, already installed, invalid firmware, every remote stage failure, cancellation |

`MCUmgrClientError::command_not_supported` is also exercised with generic and
group-specific errors. Protocol tests cover sequence reuse beyond 256
requests, stale responses, bad headers, invalid CBOR, unknown response fields,
unknown error codes, and preserved transport error categories.

## Baseline regressions

On the pinned crate main, **81 tests pass and two fail**. Neither test is
ignored or rewritten to accept the current client behavior. Adding these
tests without fixing the corresponding bugs will make the full suite red.

Validation used Rust/Cargo 1.98.1 on x86_64 Linux. Both the default feature
configuration and `--all-features` produced the same 81-pass / 2-fail result.
`rustfmt --edition 2024 --check` passed for the integration target and its
modules. The minimum supported Rust version and other operating systems
were not separately validated.

1. `files::empty_file_upload_creates_file_on_device`: calling
   `fs_file_upload("/lfs/empty", &[][..], 0, None)` returns `Ok(())` without
   sending an upload, so no file exists. Zephyr's `fs_mgmt_file_upload`
   opens/creates the file before its `file_data.len > 0` write branch. A
   zero-length upload must still reach the server. This case asserts creation
   of a new empty file; it does not assume empty data truncates an existing
   file, which the referenced handler does not do.
2. `files::upload_rejects_impossible_acknowledgement`: for a four-byte file,
   a deliberately faulty response of `{"off": 999999}` yields `Ok(())`.
   The expected client result is `UnexpectedOffset`. Zephyr's successful
   upload response is the actual accepted byte offset; an offset beyond the
   provided bytes cannot acknowledge a valid upload.

To reproduce individually:

```sh
cargo test -p mcumgr-toolkit --test zephyr_integration files::empty_file_upload_creates_file_on_device -- --exact
cargo test -p mcumgr-toolkit --test zephyr_integration files::upload_rejects_impossible_acknowledgement -- --exact
```

For a temporary baseline of the currently passing cases only:

```sh
cargo test -p mcumgr-toolkit --test zephyr_integration -- \
  --skip files::empty_file_upload_creates_file_on_device \
  --skip files::upload_rejects_impossible_acknowledgement
```

Do not use that exclusion as the permanent CI command. Once the client bugs
are fixed, the full command should pass with no skips. The simulator does not
need a Zephyr checkout or network access at runtime. To update the reference,
review the relevant upstream handlers, record a new immutable revision and
source digests, then change the model and fixtures according to upstream.
