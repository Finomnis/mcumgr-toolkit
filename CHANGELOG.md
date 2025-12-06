# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.1] - 2025-12-07

### Changes

- Add `MCUmgrClient::new_from_usb_serial` that connects to a USB VID:PID serial port
  - Python: `MCUmgrClient::usb_serial`
  - CLI: add `-u`/`--usb-serial` flag
    - When no argument specified, list all available ports
- Add `MCUmgrClient::check_connection` that checks if the device is connected and responding
  - CLI: run connection test if no group specified

## [0.5.0] - 2025-12-06

### Breaking Changes

- Python: Rename `MCUmgrParametersResponse` to `MCUmgrParameters`
- `smp_errors::DeviceError` is no longer `Copy`

### Changes

- Add support for SMP v1 error's `rsn` field

## [0.4.2] - 2025-12-06

### Changes

- Make all functions in `MCUmgrClient` `&self` instead of `&mut self` (#62)
- Fix: Python status callbacks deadlock when they call `MCUmgrClient` functions (#62)
- Fix: Infinite loop if serial port returns EOF (#62)
- Add Python/Rust library commands:
  - `image_get_state`
- Add CLI commands:
  - `image`
    - `get_state`

## [0.4.1] - 2025-12-02

### Changes

- CLI:
  - Replace `--progress` with `--quiet` and enable progress bars by default (#57)
  - File copy operations: Add filename to output if output is a directory (#58)

## [0.4.0] - 2025-11-24

### Breaking Changes

- Python: Rename `MCUmgrClient::new_from_serial` to `MCUmgrClient::serial`

### Changes

- `OS` group completed!
- Add Python/Rust library commands:
  - `os_system_reset`
  - `os_mcumgr_parameters`
  - `os_application_info`
  - `os_bootloader_info`
- Add CLI commands:
  - `os`
    - `system-reset`
    - `mcumgr-parameters`
    - `application-info`
    - `bootloader-info`
- Python: Enable log forwarding
- Python: Implement context manager functionality for `MCUmgrClient`

## [0.3.1] - 2025-11-22

### Changes
- Add Python/Rust library commands:
  - `os_task_statistics`
  - `os_get_datetime`
  - `os_set_datetime`
  - `zephyr_erase_storage`
- Add CLI commands:
  - `os`
    - `task-statistics`
    - `set-datetime`
    - `get-datetime`
  - `zephyr`
    - `erase_storage`
- Add `__repr__` to all Python data objects to make them `print`able
- Improve Python release:
  - Add API documentation
  - Improve tags and links on PyPI website

## [0.3.0] - 2025-11-18

### Breaking changes

- Refactor `file_upload_max_data_chunk_size`:
  - Require `filename` as parameter
  - Fix computational bug
  - Add error return value
  - Is no longer `const`
- `FileClose` command now returns `FileCloseResponse` instead of `()`. Can be converted to `()` via `From`/`Into`.
- Add `FileUploadError::FrameSizeTooSmall`

### Changes

- Make all command structs `Eq, PartialEq`
- Fix CBOR serialization/deserialization of empty structs

## [0.2.1] - 2025-11-14

### Changes
- Complete the `fs` command group
  - Add Python/Rust library commands:
    - `fs_file_status`
    - `fs_file_checksum`
    - `fs_supported_checksum_types`
    - `fs_file_close`
  - Add CLI commands:
    - `fs`
      - `status`
      - `checksum`
      - `supported-checksums`
      - `close`
- Add CLI output options:
  - `--verbose` flag for detailed output
  - `--json` flag for structured JSON output


## [0.2.0] - 2025-11-13

### Breaking Changes
- Python: `shell_execute` now returns a `str` and raises an error on negative shell exit code (#38)

### Changes
- Fix error for `raw` command when response contains a bytes array (#39)
- Improve CBOR encoding/decoding error messages (#39)

## [0.1.1] - 2025-11-12

### Changes
- Add `Errno` enum and use it to decode shell command errors in CLI (#37)
- Add link to implementation progress in README (#35)

## [0.1.0] - 2025-11-11

### Changes
- Add Python commands:
  - `fs_file_download`
  - `fs_file_upload`
- Add CLI commands:
  - `fs`
    - `upload`
    - `download`
- Add progress callbacks for upload/download
- Refactor error enums
- Rework python error messages

## [0.0.2] - 2025-11-10

### Changes
- Rename `with_frame_size` to `set_frame_size` and make it non-consuming (#20)
- Add MCUmgrClient commands: (#20)
  - `set_frame_size`
  - `use_auto_frame_size`
  - `set_timeout_ms`
  - `shell_execute`
  - `raw_command`
- Add separate README for Pypi. (#19)
- Add Python docstrings.  (#19)


## [0.0.1] - 2025-11-10

Initial release, not feature complete yet.

Primarily to test release workflow.

[0.5.1]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.5.0...0.5.1
[0.5.0]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.4.2...0.5.0
[0.4.2]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.4.1...0.4.2
[0.4.1]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.4.0...0.4.1
[0.4.0]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.3.1...0.4.0
[0.3.1]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.3.0...0.3.1
[0.3.0]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.2.1...0.3.0
[0.2.1]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.2.0...0.2.1
[0.2.0]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.1.1...0.2.0
[0.1.1]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.1.0...0.1.1
[0.1.0]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.0.2...0.1.0
[0.0.2]: https://github.com/Finomnis/zephyr-mcumgr-client/compare/0.0.1...0.0.2
[0.0.1]: https://github.com/Finomnis/zephyr-mcumgr-client/releases/tag/0.0.1
