# Bridging in tokens via Hyperlane

This tutorial demonstrates how configure bridging from EVM-like chain.

[Anvil](https://getfoundry.sh/anvil/reference/#anvil) is used for demonstration because it can be run locally.


## High Level overview

1. +Start docker compose with anvil and hyperlane agents
2. +Start the rollup
3. ~Register warp router and relayer on rollup
4. Enroll anvil warp route
5. Make transfers

## Start Anvil and Hyperlane Agents


```
# should be background, wait for validator announce message
cd integrations/hyperlane
make clean
docker compose -f docker-compose.hyp-evm.yml up 
```

### test the setup

print warp route configuration on ethest:

```bash
make print-hyperlane-ethtest-warp
```

print validator announcement message:

```bash
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


3. call relayer metrics endpoint and see details.

## Start the rollup

```
cargo run
```


## Enroll rollup route onto anvil

```
cast send 0x59b670e9fA9D0A427751Af201D676719a970857b \
    "enrollRemoteRouter(uint32,bytes32)" \
    5577 \
    0x1383103db8d7d56968f9b1c69a7cd1379ef0f2df41ed3a728489ae26a7cdf151 \
    --rpc-url http://localhost:8545 \
    --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

Now remote route should be shown:

```
$ make print-hyperlane-ethtest-warp
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

# Rest
```
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_getBalance","params":["0xD2C1bE33A0BcD2007136afD8Ed61CC7561aDa747", "latest"],"id":1}' \
  http://127.0.0.1:8545

```

