## JS Example

This package demonstrates how to use TypeScript/JS to interact with your rollup.

### Prerequisites

1.  Install nodejs and a package manager. All commands in this example assume `npm`, but feel free to substitute `yarn` instead. `bun` is not currently supported in this example but will work if you want to use it for your project.
2.  Install dependencies `npm install`
3.  In root of the repo execute `cargo run`
4.  In this directory run `npm run start`

Additional examples:
- `npm run hyperlane-outbound`

### Authorized Match Example

1. In the repo root, run `cargo run` and leave it running.
2. In Remix, compile your Solidity authorizer. (See example below for format)
3. Open `Compilation Details`.
4. Under `Deployed Bytecode`, copy the `object` field.
5. In `examples/starter-js`, run:

```bash
MAKER_AUTHORIZER_RUNTIME_BYTECODE=<paste_deployed_bytecode_object> \
TAKER_AUTHORIZER_RUNTIME_BYTECODE=<paste_deployed_bytecode_object> \
npm run match
```

The script generates `maker`, `taker`, and `admin` accounts, funds them, installs the maker/taker authorizers, and submits a match.

```solidity
// SPDX-License-Identifier: MIT
  pragma solidity ^0.8.24;

  contract SkeletonAuthorizer {
      error MatchRejected(string reason);

      struct MatchInput {
          uint64 id;
          uint64 price;
          uint64 quantity;
          bytes longAccount;
          bytes shortAccount;
          uint64 timestamp;
          bytes longAccountCalldata;
          bytes shortAccountCalldata;
      }

      // The rollup passes raw abi.encode(MatchInput), not a function selector.
      fallback(bytes calldata rawInput) external returns (bytes memory) {
          MatchInput memory m = abi.decode(rawInput, (MatchInput));
          _authorize(m);
          return "";
      }

      function _authorize(MatchInput memory m) internal pure {
          if (m.quantity == 0) revert MatchRejected("quantity is zero");
          if (m.price == 0) revert MatchRejected("price is zero");
          if (m.id != 4) revert MatchRejected("Only asset 4 is allowed");
          if ((m.quantity * m.price) > 1000) revert MatchRejected("Max budget is 1000 tokens");

          // Example policy hooks.
          if (m.longAccountCalldata.length > 0 && m.longAccountCalldata[0] == 0x00) {
              revert MatchRejected("long side rejected");
          }

          if (m.shortAccountCalldata.length > 0 && m.shortAccountCalldata[0] == 0x00) {
              revert MatchRejected("short side rejected");
          }

          // Accept by returning normally.
      }
  }
```
