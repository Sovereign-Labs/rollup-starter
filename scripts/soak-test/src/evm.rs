use clap::Parser;
use sov_eth_dev_signer::{SecretKey, TransferGenerator};
const KEYS: [(&str, &str); 20] = [
	("0x5cAA360D300fcA6DaFEBce1a693B9daC949738D2", "0x3140b0283415a0909797e94f4993897fed5336d175ef52622fb7187c792abec6"),
	("0x503210c9A90d61FC0772bAC00aba9b12b6Fa9EDC", "0x9cbe35b6214ded5b926d77b8357acab8c60222a362e5385e543424a1645720d8"),
	("0xf1ceAC618Bc181450a052aA23bCDF3a8b328Edc3", "0x563b8b95877b29e009c3f5b3bbc54d20b32ade058be54a6fc66c980b06ca42b6"),
	("0x0fbd0C30aEEEd9d2f9936B6A0A2c87Df0F8fCDC3", "0xb5153e51d4665b79ed9260cd4c68a49a7d238df7fcabadc9296954f071f3da72"),
	("0x147Af6275322304cD0f6C5948d89ef9ae70757d4", "0xabb4ceef34add0aeb3d60afa946e213fbc9936be92e7fdb479bc3fe924f5275d"),
	("0x2780F1FC8064Ae158F414bBB9A0a96584f83f72a", "0xba15c0be634a57a3eef89891242021d286d76275d318ffe690564ff84c897a3c"),
	("0x0E88357eB4E0aA75F8DbA2cB8141803cd560C757", "0x2d82705adac6ee5827dcf7bd6116e65c1d497d9234fef3ea3970a42233bcf4a1"),
	("0xb7E55830E9E7D431A2B31f15c98D9c20Dc477A8d", "0x3b4f9220aa84e37de18f66657c49ee87e9dccbe77770077b3fafbf58e1d5e0d2"),
	("0x45131F098386322909d4A45132810fB403340BD4", "0xfcc78a596cb660b9ebaf8052e701e8ba0cd7e4f566986a5c6d9d1f12c7134602"),
	("0x22545Cea3C9A4197a4e920b0A471e8f114B05611", "0xb73655e75cd758d8be543b414b24884df20b2ed5360acc2709111b6525b3545c"),
	("0xEca46725aBEBe0ba3A6c4D202A5Cd7388B4752A0", "0xe94daa199f53f8ef97e159709ed6c3e03a2e30c534637978703c649ca0673933"),
	("0x8de0a26B3167B6e9703297f0c680f607dab04DcE", "0x33e658f7a04db9c23a7063bf7d393aed90d35fe29f606dcd31730a6b78ce9506"),
	("0x716D2794f6B8a6fAD79a4634c444c875B9C1a9a2", "0xf35bbc3d65d31666bbe0bf2061dd60ff3506d99846d0dea2571120044ce26668"),
	("0xAd7096467877382A35A532FAc9886D8378F8F8E8", "0x4dee55f67f7f8a648a2f68e3eb85afa407dc8f4706ecff95332889373ec5706d"),
	("0x6d85315f4ba6e6f367A252660c8d9702C57922Dc", "0xb74579448def8eaa0fb111a98f22fabe50ba0b2f9d3902468c71ed0f98847edf"),
	("0x3973CfB35983592d603649D980966E9765346ed9", "0x482bbc019018d16799691ce7689334a99574c34dbb6322afffd55b361d889288"),
	("0xf3c182417d1172835BEc2FaF36C12d45DcD90c01", "0xf682959346866c4bf99a903f5e79eb8ff70a871605f5c44d51a6d11e7edc28df"),
	("0xAC40b4EFDc2Bd4BC937505409708aAA8D17172a9", "0x7c9d62f5e7c45b1ea5dd817bbf91991b81b26da120bed47cbe36d4b3983eb125"),
	("0x63e4b21Dde03dDFf847c31Fc6D00B6bBe38c206b", "0xe7be4e3bc51211cbb83463339f4188241e84ebdacba2dc7b67783615920ed424"),
	("0xa95a33aD06910489e65aAa5e9B72C1a4bbBE0Ee2", "0xf3edac577cd0614609875c8aab34496ea3ef5adf1cb19652d4690fe96f8bd024"),
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
					let tx_hex = hex::encode(tx.envelope_encoded());
					let body = serde_json::json!({
							"jsonrpc": "2.0",
							"method": "eth_sendRawTransaction",
							"params": [tx_hex],
							"id": 1
						});
					for attempt in 1..=5 {
						
						let res = match client.post(url.clone()).json(&body).send().await 
						{
							Ok(res) => res,
							Err(e) => {
								println!("Worker {} failed to send transaction with err {}. Retrying", w, e);
								tokio::time::sleep(std::time::Duration::from_millis(100)).await;
								continue;
							}
						};
						let res  = res.text().await.unwrap();
						assert!(res.contains("\"result\""), "Unexpected response: {}", res);
						success += 1;
						break;
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

    #[arg(short, long, default_value = "20")]
    /// The number of workers to spawn - this controls the number of concurrent transactions. Defaults to 5.
    num_workers: u32,

    #[arg(short, long, default_value = "0")]
    /// The salt to use for RNG. Use this value if you're restarting the generator and want to ensure that the generated
    /// transactions don't overlap with the previous run.
    salt: u32,
}
