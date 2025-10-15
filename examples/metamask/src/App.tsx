import { useState, useEffect } from "react";
import { createStandardRollup, SovereignClient } from "@sovereign-sdk/web3";
import { Eip712Signer } from "@sovereign-sdk/signers";
import ConnectButton from "./ConnectButton.tsx";
import "./App.css";
import type { RuntimeCall } from "./types";
import type { MetaMaskInpageProvider } from "@metamask/providers";

declare global {
  interface Window {
    ethereum?: MetaMaskInpageProvider;
  }
}

type TxResponse = SovereignClient.SovereignSDK.Sequencer.TxCreateResponse.Data;

// Chain configuration
const ROLLUP_URL = import.meta.env.VITE_ROLLUP_URL || "http://localhost:12346";

export default function App() {
  const [account, setAccount] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);
  const [txResult, setTxResult] = useState<TxResponse | null>(null);
  const [txError, setTxError] = useState<string>("");
  const [isLoading, setIsLoading] = useState(false);
  const [txInput, setTxInput] = useState<string>(
    JSON.stringify(
      {
        bank: {
          create_token: {
            token_name: "Yo Yo Token",
            token_decimals: 8,
            initial_balance: 1000000000,
            mint_to_address: "<wallet_address>",
            admins: [],
            supply_cap: 100000000000,
          },
        },
      },
      null,
      2,
    ),
  );
  const [jsonError, setJsonError] = useState<string>("");

  const connect = async () => {
    if (!window.ethereum) {
      alert("Please install MetaMask!");
      return;
    }

    setIsConnecting(true);
    try {
      const accounts = await window.ethereum.request({
        method: "eth_requestAccounts",
      }) as string[];

      if (accounts && accounts.length > 0) {
        setAccount(accounts[0]);
      }
    } catch (error) {
      console.error("Error connecting to MetaMask:", error);
    } finally {
      setIsConnecting(false);
    }
  };

  // Listen for account changes
  useEffect(() => {
    if (!window.ethereum) return;

    const checkAccount = async () => {
      try {
        const accounts = await window.ethereum.request({
          method: "eth_accounts",
        }) as string[];

        if (accounts && accounts.length > 0) {
          setAccount(accounts[0]);
        } else {
          setAccount(null);
        }
      } catch (error) {
        console.error("Error checking accounts:", error);
      }
    };

    // Check initial account
    checkAccount();

    // Listen for account changes
    const handleAccountsChanged = (accounts: unknown) => {
      const accountsList = accounts as string[];
      if (accountsList.length > 0) {
        setAccount(accountsList[0]);
      } else {
        setAccount(null);
      }
    };

    window.ethereum.on("accountsChanged", handleAccountsChanged);

    return () => {
      window.ethereum?.removeListener("accountsChanged", handleAccountsChanged);
    };
  }, []);

  const handleJsonFormat = () => {
    try {
      const parsed = JSON.parse(txInput);
      setTxInput(JSON.stringify(parsed, null, 2));
      setJsonError("");
    } catch {
      setJsonError("Invalid JSON format");
    }
  };

  const handleSignAndSendTransaction = async () => {
    if (!window.ethereum) {
      setTxError("MetaMask is not installed");
      setTxResult(null);
      return;
    }

    if (!account) {
      setTxError("Please connect to MetaMask first");
      setTxResult(null);
      return;
    }

    setIsLoading(true);
    setTxError("");
    setTxResult(null);
    console.log("Preparing transaction…");

    try {
      // Parse transaction input
      let parsedTx: RuntimeCall;
      try {
        const txData = JSON.parse(txInput);
        // Replace placeholder with actual wallet address
        const txString = JSON.stringify(txData).replace(
          "<wallet_address>",
          account,
        );
        parsedTx = JSON.parse(txString);
      } catch {
        setTxError("Invalid JSON format. Please check your transaction data.");
        setIsLoading(false);
        return;
      }

      // Instantiate Sovereign rollup client
      let rollup;
      try {
        rollup = await createStandardRollup({
          url: ROLLUP_URL,
          txSubmissionEndpoint: "/sequencer/eip712_tx",
        });
      } catch (rollupError) {
        console.error("Error creating rollup:", rollupError);
        setTxError(`Failed to create rollup client: ${rollupError}`);
        setIsLoading(false);
        return;
      }

      // Get the serializer which contains the schema
      const serializer = await rollup.serializer();
      const schemaJson = serializer.schema;

      // Create the EIP-712 signer with MetaMask provider, schema, and address
      const signer = new Eip712Signer(window.ethereum, schemaJson, account);

      // Sign + send tx
      console.log("Sending parsedTx: ", parsedTx);
      const result = await rollup.call(parsedTx, { signer });
      setTxResult(result.response?.data);
    } catch (err) {
      console.error(err);

      // Store the full error for display
      if (err instanceof Error) {
        setTxError(err.message);
      } else {
        setTxError(String(err));
      }
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="container">
      <header>
        <h1>MetaMask × Sovereign Demo</h1>
        <ConnectButton
          account={account}
          isConnecting={isConnecting}
          isMetaMaskInstalled={!!window.ethereum}
          onConnect={connect}
        />
      </header>

      {account && (
        <section className="transaction-section">
          <h3>Send Transaction</h3>

          <div style={{ marginBottom: "20px" }}>
            <label
              htmlFor="tx-input"
              style={{ display: "block", marginBottom: "8px" }}
            >
              Transaction Data (JSON):
            </label>
            <textarea
              id="tx-input"
              value={txInput}
              onChange={(e) => {
                setTxInput(e.target.value);
                setJsonError("");
              }}
              onBlur={handleJsonFormat}
              placeholder="Enter transaction JSON here..."
              style={{
                width: "100%",
                minHeight: "200px",
                padding: "10px",
                fontSize: "14px",
                fontFamily: "monospace",
                border: jsonError ? "1px solid #dc3545" : "1px solid #ccc",
                borderRadius: "4px",
                backgroundColor: "#f5f5f5",
                color: "#333",
                lineHeight: "1.5",
              }}
            />
            {jsonError && (
              <div
                style={{
                  color: "#dc3545",
                  fontSize: "14px",
                  marginTop: "5px",
                }}
              >
                {jsonError}
              </div>
            )}
          </div>

          <button
            onClick={handleSignAndSendTransaction}
            disabled={isLoading || !account}
            className="primary-button"
          >
            {isLoading ? "Processing…" : "Sign and Send Transaction"}
          </button>

          {txError && (
            <div className="status-message error" style={{ textAlign: "left" }}>
              <div style={{ marginBottom: "10px" }}>
                <strong>❌ Transaction failed</strong>
              </div>

              {(() => {
                // Try to parse as JSON for better formatting
                const errorStr = txError;

                // Check if it's a status code followed by JSON
                const match = errorStr.match(/^(\d{3})\s+(\{.*\})$/s);
                if (match) {
                  const statusCode = match[1];
                  try {
                    const errorData = JSON.parse(match[2]);
                    return (
                      <>
                        <div style={{ marginBottom: "5px" }}>
                          <strong>Status Code:</strong> {statusCode}
                        </div>
                        <div>
                          <strong>Error Details:</strong>
                          <pre
                            style={{
                              backgroundColor: "#fff5f5",
                              border: "1px solid #ffdddd",
                              borderRadius: "4px",
                              padding: "10px",
                              marginTop: "5px",
                              fontSize: "12px",
                              overflow: "auto",
                              maxHeight: "300px",
                            }}
                          >
                            {JSON.stringify(errorData, null, 2)}
                          </pre>
                        </div>
                      </>
                    );
                  } catch {
                    // If JSON parsing fails, show as is
                  }
                }

                // For non-JSON errors, try to parse as JSON anyway
                try {
                  const parsed = JSON.parse(errorStr);
                  return (
                    <pre
                      style={{
                        backgroundColor: "#fff5f5",
                        border: "1px solid #ffdddd",
                        borderRadius: "4px",
                        padding: "10px",
                        fontSize: "12px",
                        overflow: "auto",
                        maxHeight: "300px",
                      }}
                    >
                      {JSON.stringify(parsed, null, 2)}
                    </pre>
                  );
                } catch {
                  // Show as plain text
                  return (
                    <div
                      style={{
                        backgroundColor: "#fff5f5",
                        border: "1px solid #ffdddd",
                        borderRadius: "4px",
                        padding: "10px",
                        fontSize: "12px",
                        overflow: "auto",
                        maxHeight: "300px",
                        whiteSpace: "pre-wrap",
                        wordBreak: "break-word",
                      }}
                    >
                      {errorStr}
                    </div>
                  );
                }
              })()}
            </div>
          )}

          {txResult && (
            <div className="status-message" style={{ textAlign: "left" }}>
              <div style={{ marginBottom: "10px" }}>
                <strong>✅ Transaction sent successfully!</strong>
              </div>

              <div style={{ marginBottom: "5px" }}>
                <strong>Transaction Hash:</strong>
                <div
                  style={{
                    fontFamily: "monospace",
                    fontSize: "12px",
                    wordBreak: "break-all",
                    marginTop: "5px",
                  }}
                >
                  {txResult.id || "N/A"}
                </div>
              </div>

              <div style={{ marginBottom: "5px" }}>
                <strong>Status:</strong> {txResult.status || "N/A"}
              </div>

              {txResult.receipt && (
                <div style={{ marginBottom: "5px" }}>
                  <strong>Result:</strong> {txResult.receipt.result || "N/A"}
                  {(txResult.receipt as any).data?.gas_used && (
                    <span>
                      {" "}
                      | <strong>Gas:</strong>{" "}
                      {(txResult.receipt as any).data.gas_used[0]}
                    </span>
                  )}
                </div>
              )}

              {txResult.events && txResult.events.length > 0 && (
                <div style={{ marginTop: "15px" }}>
                  <strong>Events:</strong>
                  <pre
                    style={{
                      backgroundColor: "#f5f5f5",
                      border: "1px solid #ddd",
                      borderRadius: "4px",
                      padding: "10px",
                      marginTop: "5px",
                      fontSize: "12px",
                      overflow: "auto",
                      maxHeight: "300px",
                    }}
                  >
                    {JSON.stringify(txResult.events, null, 2)}
                  </pre>
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {!account && (
        <section style={{ textAlign: "center", marginTop: "50px" }}>
          <p>Please connect your MetaMask wallet to send transactions</p>
        </section>
      )}
    </div>
  );
}
