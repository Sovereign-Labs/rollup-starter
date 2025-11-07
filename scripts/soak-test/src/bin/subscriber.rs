use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let api_url = "http://localhost:12346";
    let client = sov_api_spec::Client::new(api_url);

    println!("Starting subscription");
    let mut sub = client.subscribe_to_events().await?;

    println!("Subscription started");
    while let Some(event) = sub.next().await {
        println!("{:?}", event);
    }
    Ok(())
}
