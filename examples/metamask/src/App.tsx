import { useState, useEffect } from "react";
import { createStandardRollup, SovereignClient } from "@sovereign-sdk/web3";
import { Eip712Signer } from "@sovereign-sdk/signers/wasm";
import ConnectButton from "./ConnectButton.tsx";
import "./App.css";
import type { RuntimeCall } from "./types";
import type { MetaMaskInpageProvider } from "@metamask/providers";

declare global {
  interface Window {
    ethereum?: MetaMaskInpageProvider;
  }
}

type TxResponse = SovereignClient.SovereignSDK.Sequencer.TxCreateResponse;

const ROLLUP_URL = import.meta.env.VITE_ROLLUP_URL || "http://localhost:12346";
const CHAIN_ID = import.meta.env.VITE_CHAIN_ID || "6669";
const CHAIN_NAME = import.meta.env.VITE_CHAIN_NAME || "Sovereign Rollup";

const DEFAULT_TX = {
  bank: {
    create_token: {
      token_name: "My Token",
      token_decimals: 8,
      initial_balance: 1000000000,
      mint_to_address: "<wallet_address>",
      admins: [],
      supply_cap: 100000000000,
    },
  },
};

export default function App() {
  const [account, setAccount] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);
  const [txResult, setTxResult] = useState<TxResponse | null>(null);
  const [txError, setTxError] = useState<string>("");
  const [isLoading, setIsLoading] = useState(false);
  const [isSuccess, setIsSuccess] = useState(false);
  const [txInput, setTxInput] = useState(JSON.stringify(DEFAULT_TX, null, 2));

  // Connect to MetaMask
  const connect = async () => {
    if (!window.ethereum) {
      alert("Please install MetaMask!");
      return;
    }

    setIsConnecting(true);
    try {
      const accounts = (await window.ethereum.request({
        method: "eth_requestAccounts",
      })) as string[];
      if (accounts?.[0]) setAccount(accounts[0]);
    } catch (error) {
      console.error("Connection error:", error);
    } finally {
      setIsConnecting(false);
    }
  };

  // Add rollup network to MetaMask
  const addNetwork = async () => {
    if (!window.ethereum) {
      alert("Please install MetaMask!");
      return;
    }

    const chainIdHex = `0x${parseInt(CHAIN_ID).toString(16)}`;

    try {
      // First try to switch to the network (if it already exists)
      await window.ethereum.request({
        method: "wallet_switchEthereumChain",
        params: [{ chainId: chainIdHex }],
      });
    } catch (switchError: unknown) {
      // Network doesn't exist, add it
      if ((switchError as { code?: number })?.code === 4902) {
        try {
          await window.ethereum.request({
            method: "wallet_addEthereumChain",
            params: [
              {
                chainId: chainIdHex,
                chainName: CHAIN_NAME,
                rpcUrls: [ROLLUP_URL],
                nativeCurrency: {
                  name: "SOV",
                  symbol: "SOV",
                  decimals: 18,
                },
              },
            ],
          });
        } catch (addError) {
          console.error("Failed to add network:", addError);
        }
      } else {
        console.error("Failed to switch network:", switchError);
      }
    }
  };

  // Listen for account changes
  useEffect(() => {
    if (!window.ethereum) return;

    const handleAccounts = (accounts: unknown) => {
      const list = accounts as string[];
      setAccount(list[0] || null);
    };

    // Check initial account
    window.ethereum.request({ method: "eth_accounts" }).then(handleAccounts);
    window.ethereum.on("accountsChanged", handleAccounts);

    return () => {
      window.ethereum?.removeListener("accountsChanged", handleAccounts);
    };
  }, []);

  // Sign and send transaction
  const handleSignAndSend = async () => {
    if (!window.ethereum || !account) return;

    setIsLoading(true);
    setTxError("");
    setTxResult(null);
    setIsSuccess(false);

    try {
      // Parse and prepare transaction
      const txString = txInput.replace("<wallet_address>", account);
      const parsedTx: RuntimeCall = JSON.parse(txString);

      // Create rollup client and EIP-712 signer
      const rollup = await createStandardRollup({ url: ROLLUP_URL });
      const serializer = await rollup.serializer();
      const signer = new Eip712Signer(window.ethereum, serializer.schema, account);

      // Sign and send (EIP-712 requires the dedicated endpoint)
      const result = await rollup.call(parsedTx, { signer }, { path: "/sequencer/eip712_tx" });
      setTxResult(result.response ?? null);
      setIsSuccess(true);
    } catch (err: unknown) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const e = err as any;
      const details = e.error?.details || e.details;
      setTxError(e.message + (details ? "\n\n" + JSON.stringify(details, null, 2) : ""));
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="container">
      <header>
        <h1>MetaMask EIP-712 Example</h1>
        <ConnectButton
          account={account}
          isConnecting={isConnecting}
          isMetaMaskInstalled={!!window.ethereum}
          onConnect={connect}
        />
      </header>

      <div style={{ marginBottom: "1rem" }}>
        <button onClick={addNetwork} className="secondary-button">
          Add Network to MetaMask
        </button>
      </div>

      {account ? (
        <section className="transaction-section">
          <h3>Send Transaction</h3>

          <label htmlFor="tx-input">Transaction Data (JSON):</label>
          <textarea
            id="tx-input"
            value={txInput}
            onChange={(e) => setTxInput(e.target.value)}
            placeholder="Enter transaction JSON..."
          />

          <button
            onClick={handleSignAndSend}
            disabled={isLoading}
            className="primary-button"
          >
            {isLoading ? "Processing..." : "Sign and Send"}
          </button>

          {txError && (
            <div className="message error">
              <strong>Error:</strong>
              <pre>{txError}</pre>
            </div>
          )}

          {isSuccess && (
            <div className="message success">
              <strong>Transaction Submitted Successfully!</strong>
              {txResult ? (
                <>
                  <div>
                    <strong>Hash:</strong>{" "}
                    <code>{txResult.id || "N/A"}</code>
                  </div>
                  <div>
                    <strong>Status:</strong> {txResult.status || "N/A"}
                  </div>
                  {txResult.events && txResult.events.length > 0 && (
                    <div>
                      <strong>Events:</strong>
                      <pre>{JSON.stringify(txResult.events, null, 2)}</pre>
                    </div>
                  )}
                </>
              ) : (
                <div>Transaction was sent to the rollup.</div>
              )}
            </div>
          )}
        </section>
      ) : (
        <section className="connect-prompt">
          <p>Connect your MetaMask wallet to send transactions</p>
        </section>
      )}
    </div>
  );
}
