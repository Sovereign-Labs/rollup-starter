# Bridging in tokens via Hyperlane

This tutorial demonstrates how to configure bridging from an EVM-like chain.

[Anvil](https://getfoundry.sh/anvil/reference/#anvil) is used for demonstration because it can be run locally.


## High Level overview

1. +Start docker compose with anvil and hyperlane agents
2. +Start the rollup
3. +Register warp router and relayer on rollup 
4. +Enroll anvil warp route
5. ~Make transfers

## Start Anvil and Hyperlane Agents

Start the anvil and hyperlane and let it run.
Wait till you see message `Successfully announced validator` from validator container
Continue working in another console.

```bash,test-ci,bashtestmd:exit-code=0
$ make start-hyperlane-ethtest
```

### test the setup

Print warp route configuration on Ethtest. Notice, `remoteRouters` map is empty.

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

Print validator announcement message:

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


TODO: Call relayer metrics

## Start the rollup

Start the rollup and let it run.

```bash,test-ci,bashtestmd:long-running,bashtestmd:wait-until=rest_address
$ cargo run
```

Install dependencies if it wasn't done previously

```bash,test-ci,bashtestmd:exit-code=0
$ cd examples/starter-js && npm install
```

Setup warp route on the rollup side. This script will:

* Register a warp router and add a remote route on ethtest
* Configure a relayer state on rollup

```bash,test-ci,bashtestmd:compare-output,bashtestmd:exit-code=0
$ npm run hyperlane-warp-setup
Summary:
  Route ID: 0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a
  Token ID: token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf
```

Check the total supply of this token should be 0:

```bash,test-ci,bashtestmd:compare-output,bashtestmd:exit-code=0
$ curl -Ss http://127.0.0.1:12346/modules/bank/tokens/token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf/total-supply
{"amount":"0","token_id":"token_195zght0wmhcx9j462jtj9lypdua4xw07r6jnjfjsddsmzeh2wsfqrhddvf"}
```

Check the warp configuration. 
Note ism configuration and admin.
`remote_token_id` should match 

```bash,test-ci,bashtestmd:exit-code=0
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
          "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"
        ],
        "threshold": 1
      }
    },
    "enrolled_destinations": [
      3133790210
    ],
```

Or enrolled routers in particular:

```bash,test-ci,bashtestmd:compare-output,bashtestmd:exit-code=0
$ curl -Ss http://127.0.0.1:12346/modules/warp/route/0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a/routers
[{"domain":3133790210,"address":"0x0000000000000000000000004ed7c70f96b99c776995fb64377f0d4ab3b0e1c1"}]
```

**TODO: Check the relayer metrics**

```
curl http://
```

## Enroll rollup route onto anvil

```bash,test-ci,bashtestmd:compare-output,bashtestmd:exit-code=0
$ npm run hyperlane-enroll-router-on-ethtest
[✓] Enrolling remote router...
  Contract: 0x4ed7c70F96B99c776995fB64377f0d4aB3B0e1C1
  Domain: 5555
  Router: 0x9c081539d40ef7b02d359c5d694e006f0c1130097466cd22d062e07065c6987a
```

Now remoteRouters should have element:

```bash,test-ci,bashtestmd:exit-code=0
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


## Make transfers

### Inbound

Before start inbound transfer to `0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747` let's check that its balance of bridged ETH is zero.

Bank endpoint will return 404.

```bash,test-ci,bashtestmd:exit-code=0
$ curl -Ss http://127.0.0.1:12346/modules/bank/tokens/token_1s0242cee5dvg7vazxm98nu62axnrh4k60fsr5we7xl0cymzz4qfqtqgruc/balances/0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747
{"status":404,"message":"Balance '0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747' not found","details":{"id":"0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747"}}
```

```bash,test-ci,bashtestmd:exit-code=0
$ cd examples/starter-js && npm run hyperlane-inbound
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

**TODO**: How to wait till transfer is processed???. Bash script that pulls balance? Checking logs 

```bash,test-ci,bashtestmd:compare-output
$ sleep 30 && curl -Ss http://127.0.0.1:12346/modules/bank/tokens/token_1s0242cee5dvg7vazxm98nu62axnrh4k60fsr5we7xl0cymzz4qfqtqgruc/balances/0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747
{"status":404,"message":"Balance '0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747' not found","details":{"id":"0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747"}}
```

### Outbound



# Rest

Balance on the rollup



```
$ curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_getBalance","params":["0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747", "latest"],"id":1}' \
  http://127.0.0.1:8545
```


# Common problems

## Validator isn't posting checkpoints

#### Check configuration

1. Check CHAIN_ID and DOMAIN_ID for both chains in all necessary files:
2. Mailbox matches in all configurations:
    ```
    grep -i 'mailbox' integrations/hyperlane/configs/chains/ethtest/addresses.yaml
    grep -i 'mailbox' integrations/hyperlane/configs/agent-config.json
    ```
3. Check that warp routes are enrolled on both chains:
    - On ethtest use `make print-hyperlane-ethtest-warp` and check remoteRouters has correct DOMAIN_ID and note route id there
    - On rollup side: `curl http://127.0.0.1:12346/modules/warp/route/<ROUTE_ID_FROM_PREV_COMMAND/routers`
4. Make sure that anvil is configured for periodic block production

## Relayer does not process messages it saw


## 


curl -Ss http://127.0.0.1:9091/metrics | grep 'hyperlane_wallet_balance'
# HELP hyperlane_wallet_balance Current native token balance for the wallet addresses in the `wallets` set
# TYPE hyperlane_wallet_balance gauge
hyperlane_wallet_balance{agent="relayer",chain="ethtest",hyperlane_baselib_version="0.1.0",token_address="none",token_name="Native",token_symbol="Native",wallet_address="3c44cdddb6a900fa2b585dd299e03d12fa4293bc",wallet_name="relayer"} 10000


hyperlane_critical_error{agent="relayer",chain="ethtest",hyperlane_baselib_version="0.1.0"} 0
hyperlane_critical_error{agent="relayer",chain="sovstarter",hyperlane_baselib_version="0.1.0"} 1