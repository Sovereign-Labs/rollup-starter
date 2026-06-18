use std::time::Duration;

use price_oracle_ipc::{
    connect, read_frame_with_timeout, write_frame, Backoff, BoundListener, Endpoint, OracleFrame,
    B256, PROTOCOL_VERSION,
};
use tempfile::TempDir;

#[derive(Clone, Copy)]
pub enum Kind {
    Unix,
    Tcp,
}

pub async fn listener(kind: Kind) -> (BoundListener, Option<TempDir>) {
    match kind {
        Kind::Tcp => (
            BoundListener::bind(Endpoint::tcp("127.0.0.1:0"))
                .await
                .unwrap(),
            None,
        ),
        Kind::Unix => {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("oracle.sock");
            let listener = BoundListener::bind(Endpoint::unix(path)).await.unwrap();
            (listener, Some(dir))
        }
    }
}

pub fn hello() -> OracleFrame {
    OracleFrame::Hello {
        protocol_version: PROTOCOL_VERSION,
        provider_id: B256::repeat_byte(0x11),
        feeds: vec![B256::repeat_byte(0x01)],
        heartbeat_interval_sec: 1,
    }
}

pub fn update(feed: u8, payload: &[u8]) -> OracleFrame {
    OracleFrame::PriceUpdate {
        provider_id: B256::repeat_byte(0x11),
        feed_id: B256::repeat_byte(feed),
        payload: payload.to_vec(),
        ingested_at: 1_700_000_000,
    }
}

pub fn heartbeat(sent_at_unix: u64) -> OracleFrame {
    OracleFrame::Heartbeat { sent_at_unix }
}

pub fn serve_once(
    listener: BoundListener,
    frames: Vec<OracleFrame>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        for frame in &frames {
            write_frame(&mut stream, frame).await.unwrap();
        }
    })
}

pub async fn run_consumer(endpoint: Endpoint, deadline: Duration, want: usize) -> Vec<OracleFrame> {
    let mut backoff = Backoff::new(Duration::from_millis(1), Duration::from_millis(20));
    let mut collected = Vec::new();
    loop {
        let mut stream = match connect(&endpoint).await {
            Ok(stream) => {
                backoff.reset();
                stream
            }
            Err(_) => {
                tokio::time::sleep(backoff.next_delay()).await;
                continue;
            }
        };
        while let Ok(frame) = read_frame_with_timeout(&mut stream, deadline).await {
            collected.push(frame);
            if collected.len() >= want {
                return collected;
            }
        }
    }
}
