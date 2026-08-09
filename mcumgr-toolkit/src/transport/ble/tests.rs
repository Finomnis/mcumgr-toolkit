use super::*;
use futures::{StreamExt as _, stream};
use proptest::prelude::*;

type TestStream = Pin<Box<dyn futures::Stream<Item = ValueNotification> + Send>>;

const OTHER_SERVICE_UUID: Uuid = Uuid::from_u128(0x11111111_2222_3333_4444_555555555555);
const OTHER_CHARACTERISTIC_UUID: Uuid = Uuid::from_u128(0xaaaaaaaa_bbbb_cccc_dddd_eeeeeeeeeeee);

const SMP_HEADER_SIZE: usize = 8;
const SMP_TRANSFER_BUFFER_SIZE: usize = u16::MAX as usize;

fn notification(service_uuid: Uuid, uuid: Uuid, value: impl Into<Vec<u8>>) -> ValueNotification {
    ValueNotification {
        service_uuid,
        uuid,
        value: value.into(),
    }
}

fn smp_notification(value: impl Into<Vec<u8>>) -> ValueNotification {
    notification(SMP_UUID, CHARACTERISTIC_UUID, value)
}

fn wrong_service_notification(value: impl Into<Vec<u8>>) -> ValueNotification {
    notification(OTHER_SERVICE_UUID, CHARACTERISTIC_UUID, value)
}

fn wrong_characteristic_notification(value: impl Into<Vec<u8>>) -> ValueNotification {
    notification(SMP_UUID, OTHER_CHARACTERISTIC_UUID, value)
}

fn immediate_stream(messages: Vec<ValueNotification>) -> TestStream {
    Box::pin(stream::iter(messages))
}

fn pending_stream() -> TestStream {
    Box::pin(stream::pending())
}

/// Creates a stream where every delay is relative to the previous item and the stream remains
/// open after the last scheduled item.
fn delayed_open_stream(messages: Vec<(Duration, ValueNotification)>) -> TestStream {
    Box::pin(
        stream::iter(messages)
            .then(|(delay, message)| async move {
                tokio::time::sleep(delay).await;
                message
            })
            .chain(stream::pending()),
    )
}

fn smp_frame(payload: &[u8]) -> Vec<u8> {
    let header = SmpHeader {
        ver: 1,
        op: 1,
        flags: 0,
        data_length: payload.len().try_into().unwrap(),
        group_id: 0x1234,
        sequence_num: 0x56,
        command_id: 0x78,
    }
    .to_bytes();

    let mut frame = Vec::with_capacity(SMP_HEADER_SIZE + payload.len());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(payload);
    frame
}

fn header_declaring(payload_len: u16) -> [u8; SMP_HEADER_SIZE] {
    SmpHeader {
        ver: 1,
        op: 1,
        flags: 0,
        data_length: payload_len,
        group_id: 0x1234,
        sequence_num: 0x56,
        command_id: 0x78,
    }
    .to_bytes()
}

fn fragments_for(
    frame: &[u8],
    first_payload_len: usize,
    continuation_sizes: &[usize],
) -> Vec<ValueNotification> {
    let first_end = frame.len().min(SMP_HEADER_SIZE + first_payload_len);
    let mut notifications = vec![smp_notification(frame[..first_end].to_vec())];
    let mut offset = first_end;
    let mut sizes = continuation_sizes.iter().copied().cycle();

    while offset < frame.len() {
        let size = sizes.next().unwrap_or(1).max(1);
        let end = frame.len().min(offset + size);
        notifications.push(smp_notification(frame[offset..end].to_vec()));
        offset = end;
    }

    notifications
}

// ---- next_smp_notification -------------------------------------------------------------

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn accepts_matching_notification() {
    let expected = smp_notification([1, 2, 3]);
    let mut notifications = immediate_stream(vec![expected.clone()]);

    let actual = next_smp_notification(&mut notifications, Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(actual, expected);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn ignores_wrong_service() {
    let expected = smp_notification([4, 5, 6]);
    let mut notifications =
        immediate_stream(vec![wrong_service_notification([1]), expected.clone()]);

    let actual = next_smp_notification(&mut notifications, Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(actual, expected);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn ignores_wrong_characteristic() {
    let expected = smp_notification([4, 5, 6]);
    let mut notifications = immediate_stream(vec![
        wrong_characteristic_notification([1]),
        expected.clone(),
    ]);

    let actual = next_smp_notification(&mut notifications, Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(actual, expected);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn ignores_multiple_unrelated_notifications() {
    let expected = smp_notification([9]);
    let mut notifications = immediate_stream(vec![
        wrong_service_notification([1]),
        wrong_characteristic_notification([2]),
        notification(OTHER_SERVICE_UUID, OTHER_CHARACTERISTIC_UUID, [3]),
        expected.clone(),
    ]);

    let actual = next_smp_notification(&mut notifications, Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(actual, expected);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn closed_notification_stream_is_transport_error() {
    let mut notifications = immediate_stream(vec![]);

    let result = next_smp_notification(&mut notifications, Duration::from_secs(1)).await;

    assert!(matches!(result, Err(ReceiveError::TransportError(_))));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pending_notification_stream_times_out() {
    let timeout = Duration::from_millis(100);
    let mut notifications = pending_stream();
    let start = tokio::time::Instant::now();

    let result = next_smp_notification(&mut notifications, timeout).await;

    assert!(matches!(result, Err(ReceiveError::Timeout)));
    assert_eq!(tokio::time::Instant::now() - start, timeout);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unrelated_notifications_do_not_reset_inactivity_timeout() {
    let timeout = Duration::from_millis(100);
    let mut notifications = delayed_open_stream(vec![
        (Duration::from_millis(30), wrong_service_notification([1])),
        (
            Duration::from_millis(30),
            wrong_characteristic_notification([2]),
        ),
        (Duration::from_millis(30), wrong_service_notification([3])),
    ]);
    let start = tokio::time::Instant::now();

    let result = next_smp_notification(&mut notifications, timeout).await;

    assert!(matches!(result, Err(ReceiveError::Timeout)));
    assert_eq!(tokio::time::Instant::now() - start, timeout);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn empty_smp_notifications_do_not_reset_inactivity_timeout() {
    let timeout = Duration::from_millis(100);
    let mut notifications = delayed_open_stream(vec![
        (Duration::from_millis(30), smp_notification([])),
        (Duration::from_millis(30), smp_notification([])),
        (Duration::from_millis(30), smp_notification([])),
    ]);
    let start = tokio::time::Instant::now();

    let result = next_smp_notification(&mut notifications, timeout).await;

    assert!(matches!(result, Err(ReceiveError::Timeout)));
    assert_eq!(tokio::time::Instant::now() - start, timeout);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn empty_smp_then_valid_notification_succeeds_within_original_timeout() {
    let timeout = Duration::from_millis(100);
    let expected = smp_notification([7, 8]);
    let mut notifications = delayed_open_stream(vec![
        (Duration::from_millis(40), smp_notification([])),
        (Duration::from_millis(40), expected.clone()),
    ]);
    let start = tokio::time::Instant::now();

    let actual = next_smp_notification(&mut notifications, timeout)
        .await
        .unwrap();

    assert_eq!(actual, expected);
    assert_eq!(
        tokio::time::Instant::now() - start,
        Duration::from_millis(80)
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn matching_notification_just_before_timeout_succeeds() {
    let timeout = Duration::from_millis(100);
    let expected = smp_notification([1]);
    let mut notifications =
        delayed_open_stream(vec![(Duration::from_millis(99), expected.clone())]);
    let start = tokio::time::Instant::now();

    let actual = next_smp_notification(&mut notifications, timeout)
        .await
        .unwrap();

    assert_eq!(actual, expected);
    assert_eq!(
        tokio::time::Instant::now() - start,
        Duration::from_millis(99)
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn matching_notification_after_timeout_times_out() {
    let timeout = Duration::from_millis(100);
    let mut notifications =
        delayed_open_stream(vec![(Duration::from_millis(101), smp_notification([1]))]);
    let start = tokio::time::Instant::now();

    let result = next_smp_notification(&mut notifications, timeout).await;

    assert!(matches!(result, Err(ReceiveError::Timeout)));
    assert_eq!(tokio::time::Instant::now() - start, timeout);
}

// ---- receive_smp_frame ----------------------------------------------------------------

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn receives_header_only_frame() {
    let frame = smp_frame(&[]);
    let mut notifications = immediate_stream(vec![smp_notification(frame.clone())]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn receives_complete_frame_in_one_notification() {
    let frame = smp_frame(b"hello BLE");
    let mut notifications = immediate_stream(vec![smp_notification(frame.clone())]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn receives_frame_in_two_notifications() {
    let frame = smp_frame(b"two chunks");
    let split = SMP_HEADER_SIZE + 2;
    let mut notifications = immediate_stream(vec![
        smp_notification(frame[..split].to_vec()),
        smp_notification(frame[split..].to_vec()),
    ]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn receives_frame_in_many_notifications() {
    let frame = smp_frame(b"this payload is deliberately fragmented");
    let messages = fragments_for(&frame, 1, &[1, 2, 3, 4, 5]);
    let mut notifications = immediate_stream(messages);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn receives_one_byte_continuation_fragments() {
    let frame = smp_frame(b"abcdef");
    let messages = fragments_for(&frame, 0, &[1]);
    let mut notifications = immediate_stream(messages);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn first_notification_may_end_exactly_after_header() {
    let frame = smp_frame(b"payload");
    let mut notifications = immediate_stream(vec![
        smp_notification(frame[..SMP_HEADER_SIZE].to_vec()),
        smp_notification(frame[SMP_HEADER_SIZE..].to_vec()),
    ]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn preserves_payload_bytes_exactly() {
    let payload: Vec<u8> = (0..=255).collect();
    let frame = smp_frame(&payload);
    let messages = fragments_for(&frame, 7, &[13, 17, 29]);
    let mut notifications = immediate_stream(messages);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
    assert_eq!(&received[SMP_HEADER_SIZE..], payload);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn ignores_unrelated_notification_before_first_chunk() {
    let frame = smp_frame(b"abc");
    let mut notifications = immediate_stream(vec![
        wrong_service_notification([1, 2, 3]),
        wrong_characteristic_notification([4, 5, 6]),
        smp_notification(frame.clone()),
    ]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn ignores_unrelated_notifications_between_chunks() {
    let frame = smp_frame(b"abcdef");
    let split = SMP_HEADER_SIZE + 2;
    let mut notifications = immediate_stream(vec![
        smp_notification(frame[..split].to_vec()),
        wrong_service_notification([0xaa]),
        wrong_characteristic_notification([0xbb]),
        smp_notification(frame[split..].to_vec()),
    ]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn inactivity_timeout_resets_after_each_progressing_fragment() {
    let timeout = Duration::from_millis(100);
    let frame = smp_frame(b"abcdef");
    let mut notifications = delayed_open_stream(vec![
        (
            Duration::from_millis(80),
            smp_notification(frame[..SMP_HEADER_SIZE + 1].to_vec()),
        ),
        (
            Duration::from_millis(80),
            smp_notification(frame[SMP_HEADER_SIZE + 1..SMP_HEADER_SIZE + 3].to_vec()),
        ),
        (
            Duration::from_millis(80),
            smp_notification(frame[SMP_HEADER_SIZE + 3..].to_vec()),
        ),
    ]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];
    let start = tokio::time::Instant::now();

    let received = receive_smp_frame(&mut notifications, timeout, &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
    assert_eq!(
        tokio::time::Instant::now() - start,
        Duration::from_millis(240)
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn times_out_waiting_for_first_chunk() {
    let mut notifications = pending_stream();
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let result =
        receive_smp_frame(&mut notifications, Duration::from_millis(100), &mut buffer).await;

    assert!(matches!(result, Err(ReceiveError::Timeout)));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn times_out_between_progressing_chunks() {
    let timeout = Duration::from_millis(100);
    let frame = smp_frame(b"abcdef");
    let mut notifications = delayed_open_stream(vec![
        (
            Duration::ZERO,
            smp_notification(frame[..SMP_HEADER_SIZE + 1].to_vec()),
        ),
        (
            Duration::from_millis(101),
            smp_notification(frame[SMP_HEADER_SIZE + 1..].to_vec()),
        ),
    ]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let result = receive_smp_frame(&mut notifications, timeout, &mut buffer).await;

    assert!(matches!(result, Err(ReceiveError::Timeout)));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unrelated_noise_between_chunks_does_not_prevent_timeout() {
    let timeout = Duration::from_millis(100);
    let frame = smp_frame(b"abcdef");
    let mut notifications = delayed_open_stream(vec![
        (
            Duration::ZERO,
            smp_notification(frame[..SMP_HEADER_SIZE + 1].to_vec()),
        ),
        (Duration::from_millis(30), wrong_service_notification([1])),
        (
            Duration::from_millis(30),
            wrong_characteristic_notification([2]),
        ),
        (Duration::from_millis(30), wrong_service_notification([3])),
    ]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];
    let start = tokio::time::Instant::now();

    let result = receive_smp_frame(&mut notifications, timeout, &mut buffer).await;

    assert!(matches!(result, Err(ReceiveError::Timeout)));
    assert_eq!(tokio::time::Instant::now() - start, timeout);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn empty_smp_noise_between_chunks_does_not_prevent_timeout() {
    let timeout = Duration::from_millis(100);
    let frame = smp_frame(b"abcdef");
    let mut notifications = delayed_open_stream(vec![
        (
            Duration::ZERO,
            smp_notification(frame[..SMP_HEADER_SIZE + 1].to_vec()),
        ),
        (Duration::from_millis(30), smp_notification([])),
        (Duration::from_millis(30), smp_notification([])),
        (Duration::from_millis(30), smp_notification([])),
    ]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];
    let start = tokio::time::Instant::now();

    let result = receive_smp_frame(&mut notifications, timeout, &mut buffer).await;

    assert!(matches!(result, Err(ReceiveError::Timeout)));
    assert_eq!(tokio::time::Instant::now() - start, timeout);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn empty_notification_before_frame_is_ignored() {
    let frame = smp_frame(b"abc");
    let mut notifications =
        immediate_stream(vec![smp_notification([]), smp_notification(frame.clone())]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn short_first_chunk_is_unexpected_response() {
    for len in 1..SMP_HEADER_SIZE {
        let mut notifications = immediate_stream(vec![smp_notification(vec![0; len])]);
        let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

        let result =
            receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer).await;

        assert!(
            matches!(result, Err(ReceiveError::UnexpectedResponse)),
            "length {len} should be rejected"
        );
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn declared_frame_larger_than_receive_buffer_is_rejected_immediately() {
    let declared_payload_len: u16 = (SMP_TRANSFER_BUFFER_SIZE - SMP_HEADER_SIZE + 1)
        .try_into()
        .unwrap();
    let mut notifications = immediate_stream(vec![smp_notification(header_declaring(
        declared_payload_len,
    ))]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let result = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer).await;

    assert!(matches!(result, Err(ReceiveError::FrameTooBig)));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn first_notification_overshooting_declared_frame_is_rejected() {
    let mut bytes = header_declaring(1).to_vec();
    bytes.extend_from_slice(&[0xaa, 0xbb]);
    let mut notifications = immediate_stream(vec![smp_notification(bytes)]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let result = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer).await;

    assert!(matches!(result, Err(ReceiveError::UnexpectedResponse)));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn continuation_overshooting_declared_frame_is_rejected() {
    let frame = smp_frame(&[0xaa, 0xbb]);
    let mut notifications = immediate_stream(vec![
        smp_notification(frame[..SMP_HEADER_SIZE + 1].to_vec()),
        smp_notification([0xbb, 0xcc]),
    ]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let result = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer).await;

    assert!(matches!(result, Err(ReceiveError::UnexpectedResponse)));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_closing_mid_frame_is_transport_error() {
    let frame = smp_frame(b"abcdef");
    let mut notifications = immediate_stream(vec![smp_notification(
        frame[..SMP_HEADER_SIZE + 1].to_vec(),
    )]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let result = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer).await;

    assert!(matches!(result, Err(ReceiveError::TransportError(_))));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_closing_before_first_frame_is_transport_error() {
    let mut notifications = immediate_stream(vec![]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let result = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer).await;

    assert!(matches!(result, Err(ReceiveError::TransportError(_))));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn largest_supported_frame_size_succeeds() {
    let payload = vec![0xa5; SMP_TRANSFER_BUFFER_SIZE - SMP_HEADER_SIZE];
    let frame = smp_frame(&payload);
    assert_eq!(frame.len(), SMP_TRANSFER_BUFFER_SIZE);
    let mut notifications = immediate_stream(vec![smp_notification(frame.clone())]);
    let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];

    let received = receive_smp_frame(&mut notifications, Duration::from_secs(1), &mut buffer)
        .await
        .unwrap();

    assert_eq!(received, frame);
}

// ---- Property tests --------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn prop_arbitrary_fragmentation_round_trips(
        payload in prop::collection::vec(any::<u8>(), 0..4096),
        first_payload_len in 0usize..256,
        continuation_sizes in prop::collection::vec(1usize..256, 0..32),
    ) {
        let frame = smp_frame(&payload);
        let messages = fragments_for(&frame, first_payload_len, &continuation_sizes);
        let mut notifications = immediate_stream(messages);
        let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();

        let received = runtime
            .block_on(receive_smp_frame(
                &mut notifications,
                Duration::from_secs(1),
                &mut buffer,
            ))
            .unwrap()
            .to_vec();

        prop_assert_eq!(received, frame);
    }

    #[test]
    fn prop_unrelated_notifications_do_not_change_reassembled_frame(
        payload in prop::collection::vec(any::<u8>(), 0..4096),
        first_payload_len in 0usize..128,
        continuation_size in 1usize..128,
    ) {
        let frame = smp_frame(&payload);
        let clean = fragments_for(&frame, first_payload_len, &[continuation_size]);
        let mut noisy = Vec::with_capacity(clean.len() * 3);
        for message in clean {
            noisy.push(wrong_service_notification([0xaa]));
            noisy.push(wrong_characteristic_notification([0xbb]));
            noisy.push(message);
        }

        let mut notifications = immediate_stream(noisy);
        let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();

        let received = runtime
            .block_on(receive_smp_frame(
                &mut notifications,
                Duration::from_secs(1),
                &mut buffer,
            ))
            .unwrap()
            .to_vec();

        prop_assert_eq!(received, frame);
    }

    #[test]
    fn prop_overrun_is_never_accepted(
        payload in prop::collection::vec(any::<u8>(), 0..4096),
        extra in prop::collection::vec(any::<u8>(), 1..64),
    ) {
        let mut overlong = smp_frame(&payload);
        overlong.extend_from_slice(&extra);
        let mut notifications = immediate_stream(vec![smp_notification(overlong)]);
        let mut buffer = [0; SMP_TRANSFER_BUFFER_SIZE];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();

        let result = runtime.block_on(receive_smp_frame(
            &mut notifications,
            Duration::from_secs(1),
            &mut buffer,
        ));

        prop_assert!(matches!(result, Err(ReceiveError::UnexpectedResponse)));
    }
}
