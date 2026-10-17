use crate::{
    assert_group_error, assert_smp_error,
    zephyr_sim::{
        Config, Fault, client, client_with,
        device::GROUPS,
        wire::{RemoteError, bytes, map, text, uint},
    },
};

#[test]
fn settings_binary_values_and_read_limits() {
    let (client, handle) = client();
    let value: Vec<u8> = (0..32).collect();
    client.settings_write("app/key", &value).unwrap();
    assert_eq!(handle.inspect(|d| d.settings["app/key"].clone()), value);
    assert_eq!(client.settings_read("app/key").unwrap(), value);
    let short = client.settings_read_ext("app/key", Some(7)).unwrap();
    assert_eq!(short.val, value[..7]);
    assert_eq!(short.max_size, None);
    let limited = client.settings_read_ext("app/key", Some(100)).unwrap();
    assert_eq!(limited.val, value);
    assert_eq!(limited.max_size, Some(32));
    assert!(
        client
            .settings_read_ext("app/key", Some(0))
            .unwrap()
            .val
            .is_empty()
    );
    assert_eq!(
        client.settings_read_ext("app/key", None).unwrap().max_size,
        None
    );
}

#[test]
fn settings_save_load_commit_and_delete_have_distinct_effects() {
    let (client, handle) = client();
    client.settings_write("app/key", b"old").unwrap();
    client.settings_write("other/key", b"other").unwrap();
    client
        .settings_write("application/key", b"different subtree")
        .unwrap();
    client.settings_commit().unwrap();
    assert_eq!(handle.inspect(|d| d.commits), 1);
    assert!(handle.inspect(|d| d.persisted.is_empty()));
    client.settings_save(Some("app")).unwrap();
    assert_eq!(
        handle.inspect(|d| d.persisted.keys().cloned().collect::<Vec<_>>()),
        ["app/key"]
    );
    client.settings_write("app/key", b"new").unwrap();
    client.settings_load().unwrap();
    assert_eq!(client.settings_read("app/key").unwrap(), b"old");
    client.settings_save(None::<&str>).unwrap();
    assert_eq!(handle.inspect(|d| d.persisted.len()), 3);
    client.settings_delete("app/key").unwrap();
    assert!(!handle.inspect(|d| d.settings.contains_key("app/key")));
    assert_group_error(client.settings_read("app/key").unwrap_err(), 3, 3);
    assert_group_error(client.settings_delete("app/missing").unwrap_err(), 3, 3);
}

#[test]
fn settings_empty_value_and_invalid_name() {
    let (client, _) = client();
    client.settings_write("app/empty", &[]).unwrap();
    assert!(client.settings_read("app/empty").unwrap().is_empty());
    assert_smp_error(client.settings_read("").unwrap_err(), 3);
    assert_group_error(
        client.settings_write("x".repeat(64), b"v").unwrap_err(),
        3,
        2,
    );
}

#[test]
fn stats_list_and_values() {
    let (client, _) = client();
    assert_eq!(client.stats_list_groups().unwrap(), ["net"]);
    let stats = client.stats_get_group_data("net").unwrap();
    assert_eq!(stats.len(), 3);
    assert_eq!(
        (stats["rx"], stats["tx"], stats["errors"]),
        (123, u64::from(u32::MAX), 0)
    );
    assert_group_error(client.stats_get_group_data("missing").unwrap_err(), 2, 2);
}

#[test]
fn enum_count_list_index_and_lazy_iterator() {
    let (client, handle) = client();
    assert_eq!(client.enum_get_group_count().unwrap(), GROUPS.len() as u16);
    assert_eq!(client.enum_get_group_ids().unwrap(), GROUPS);
    for (index, id) in GROUPS.iter().enumerate() {
        assert_eq!(client.enum_get_group_id(index as u16).unwrap(), *id);
    }
    let before = handle.requests().len();
    let mut iter = client.enum_iter_group_ids();
    assert_eq!(handle.requests().len(), before, "iterator must be lazy");
    let got: Vec<_> = iter.by_ref().map(Result::unwrap).collect();
    assert_eq!(got, GROUPS);
    let after = handle.requests().len();
    assert!(iter.next().is_none());
    assert!(iter.next().is_none());
    assert_eq!(handle.requests().len(), after, "no requests after end=true");
    // A client may also fetch the count once; that is an implementation choice.
    let requests = handle.requests();
    assert_eq!(
        requests[before..after]
            .iter()
            .filter(|r| r.group() == 10 && r.id() == 2)
            .count(),
        GROUPS.len()
    );
    assert_group_error(
        client.enum_get_group_id(GROUPS.len() as u16).unwrap_err(),
        10,
        4,
    );
}

#[test]
fn enum_details_filter_and_optional_fields() {
    let (client, _) = client();
    let all = client.enum_get_group_details(None).unwrap();
    assert_eq!(all.iter().map(|g| g.group).collect::<Vec<_>>(), GROUPS);
    assert_eq!(all[0].name.as_deref(), Some("os mgmt"));
    assert_eq!(all[0].handlers, Some(9));
    let selected = client.enum_get_group_details(Some(&[8, 0, 65535])).unwrap();
    assert_eq!(selected.iter().map(|g| g.group).collect::<Vec<_>>(), [0, 8]);
    // enum_mgmt_details treats an empty filter like an omitted filter.
    assert_eq!(
        client.enum_get_group_details(Some(&[])).unwrap().len(),
        GROUPS.len()
    );
    let (client, _) = client_with(Config {
        enum_details: false,
        ..Config::default()
    });
    assert!(
        client
            .enum_get_group_details(None)
            .unwrap()
            .iter()
            .all(|g| g.name.is_none() && g.handlers.is_none())
    );
}

#[test]
fn enum_iterator_surfaces_device_error() {
    let (client, handle) = client();
    handle.fault_on(10, 2, 1, Fault::Reply(RemoteError::Group(10, 4).body()));
    let mut iter = client.enum_iter_group_ids();
    assert_eq!(iter.next().unwrap().unwrap(), 0);
    assert_group_error(iter.next().unwrap().unwrap_err(), 10, 4);
    handle.assert_faults_consumed();
}

#[test]
fn shell_preserves_unicode_output_and_negative_exit_code() {
    let (client, handle) = client();
    let argv = vec!["echo".to_string(), "Grüße 🦀".into(), "world".into()];
    assert_eq!(
        client.shell_execute(&argv, false).unwrap(),
        (0, "Grüße 🦀 world\n".into())
    );
    assert_eq!(handle.inspect(|d| d.shell_calls[0].clone()), argv);
    assert_eq!(
        client.shell_execute(&["fail".into()], true).unwrap(),
        (-22, "invalid argument\n".into())
    );
    assert_group_error(client.shell_execute(&[], false).unwrap_err(), 9, 3);
}

#[test]
fn shell_retry_opt_in_controls_duplicate_side_effects() {
    for retry in [false, true] {
        let (client, handle) = client();
        client.set_retries(2);
        handle.fault(Fault::LoseReply);
        let result = client.shell_execute(&["echo".into(), "hi".into()], retry);
        if retry {
            assert_eq!(result.unwrap(), (0, "hi\n".into()));
        } else {
            assert!(result.is_err());
        }
        assert_eq!(
            handle.inspect(|d| d.shell_calls.len()),
            if retry { 2 } else { 1 }
        );
        handle.assert_faults_consumed();
    }
}

#[test]
fn storage_erase_changes_flash_but_not_runtime_settings() {
    let (client, handle) = client();
    client.settings_write("app/key", b"value").unwrap();
    client.settings_save(None::<&str>).unwrap();
    client.zephyr_erase_storage().unwrap();
    assert_eq!(handle.inspect(|d| d.storage_erases), 1);
    assert!(handle.inspect(|d| d.storage.iter().all(|b| *b == 0xff)));
    assert!(handle.inspect(|d| d.persisted.is_empty()));
    assert_eq!(client.settings_read("app/key").unwrap(), b"value");
}

#[test]
fn legacy_success_rc_zero_is_accepted_across_groups() {
    let (client, _) = client_with(Config {
        legacy_success_rc: true,
        ..Config::default()
    });
    assert_eq!(client.os_echo("rc=0").unwrap(), "rc=0");
    client.settings_write("app/key", b"v").unwrap();
    assert_eq!(client.settings_read("app/key").unwrap(), b"v");
    client.settings_commit().unwrap();
    assert!(!client.stats_list_groups().unwrap().is_empty());
    assert!(!client.image_get_state().unwrap().is_empty());
    assert_eq!(client.enum_get_group_count().unwrap(), 8);
    client.fs_file_close().unwrap();
    client.zephyr_erase_storage().unwrap();
}

#[test]
fn unknown_response_fields_are_forward_compatible() {
    let (client, handle) = client();
    handle.fault(Fault::Reply(map([
        ("val", bytes(b"value")),
        (
            "future",
            map([("flag", text("new extension")), ("counter", uint(123u32))]),
        ),
    ])));
    assert_eq!(client.settings_read("app/key").unwrap(), b"value");
}
