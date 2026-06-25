mod common;

use std::time::Duration;

use price_oracle_ipc::{
    connect, read_frame_with_timeout, write_frame, write_frame_with_timeout, IpcError, OracleFrame,
};
use tokio::sync::oneshot;

#[tokio::test]
async fn session_round_trip() {
    let listener = common::listener().await;
    let address = listener.address().to_string();
    let expected = vec![
        common::hello(),
        common::update(0x01, b"snap-1"),
        common::update(0x02, b"snap-2"),
        common::heartbeat(7),
        common::update(0x01, b"live-1"),
    ];
    let server = common::serve_once(listener, expected.clone());

    let mut client = connect(&address).await.unwrap();
    let mut got = Vec::new();
    for _ in 0..expected.len() {
        got.push(
            read_frame_with_timeout(&mut client, Duration::from_secs(5))
                .await
                .unwrap(),
        );
    }

    assert_eq!(got, expected);
    server.await.unwrap();
}

#[tokio::test]
async fn silent_server_trips_read_deadline() {
    let listener = common::listener().await;
    let address = listener.address().to_string();
    let (tx, rx) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        write_frame(&mut stream, &common::hello()).await.unwrap();
        let _ = rx.await;
    });

    let mut client = connect(&address).await.unwrap();
    let hello = read_frame_with_timeout(&mut client, Duration::from_secs(5))
        .await
        .unwrap();
    assert!(matches!(hello, OracleFrame::Hello { .. }));

    let timed_out = read_frame_with_timeout(&mut client, Duration::from_millis(50)).await;
    assert!(matches!(timed_out, Err(IpcError::ReadTimeout)));

    drop(tx);
    server.await.unwrap();
}

#[tokio::test]
async fn server_disconnect_reports_closed() {
    let listener = common::listener().await;
    let address = listener.address().to_string();
    let server = common::serve_once(listener, vec![common::hello()]);

    let mut client = connect(&address).await.unwrap();
    let hello = read_frame_with_timeout(&mut client, Duration::from_secs(5))
        .await
        .unwrap();
    assert!(matches!(hello, OracleFrame::Hello { .. }));

    let closed = read_frame_with_timeout(&mut client, Duration::from_secs(5)).await;
    assert!(matches!(closed, Err(IpcError::Closed)));
    server.await.unwrap();
}

#[tokio::test]
async fn write_trips_deadline_when_peer_never_reads() {
    let listener = common::listener().await;
    let address = listener.address().to_string();
    let server = tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let big = common::update(0x01, &vec![0u8; 1024 * 1024]);
        for _ in 0..1000 {
            match write_frame_with_timeout(&mut stream, &big, Duration::from_millis(100)).await {
                Ok(()) => continue,
                Err(e) => return e,
            }
        }
        panic!("expected a write to time out, but all writes succeeded");
    });

    let _client = connect(&address).await.unwrap();
    let err = server.await.unwrap();
    assert!(matches!(err, IpcError::WriteTimeout));
}

#[tokio::test]
async fn consumer_reconnects_after_drop() {
    let listener = common::listener().await;
    let address = listener.address().to_string();
    let server = tokio::spawn(async move {
        let mut first = listener.accept().await.unwrap();
        write_frame(&mut first, &common::hello()).await.unwrap();
        drop(first);

        let mut second = listener.accept().await.unwrap();
        write_frame(&mut second, &common::hello()).await.unwrap();
        write_frame(&mut second, &common::update(0x09, b"after-reconnect"))
            .await
            .unwrap();
    });

    let collected = common::run_consumer(address, Duration::from_secs(5), 3).await;
    assert_eq!(collected.len(), 3);
    assert_eq!(collected[2], common::update(0x09, b"after-reconnect"));
    server.await.unwrap();
}

#[tokio::test]
async fn streams_many_frames_in_order() {
    let listener = common::listener().await;
    let address = listener.address().to_string();
    let expected: Vec<OracleFrame> = (0u32..100)
        .map(|i| common::update(0x01, &i.to_le_bytes()))
        .collect();
    let server = common::serve_once(listener, expected.clone());

    let mut client = connect(&address).await.unwrap();
    let mut got = Vec::with_capacity(expected.len());
    for _ in 0..expected.len() {
        got.push(
            read_frame_with_timeout(&mut client, Duration::from_secs(5))
                .await
                .unwrap(),
        );
    }

    assert_eq!(got, expected);
    server.await.unwrap();
}
