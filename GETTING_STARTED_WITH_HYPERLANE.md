# Bridging Tokens via Hyperlane

This tutorial demonstrates how to configure token bridging from an EVM-compatible chain to your rollup using Hyperlane.

[Anvil](https://getfoundry.sh/anvil/reference/#anvil) is used for demonstration because it can be run locally.

## High-Level Overview

1. Start the rollup
2. Start Docker Compose with Anvil and Hyperlane agents
3. Configure warp routes
4. Make inbound transfers
5. Make outbound transfers
6. Troubleshooting

## 1. Start the Rollup

Start the rollup and keep it running throughout this tutorial.

```bash,test-ci,bashtestmd:long-running,bashtestmd:wait-until=rest_address
$ cargo run
```

## 2. Start Anvil and Hyperlane Agents

Start Anvil (local Ethereum node) and the Hyperlane agents.

Consider login into Github `docker login ghcr.io` if there's rate-limit issue about pulling the images

```bash,test-ci,bashtestmd:exit-code=0
$ make start-hyperlane-ethtest
 ✔ Network hyperlane_default                    Created                                                                                                                                                                                  0.0s
 ✔ Container hyperlane-anvil-1                  Healthy                                                                                                                                                                                  5.7s
 ✔ Container hyperlane-hyperlane-core-deploy-1  Exited                                                                                                                                                                                 102.8s
 ✔ Container hyperlane-hyperlane-warp-deploy-1  Exited                                                                                                                                                                                 175.4s
 ✔ Container hyperlane-relayer-1                Started                                                                                                                                                                                175.5s
 ✔ Container hyperlane-validator-ethtest-1      Started                                                                                                                                                                                175.5s
waiting for containers to become operational (timeout: 300 seconds)...
[2025-09-22 19:18:18] Health check - validator: 'starting', relayer: 'starting' (elapsed: 0s)
[2025-09-22 19:18:18] Waiting for hyperlane containers to be up and running...
[2025-09-22 19:18:21] Health check - validator: 'healthy', relayer: 'healthy' (elapsed: 3s)
[2025-09-22 19:18:21] ✔ All hyperlane containers are healthy
 ✔ Hyperlane ethtest containers are ready.
```

This command will deploy Hyperlane core contracts and set up the warp route.

### 2.1 Verify the Setup

Print the warp route configuration on Ethtest. Notice that the `remoteRouters` map is initially empty:

```bash,test-ci,bashtestmd:compare-output
$ make print-hyperlane-ethtest-warp
✅ Warp route config read successfully:

    ethtest:
      owner: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
      mailbox: "0x8A791620dd6260079BF849Dc5567aDC3F2FdC318"
      hook: "0x0000000000000000000000000000000000000000"
      interchainSecurityModule:
        address: "0x68B1D87F95878fE05B998F19b66F4baba5De1aed"
        type: testIsm
      remoteRouters: {}
      name: Ether
      symbol: ETH
      decimals: 18
      isNft: false
      contractVersion: 9.0.6
      type: native
      allowedRebalancers: []
      allowedRebalancingBridges: {}
      proxyAdmin:
        address: "0x3Aa5ebB10DC797CAC828524e59A333d0A371443c"
        owner: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
      destinationGas: {}
```

Verify the validator announcement message:

```bash,test-ci,bashtestmd:compare-output
$ cat integrations/hyperlane/docker-data/validator-ethtest/signatures/announcement.json
{
  "value": {
    "validator": "0x70997970c51812dc3a010c7d01b50e0d17dc79c8",
    "mailbox_address": "0x0000000000000000000000008a791620dd6260079bf849dc5567adc3f2fdc318",
    "mailbox_domain": 3133790210,
    "storage_location": "file:///ethtest-validator-signatures"
  },
  "signature": {
    "r": "0xe41dbc8132819dfacf08219a66c1ad553f9bacc76bf62df5a2b2b037cb5b365f",
    "s": "0x527981e9f2a77fd152cbd0161051620d5587f4b7691d467327d9d29ee12e177a",
    "v": 27
  },
  "serialized_signature": "0xe41dbc8132819dfacf08219a66c1ad553f9bacc76bf62df5a2b2b037cb5b365f527981e9f2a77fd152cbd0161051620d5587f4b7691d467327d9d29ee12e177a1b"
}
```

Confirm that the relayer is running and metrics show meaningful data. For example, check the balance for ethtest:

```bash
$ curl -Ss http://127.0.0.1:9091/metrics | grep 'hyperlane_wallet_balance'
hyperlane_wallet_balance{agent="relayer",chain="ethtest",hyperlane_baselib_version="0.1.0",token_address="none",token_name="Native",token_symbol="Native",wallet_address="3c44cdddb6a900fa2b585dd299e03d12fa4293bc",wallet_name="relayer"} 10000
```

## 3. Configure Warp Routes

First, install dependencies for the JavaScript scripts if not already done:

```bash,test-ci,bashtestmd:exit-code=0
$ cd examples/starter-js && npm install
```

Set up the warp route on the rollup side. This script will:

* Register a warp router and add a remote route on ethtest
* Configure relayer state on the rollup

```bash,test-ci,bashtestmd:compare-output
$ npm run hyperlane-warp-setup
Summary:
  Route ID: 0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a
  Token ID: token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf
```

Verify that the total supply of this token is initially 0:

```bash,test-ci,bashtestmd:compare-output
$ curl -Ss http://127.0.0.1:12346/modules/bank/tokens/token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf/total-supply
{"amount":"0","token_id":"token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf"}
```

Verify the warp configuration. Pay attention to these important fields:

* ISM configuration: `validators` should match the validator key address
* `remote_token_id` should match the configuration on ethtest
* `enrolled_destinations` should include the domain ID of ethtest
* `local_token_id` will be used later to verify transfers

```bash,test-ci,bashtestmd:compare-output
$ curl -Ss http://127.0.0.1:12346/modules/warp/state/warp-routes/items/0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a | jq
{
  "key": "0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a",
  "value": {
    "token_source": {
      "Synthetic": {
        "remote_token_id": "0x0000000000000000000000004ed7c70f96b99c776995fb64377f0d4ab3b0e1c1",
        "local_decimals": 18,
        "remote_decimals": 18,
        "local_token_id": "token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf"
      }
    },
    "admin": {
      "InsecureOwner": "0xd2c1be33a0bcd2007136afd8ed61cc7561ada747"
    },
    "ism": {
      "MessageIdMultisig": {
        "validators": [
          "0x70997970c51812dc3a010c7d01b50e0d17dc79c8"
        ],
        "threshold": 1
      }
    },
    "enrolled_destinations": [
      3133790210
    ],
```

You can also check the enrolled routers specifically:

```bash,test-ci,bashtestmd:compare-output,bashtestmd:exit-code=0
$ curl -Ss http://127.0.0.1:12346/modules/warp/route/0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a/routers
[{"domain":3133790210,"address":"0x0000000000000000000000004ed7c70f96b99c776995fb64377f0d4ab3b0e1c1"}]
```

### 3.1 Enroll Rollup Route on Anvil

```bash,test-ci,bashtestmd:compare-output,bashtestmd:exit-code=0
$ npm run hyperlane-enroll-router-on-ethtest
[✓] Enrolling remote router...
  Contract: 0x4ed7c70F96B99c776995fB64377f0d4aB3B0e1C1
  Domain: 5555
  Router: 0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a
```

Now the `remoteRouters` should contain an entry for the rollup:

```bash,test-ci,bashtestmd:compare-output
$ cd ../../ && make print-hyperlane-ethtest-warp
✅ Warp route config read successfully:

    ethtest:
      owner: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
      mailbox: "0x8A791620dd6260079BF849Dc5567aDC3F2FdC318"
      hook: "0x0000000000000000000000000000000000000000"
      interchainSecurityModule:
        address: "0x68B1D87F95878fE05B998F19b66F4baba5De1aed"
        type: testIsm
      remoteRouters:
        "5555":
          address: "0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a"
      name: Ether
      symbol: ETH
      decimals: 18
      isNft: false
      contractVersion: 9.0.6
      type: native
      allowedRebalancers: []
      allowedRebalancingBridges: {}
      proxyAdmin:
        address: "0x3Aa5ebB10DC797CAC828524e59A333d0A371443c"
        owner: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
      destinationGas:
        "5555": "0"
```

## 4. Make Inbound Transfers

Before initiating an inbound transfer to `0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747`, let's verify that its balance of bridged ETH is zero.

The bank endpoint will return a 404 error for non-existent balances:

```bash,test-ci,bashtestmd:exit-code=0
$ curl -Ss http://127.0.0.1:12346/modules/bank/tokens/token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf/balances/0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747
{"status":404,"message":"Balance '0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747' not found","details":{"id":"0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747"}}
```

Navigate to `examples/starter-js` and execute the inbound transfer:

```bash,test-ci,bashtestmd:exit-code=0
$ npm run hyperlane-inbound
Making inbound warp transfer...
  Contract:  0x4ed7c70F96B99c776995fB64377f0d4aB3B0e1C1
  Domain:    5555
  Router:    0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a
  Recipient: 0x000000000000000000000000D2C1bE33A0BcD2007136afD8Ed61CC7561aDa747
  Amount:    0.01 ETH
  Gas:       0.0 ETH
  Total:     0.01 ETH
Transaction sent: 0xda1dbcb27ad6d12a53f3137559628ac39f09cc578be740288deb7d7bca6d452b
```

Wait a moment for the transfer to process, then verify that the total supply of the synthetic token has increased along with the recipient's balance:

```bash
$ curl -Ss http://127.0.0.1:12346/modules/bank/tokens/token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf/total-supply
{"amount":"10000000000000000","token_id":"token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf"}
```

```bash,test-ci,bashtestmd:compare-output
$ sleep 30 && curl -Ss http://127.0.0.1:12346/modules/bank/tokens/token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf/balances/0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747
{"amount":"10000000000000000","token_id":"token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf"}
```

## 5. Make Outbound Transfers

Now we'll send funds back to `0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747` on ethtest.

First, verify that its balance is zero on ethtest:

```bash,test-ci,bashtestmd:compare-output
$ curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_getBalance","params":["0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747", "latest"],"id":1}' http://127.0.0.1:8545
{"jsonrpc":"2.0","id":1,"result":"0x0"}
```

Then initiate the outbound transfer:

```bash,test-ci,bashtestmd:compare-output
$ npm run hyperlane-outbound
[✓] Receipt result: successful
[✓] Mailbox/DispatchId (HyperlaneId): 0x873e0bfeb9251c268fbc483b4dae63a548360dd7594a1768aeb5a1532dd16e5c
[✓] Warp/TokenTransferredRemote:
    Route ID: 0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a
    To Domain: 3133790210
    Recipient: 0x000000000000000000000000d2c1be33a0bcd2007136afd8ed61cc7561ada747
    Amount (hex): 0x0000000000000000000000000000000000000000000000000000702d54e2f800
    Amount (decimal): 123340000000000
```

Wait for the transfer to process and verify that the balance on ethtest has increased:

```bash,test-ci,bashtestmd:compare-output
$ sleep 30 && curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_getBalance","params":["0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747", "latest"],"id":1}' http://127.0.0.1:8545
{"jsonrpc":"2.0","id":1,"result":"0x702d54e2f800"}
```

Confirm that the total supply of the synthetic token has decreased accordingly:

```bash,test-ci,bashtestmd:compare-output
$ curl -Ss http://127.0.0.1:12346/modules/bank/tokens/token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf/total-supply
{"amount":"9876660000000000","token_id":"token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf"}
```

## 6. Troubleshooting

### Validator Not Posting Checkpoints

**Check Configuration:**

1. Verify `CHAIN_ID` and `DOMAIN_ID` for both chains in all configuration files:
   * `constants.toml`
   * `integrations/hyperlane/configs/agent-config.json`
   * `integrations/hyperlane/configs/chains/ethtest/metadata.yaml`
   * `examples/starter-js/src/hyperlane/consts.ts`
2. Ensure mailbox addresses match across all configurations:
    ```
    grep -i 'mailbox' integrations/hyperlane/configs/chains/ethtest/addresses.yaml
    grep -i 'mailbox' integrations/hyperlane/configs/agent-config.json
    ```
3. Verify that warp routes are enrolled on both chains:
    - On ethtest: use `make print-hyperlane-ethtest-warp` and check that remoteRouters contains the correct DOMAIN_ID (note the route ID)
    - On rollup: `curl http://127.0.0.1:12346/modules/warp/route/<ROUTE_ID>/routers`
4. Ensure Anvil is configured for periodic block production

### Relayer Not Processing Messages

1. Verify that the validator is posting checkpoints: 
   `ls integrations/hyperlane/docker-data/validator-ethtest/signatures` should contain at least one file `0_with_id.json`
2. Check for errors or warnings in relayer logs: 
   `docker logs hyperlane-relayer-1 | grep -i -E 'error|warn'`
3. Verify that the relayer has sufficient balance: 
   `curl http://127.0.0.1:9091/metrics | grep hyperlane_wallet_balance`
4. Confirm the validator key is present in the ISM `MessageIdMultisig` configuration:
   `curl -Ss http://127.0.0.1:12346/modules/warp/state/warp-routes/items/<WARP_ROUTE_ID> | jq`
5. If you see "default IGP fee amount not set" error:
   Check: `curl http://127.0.0.1:12346/modules/interchain-gas-paymaster/state/relayer-default-gas/items/<RELAYER_SOV_ADDRESS>`
   Ensure the relayer address in the message is correct

### Additional Helpful Commands

**Check relayer balance:**

```
curl -Ss http://127.0.0.1:9091/metrics | grep 'hyperlane_wallet_balance'
# HELP hyperlane_wallet_balance Current native token balance for the wallet addresses in the `wallets` set
# TYPE hyperlane_wallet_balance gauge
hyperlane_wallet_balance{agent="relayer",chain="ethtest",hyperlane_baselib_version="0.1.0",token_address="none",token_name="Native",token_symbol="Native",wallet_address="3c44cdddb6a900fa2b585dd299e03d12fa4293bc",wallet_name="relayer"} 10000
```

**Verify no critical errors:**

```
curl -Ss http://127.0.0.1:9091/metrics | grep 'hyperlane_critical_error'
hyperlane_critical_error{agent="relayer",chain="ethtest",hyperlane_baselib_version="0.1.0"} 0
hyperlane_critical_error{agent="relayer",chain="sovstarter",hyperlane_baselib_version="0.1.0"} 0
```