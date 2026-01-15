import { useState } from "react";
import { usePrivy, useWallets } from "@privy-io/react-auth";
import { createStandardRollup, SovereignClient } from "@sovereign-sdk/web3";
import { PrivySigner } from "@sovereign-sdk/signers";
import ConnectButton from "./ConnectButton.tsx";
import "./App.css";
import type { RuntimeCall } from "./types";

type TxResponse = SovereignClient.SovereignSDK.Sequencer.TxCreateResponse;

const ROLLUP_URL = import.meta.env.VITE_ROLLUP_URL || "http://localhost:12346";

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
  const { ready, authenticated } = usePrivy();
  const { wallets } = useWallets();

  const [txResult, setTxResult] = useState<TxResponse | null>(null);
  const [txError, setTxError] = useState<string>("");
  const [isLoading, setIsLoading] = useState(false);
  const [isSuccess, setIsSuccess] = useState(false);
  const [txInput, setTxInput] = useState(JSON.stringify(DEFAULT_TX, null, 2));

  // Get the Privy-managed embedded wallet
  const embeddedWallet = wallets.find(
    (wallet) => wallet.walletClientType === "privy"
  );

  const handleSignAndSend = async () => {
    if (!embeddedWallet) {
      setTxError("No embedded wallet found. Please log in first.");
      return;
    }

    const provider = await embeddedWallet.getEthereumProvider();
    if (!provider) {
      setTxError("Provider unavailable on the embedded wallet.");
      return;
    }

    setIsLoading(true);
    setTxError("");
    setTxResult(null);
    setIsSuccess(false);

    try {
      // Parse and prepare transaction
      const txString = txInput.replace("<wallet_address>", embeddedWallet.address);
      const parsedTx: RuntimeCall = JSON.parse(txString);

      // Create rollup client and Privy signer
      const rollup = await createStandardRollup({ url: ROLLUP_URL });
      const signer = new PrivySigner(provider);

      // Sign and send
      const result = await rollup.call(parsedTx, { signer });
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

  if (!ready) {
    return <div className="container">Loading Privy...</div>;
  }

  return (
    <div className="container">
      <header>
        <h1>Privy Example</h1>
        <ConnectButton />
      </header>

      {authenticated ? (
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
            disabled={isLoading || !embeddedWallet}
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
          <p>Connect with Privy to send transactions</p>
        </section>
      )}
    </div>
  );
}
