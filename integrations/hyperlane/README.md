## Bridging in tokens via Hyperlane

This tutorial assumes you're bridging from Solana testnet.

### Step 1: Deploy Contracts

#### Set up relayer for interchain Gas Payments on the Sovereign SDK Chain

### Run One-Time deployment to Counterparty Chains

To get started, you'll need to do a one-time deployment to your counterparty chains. This will involve...

- Changing the HYPERLANE_BRIDGE_DOMAIN in constants.toml, and any usages of `5555` as a domain in the example transactions under the `js` subdirectory.
- Generating new keys for your validator set and relayer
- Deploying new ISM smart contracts to each of your counter-party chains
- Deploying new warp route contracts on your counterparty chains which point to your new ISM
- We're working on ways to automate this, but for now we'd recommend getting in touch with the Sovereign Labs team to help with this initial deployment

### Create a Warp Route on the Sovereign Chain

- Set up interchain gas paymaster to sponsor txs from your counterparty chain(s). You can find an example transaction at `../js/src/set-igp.ts`
- Create a warp route on the Sov side. You can find an example transaction at `../js/src/create-warp-route.ts`. (Note that you'll need to modify this transaction with the appropriate contract address from your new warp route deployment on the counterparty chain)

### Enroll the Sovereign Remote Router on the Counterparty Chain

- Take the Warp Route ID and deploy it as a remote router on your counterparties. Your final config file will look something like this

```
{
	"solanatestnet": {
	  "type": "native",
	  "decimals": 9,
		"name": "sol",
		"symbol": "SOL",
	  "interchainSecurityModule": "7ym4qpYoX5z22wJxuLo3pChpLXvVM6xiW7FEDpseBKbQ" // The new ISM you just deployed on Solana
	},
	"sovstarter": {
		"type": "synthetic",
		"decimals": 9,
		"name": "sol",
		"symbol": "SOL",
		"token": "token_1zaggz39msqa6dceryw25w6770vmlrjrgt2x53yece4pftyvlvvysxn0h7x", // The token ID of the new synthetic on the Sovereign SDK side. Taken from the emitted events
		"foreignDeployment": "0xe14e75006fa7444985b9876c71b6a7689631af98658b10598c5151f18490e812" // The warp route ID on the sovereign side. Taken from the emitted events
	}
}

```

### Step 2: Launch Agents

1. Create a `.env` file providing private keys for your relayer (`RELAYER_KEY`) and validator `VALIDATOR_KEY` (see [`.env.example`](./.env.example))
2. docker-compose up

### Step 3: Send Tokens

To send from SOL -> Sov, you can use our fork of the Hyperlane Warp UI

To send from Sov -> SOL, use the following callmessage:

```
TransferRemote {
    warp_route: route_id,
    destination_domain: {DESTINATION_DOMAIN},
    recipient: {RECIPIENT}, // The eth address of the EOA that should receive funds
    amount: {AMOUNT}, // in units of 1e-18 ETH
    relayer: Some(relayer.address()),  // Optional
    gas_payment_limit: Amount::MAX,
})
```
