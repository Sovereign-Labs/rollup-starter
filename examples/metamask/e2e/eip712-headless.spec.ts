/**
 * Headless E2E test for EIP-712 signing
 *
 * This test injects a mock wallet provider instead of using MetaMask,
 * making it stable and independent of MetaMask UI changes.
 *
 * The mock provider uses Playwright's exposeFunction to call viem's
 * signTypedData from Node.js when the app requests a signature.
 */
import { test, expect } from "@playwright/test";
import { privateKeyToAccount, signTypedData } from "viem/accounts";

// Hardhat's default test account #0
const TEST_PRIVATE_KEY =
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const TEST_ACCOUNT = privateKeyToAccount(TEST_PRIVATE_KEY);

test.describe("EIP-712 Headless Wallet Tests", () => {
  test.beforeEach(async ({ page }) => {
    // Expose signing function to the browser
    // This lets the mock provider call viem's signTypedData directly
    await page.exposeFunction("__signTypedData", async (typedDataJson: string) => {
      const typedData = JSON.parse(typedDataJson);

      // Remove EIP712Domain from types (viem adds it automatically)
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { EIP712Domain: _, ...typesWithoutDomain } = typedData.types;

      // Convert chainId to number
      const rawChainId = typedData.domain.chainId;
      const chainId = typeof rawChainId === "string"
        ? (rawChainId.startsWith("0x") ? parseInt(rawChainId, 16) : parseInt(rawChainId, 10))
        : rawChainId;

      return signTypedData({
        privateKey: TEST_PRIVATE_KEY,
        domain: { ...typedData.domain, chainId },
        types: typesWithoutDomain,
        primaryType: typedData.primaryType,
        message: typedData.message,
      });
    });

    // Inject mock provider before page loads
    await page.addInitScript(
      ({ address }) => {
        const mockProvider = {
          isMetaMask: true,
          _events: {} as Record<string, Array<(...args: unknown[]) => void>>,

          request: async ({ method, params }: { method: string; params?: unknown[] }) => {
            switch (method) {
              case "eth_requestAccounts":
              case "eth_accounts":
                return [address];

              case "eth_chainId":
                return "0x1a0d"; // 6669

              case "wallet_switchEthereumChain": {
                const error = new Error("Network not found");
                (error as Error & { code: number }).code = 4902;
                throw error;
              }

              case "wallet_addEthereumChain":
                return null;

              case "eth_signTypedData_v4": {
                const [, typedDataJson] = params as [string, string];
                // Call the exposed signing function directly
                return (window as Window & { __signTypedData: (json: string) => Promise<string> })
                  .__signTypedData(typedDataJson);
              }

              default:
                throw new Error(`Unsupported method: ${method}`);
            }
          },

          on: (event: string, callback: (...args: unknown[]) => void): void => {
            if (!mockProvider._events[event]) mockProvider._events[event] = [];
            mockProvider._events[event].push(callback);
          },

          removeListener: (event: string, callback: (...args: unknown[]) => void): void => {
            if (mockProvider._events[event]) {
              mockProvider._events[event] = mockProvider._events[event].filter((cb) => cb !== callback);
            }
          },

          emit: (event: string, ...args: unknown[]): void => {
            mockProvider._events[event]?.forEach((cb) => cb(...args));
          },
        };

        Object.defineProperty(window, "ethereum", {
          value: mockProvider,
          writable: false,
          configurable: true,
        });
      },
      { address: TEST_ACCOUNT.address }
    );
  });

  test("connect wallet with mock provider", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    await page.screenshot({ path: "test-results/headless-1-initial.png" });

    await expect(page.locator("text=Connected:")).toBeVisible({ timeout: 5000 });

    await page.screenshot({ path: "test-results/headless-2-connected.png" });

    await expect(page.locator("text=0xf39F")).toBeVisible();
  });

  test("add network with mock provider", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    await expect(page.locator("text=Connected:")).toBeVisible({ timeout: 5000 });

    await page.click("text=Add Network to MetaMask");

    // Wait for the click to be processed (button should still be visible after network add)
    await expect(page.locator("text=Add Network to MetaMask")).toBeVisible();

    await page.screenshot({ path: "test-results/headless-3-network-added.png" });
  });

  test("EIP-712 sign and send transaction", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    await expect(page.locator("text=Connected:")).toBeVisible({ timeout: 5000 });

    await page.click("text=Sign and Send");

    const successLocator = page.locator("text=Transaction Submitted Successfully");
    const errorLocator = page.locator(".message.error");

    await expect(successLocator.or(errorLocator)).toBeVisible({ timeout: 30000 });

    await page.screenshot({ path: "test-results/headless-4-result.png" });

    if (await errorLocator.isVisible()) {
      const errorText = await page.locator(".message.error pre").textContent();
      throw new Error(`Transaction failed: ${errorText}`);
    }

    await expect(successLocator).toBeVisible();
  });
});
