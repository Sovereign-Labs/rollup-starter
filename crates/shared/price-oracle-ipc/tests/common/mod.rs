use std::time::Duration;

use price_oracle_ipc::{
    connect, read_frame_with_timeout, write_frame, Backoff, BoundListener, OracleFrame, B256,
    PROTOCOL_VERSION,
};

pub async fn listener() -> BoundListener {
    BoundListener::bind("127.0.0.1:0").await.unwrap()
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
        source_time: 1_700_000_000,
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

pub async fn run_consumer(address: String, deadline: Duration, want: usize) -> Vec<OracleFrame> {
    let mut backoff = Backoff::new(Duration::from_millis(1), Duration::from_millis(20));
    let mut collected = Vec::new();
    loop {
        let mut stream = match connect(&address).await {
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
