use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use serde::Deserialize;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, default_value = "http://localhost:12346")]
    /// The URL of the rollup node to connect to.
    api_url: String,

    #[arg(short = 'i', long, default_value_t = 1000)]
    /// Poll interval in milliseconds.
    interval_ms: u64,
}

#[derive(Deserialize, Debug)]
struct ValueResponse {
    value: (u64, u64),
}

async fn read_rollup_height(client: &reqwest::Client, api_url: &str) -> anyhow::Result<u64> {
    let endpoint = format!(
        "{}/modules/chain-state/state/current-heights/",
        api_url.trim_end_matches('/')
    );

    let response = client.get(&endpoint).send().await?;

    let heights = response
        .json::<ValueResponse>()
        .await
        .context("failed to decode current-heights response")?;

    Ok(heights.value.0)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    let poll_every_ms = args.interval_ms;
    let client = reqwest::ClientBuilder::new()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(1))
        .build()?;
    let mut interval = tokio::time::interval(Duration::from_millis(poll_every_ms));

    loop {
        interval.tick().await;
        match read_rollup_height(&client, &args.api_url).await {
            Ok(height) => println!("rollup_height={height}"),
            Err(err) => eprintln!("failed to read rollup height: {err:#}"),
        }
    }
}
