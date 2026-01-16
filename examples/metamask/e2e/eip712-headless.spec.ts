/**
 * Headless E2E test for EIP-712 signing
 * This test injects a mock wallet provider instead of using MetaMask,
 * making it stable and independent of MetaMask UI changes.
 * Uses viem for EIP-712 signing.
 */
import { test, expect } from "@playwright/test";
import { privateKeyToAccount } from "viem/accounts";

// Hardhat's default test account #0
const TEST_PRIVATE_KEY =
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const TEST_ACCOUNT = privateKeyToAccount(TEST_PRIVATE_KEY);

test.describe("EIP-712 Headless Wallet Tests", () => {
  test.beforeEach(async ({ page }) => {
    // Inject mock provider before page loads
    await page.addInitScript(
      ({ address, privateKey }) => {
        // Simple secp256k1 signing implementation for browser
        // We'll use the browser's crypto API to sign EIP-712 messages

        const mockProvider = {
          isMetaMask: true,
          _events: {} as Record<string, Array<(...args: unknown[]) => void>>,

          request: async ({
            method,
            params,
          }: {
            method: string;
            params?: unknown[];
          }) => {
            switch (method) {
              case "eth_requestAccounts":
              case "eth_accounts":
                return [address];

              case "eth_chainId":
                return "0x1a0d"; // 6669

              case "wallet_switchEthereumChain":
                // Simulate network not found error to trigger wallet_addEthereumChain
                const switchError = new Error("Network not found");
                (switchError as Error & { code: number }).code = 4902;
                throw switchError;

              case "wallet_addEthereumChain":
                return null; // Success

              case "eth_signTypedData_v4": {
                // Parse the typed data
                const [, typedDataJson] = params as [string, string];
                const typedData = JSON.parse(typedDataJson);

                // Use viem's signTypedData in the test context
                // Since we can't use viem in the browser directly,
                // we'll store the request and sign it from the test
                (window as Window & { __pendingSignRequest?: unknown }).__pendingSignRequest = typedData;

                // Wait for the signature to be provided by the test
                return new Promise((resolve, reject) => {
                  const checkSignature = () => {
                    const sig = (window as Window & { __signatureResult?: string }).__signatureResult;
                    if (sig) {
                      (window as Window & { __signatureResult?: string }).__signatureResult = undefined;
                      resolve(sig);
                    } else if ((window as Window & { __signatureError?: string }).__signatureError) {
                      const err = (window as Window & { __signatureError?: string }).__signatureError;
                      (window as Window & { __signatureError?: string }).__signatureError = undefined;
                      reject(new Error(err));
                    } else {
                      setTimeout(checkSignature, 50);
                    }
                  };
                  checkSignature();
                });
              }

              default:
                throw new Error(`Unsupported method: ${method}`);
            }
          },

          on: (
            event: string,
            callback: (...args: unknown[]) => void
          ): void => {
            if (!mockProvider._events[event]) {
              mockProvider._events[event] = [];
            }
            mockProvider._events[event].push(callback);
          },

          removeListener: (
            event: string,
            callback: (...args: unknown[]) => void
          ): void => {
            if (mockProvider._events[event]) {
              mockProvider._events[event] = mockProvider._events[event].filter(
                (cb) => cb !== callback
              );
            }
          },

          emit: (event: string, ...args: unknown[]): void => {
            if (mockProvider._events[event]) {
              mockProvider._events[event].forEach((cb) => cb(...args));
            }
          },
        };

        // Inject as window.ethereum
        Object.defineProperty(window, "ethereum", {
          value: mockProvider,
          writable: false,
          configurable: true,
        });
      },
      { address: TEST_ACCOUNT.address, privateKey: TEST_PRIVATE_KEY }
    );
  });

  test("connect wallet with mock provider", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Screenshot initial state
    await page.screenshot({ path: "test-results/headless-1-initial.png" });

    // Mock provider auto-connects via eth_accounts on page load
    // Wait for connected state (should be immediate)
    await expect(page.locator("text=Connected:")).toBeVisible({ timeout: 5000 });

    // Screenshot connected state
    await page.screenshot({ path: "test-results/headless-2-connected.png" });

    // Verify it shows our test account address
    await expect(page.locator("text=0xf39F")).toBeVisible();
  });

  test("add network with mock provider", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Wait for auto-connection
    await expect(page.locator("text=Connected:")).toBeVisible({ timeout: 5000 });

    // Add network - should auto-approve
    await page.click("text=Add Network to MetaMask");

    // Give it a moment to process
    await page.waitForTimeout(500);

    // Screenshot after adding network
    await page.screenshot({ path: "test-results/headless-3-network-added.png" });
  });

  test("EIP-712 sign and send transaction", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Wait for auto-connection
    await expect(page.locator("text=Connected:")).toBeVisible({ timeout: 5000 });

    // Modify the token name to be unique (avoid "token already exists" errors)
    const uniqueTokenName = `Test Token ${Date.now()}`;
    await page.locator("#tx-input").fill(JSON.stringify({
      bank: {
        create_token: {
          token_name: uniqueTokenName,
          token_decimals: 8,
          initial_balance: 1000000000,
          mint_to_address: TEST_ACCOUNT.address,
          admins: [],
          supply_cap: 100000000000,
        },
      },
    }, null, 2));

    // Click Sign and Send
    await page.click("text=Sign and Send");

    // Wait for the signing request
    await page.waitForFunction(
      () => (window as Window & { __pendingSignRequest?: unknown }).__pendingSignRequest !== undefined,
      { timeout: 10000 }
    );

    // Get the typed data from the page
    const typedData = await page.evaluate(
      () => (window as Window & { __pendingSignRequest?: unknown }).__pendingSignRequest
    ) as {
      domain: { name: string; chainId: string; salt: string };
      types: Record<string, Array<{name: string; type: string}>>;
      primaryType: string;
      message: object
    };

    const { signTypedData } = await import("viem/accounts");

    // Remove EIP712Domain from types as viem adds it automatically
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { EIP712Domain: _, ...typesWithoutDomain } = typedData.types;

    // Convert chainId from hex string to number (required for correct EIP-712 hashing)
    const domain = {
      ...typedData.domain,
      chainId: parseInt(typedData.domain.chainId, 16)
    };

    const signature = await signTypedData({
      privateKey: TEST_PRIVATE_KEY,
      domain,
      types: typesWithoutDomain,
      primaryType: typedData.primaryType as "UnsignedTransaction",
      message: typedData.message as Record<string, unknown>
    });

    // Provide the signature back to the page
    await page.evaluate(
      (sig) => {
        (window as Window & { __signatureResult?: string }).__signatureResult = sig;
      },
      signature
    );

    // Wait for result - either success or error
    const successLocator = page.locator("text=Transaction Submitted Successfully");
    const errorLocator = page.locator(".message.error");

    await expect(successLocator.or(errorLocator)).toBeVisible({ timeout: 30000 });

    // Screenshot the result
    await page.screenshot({ path: "test-results/headless-4-result.png" });

    // Check if we got an error and fail with details
    if (await errorLocator.isVisible()) {
      const errorText = await page.locator(".message.error pre").textContent();
      throw new Error(`Transaction failed: ${errorText}`);
    }

    // Verify success
    await expect(successLocator).toBeVisible();
  });
});
