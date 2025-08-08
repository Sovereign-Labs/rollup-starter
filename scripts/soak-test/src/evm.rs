use clap::Parser;
use sov_eth_dev_signer::{SecretKey, TransferGenerator};
const KEYS: [(&str, &str); 11] = [
	("0x5cAA360D300fcA6DaFEBce1a693B9daC949738D2", "0x3140b0283415a0909797e94f4993897fed5336d175ef52622fb7187c792abec6"),
	("0x503210c9A90d61FC0772bAC00aba9b12b6Fa9EDC", "0x9cbe35b6214ded5b926d77b8357acab8c60222a362e5385e543424a1645720d8"),
	("0xf1ceAC618Bc181450a052aA23bCDF3a8b328Edc3", "0x563b8b95877b29e009c3f5b3bbc54d20b32ade058be54a6fc66c980b06ca42b6"),
	("0x503210c9A90d61FC0772bAC00aba9b12b6Fa9EDC", "0x9cbe35b6214ded5b926d77b8357acab8c60222a362e5385e543424a1645720d8"),
	("0x0fbd0C30aEEEd9d2f9936B6A0A2c87Df0F8fCDC3", "0xb5153e51d4665b79ed9260cd4c68a49a7d238df7fcabadc9296954f071f3da72"),
	("0x147Af6275322304cD0f6C5948d89ef9ae70757d4", "0xabb4ceef34add0aeb3d60afa946e213fbc9936be92e7fdb479bc3fe924f5275d"),
	("0x2780F1FC8064Ae158F414bBB9A0a96584f83f72a", "0xba15c0be634a57a3eef89891242021d286d76275d318ffe690564ff84c897a3c"),
	("0x0E88357eB4E0aA75F8DbA2cB8141803cd560C757", "0x2d82705adac6ee5827dcf7bd6116e65c1d497d9234fef3ea3970a42233bcf4a1"),
	("0xb7E55830E9E7D431A2B31f15c98D9c20Dc477A8d", "0x3b4f9220aa84e37de18f66657c49ee87e9dccbe77770077b3fafbf58e1d5e0d2"),
	("0x45131F098386322909d4A45132810fB403340BD4", "0xfcc78a596cb660b9ebaf8052e701e8ba0cd7e4f566986a5c6d9d1f12c7134602"),
	("0x22545Cea3C9A4197a4e920b0A471e8f114B05611", "0xb73655e75cd758d8be543b414b24884df20b2ed5360acc2709111b6525b3545c"),
];

#[tokio::main]
async fn main() {
	use std::str::FromStr;
	let args = Args::parse();
	let mut tasks = Vec::new();
	for w in 0..args.num_workers {

		let key = KEYS[w as usize].1;
    	let mut generator = TransferGenerator::new(SecretKey::from_str(key.trim_start_matches("0x")).unwrap(), (w + args.salt) as u128);
		tasks.push(tokio::task::spawn( {
			let mut nonce = 0;
			let client = reqwest::Client::new();
			let url = reqwest::Url::parse(&args.api_url).unwrap();
			let mut success = 0;
			async move{
				loop {
					let tx = generator.generate(nonce);
					nonce += 1;
					for _ in 0..5 {
						let Ok(res )= client.post(url.clone()).json(&serde_json::json!({
							"jsonrpc": "2.0",
							"method": "eth_sendRawTransaction",
							"params": [tx],
							"id": 1
						})).send().await else {
							println!("Worker {} failed to send transaction. Retrying", w);
							tokio::time::sleep(std::time::Duration::from_millis(100)).await;
							continue;
						};
						let res  = res.text().await.unwrap();
						assert!(res.contains("\"result\""), "Unexpected response: {}", res);
						success += 1;
					}
					if success % 100 == 0 {
						tracing::debug!("Worker {} sent 100 more transactions. Total for worker: {}", w, success);
					}
				}
					
			}
			
		}));
	}
	for task in tasks {
		task.await.unwrap();
	}
	println!("Done");
}

#[derive(Parser)]
struct Args {
    #[arg(short, long, default_value = "http://localhost:12346/rpc")]
    /// The URL of the rollup node to connect to. Defaults to http://localhost:12346.
    api_url: String,

    #[arg(short, long, default_value = "5")]
    /// The number of workers to spawn - this controls the number of concurrent transactions. Defaults to 5.
    num_workers: u32,

    #[arg(short, long, default_value = "0")]
    /// The salt to use for RNG. Use this value if you're restarting the generator and want to ensure that the generated
    /// transactions don't overlap with the previous run.
    salt: u32,
}
