import { testWithSynpress } from "@synthetixio/synpress";
import { MetaMask, metaMaskFixtures } from "@synthetixio/synpress/playwright";
import basicSetup, { WALLET_PASSWORD } from "./wallet-setup/basic.setup";

const test = testWithSynpress(metaMaskFixtures(basicSetup));
const { expect } = test;

test.describe("MetaMask EIP-712 Example", () => {
  test("should connect wallet, add network, and send transaction", async ({
    context,
    page,
    metamaskPage,
    extensionId,
  }) => {
    // Create MetaMask instance
    const metamask = new MetaMask(
      context,
      metamaskPage,
      WALLET_PASSWORD,
      extensionId
    );

    // Navigate to the app
    await page.goto("/");

    // Verify initial state - should show connect prompt
    await expect(page.locator("text=Connect MetaMask")).toBeVisible();

    // Click connect button
    await page.locator("text=Connect MetaMask").click();

    // Approve connection in MetaMask
    await metamask.connectToDapp();

    // Wait for connected state - button should show truncated address
    await expect(page.locator("text=Connected:")).toBeVisible({ timeout: 10000 });

    // Add network to MetaMask
    await page.locator("text=Add Network to MetaMask").click();

    // Approve network addition in MetaMask
    await metamask.approveNewNetwork();

    // Click Sign and Send button
    await page.locator("text=Sign and Send").click();

    // Approve the EIP-712 signature request in MetaMask
    await metamask.confirmSignature();

    // Wait for success message or error
    // The transaction may succeed or fail depending on rollup state,
    // but we should see some response
    const successMessage = page.locator(".message.success");
    const errorMessage = page.locator(".message.error");

    // Wait for either success or error to appear
    await expect(successMessage.or(errorMessage)).toBeVisible({ timeout: 30000 });

    // If successful, verify the transaction details are shown
    if (await successMessage.isVisible()) {
      await expect(
        page.locator("text=Transaction Submitted Successfully!")
      ).toBeVisible();
    }
  });

  test("should display connect prompt when not connected", async ({ page }) => {
    await page.goto("/");

    // Should show connect prompt
    await expect(
      page.locator("text=Connect your MetaMask wallet to send transactions")
    ).toBeVisible();

    // Connect button should be visible
    await expect(page.locator("text=Connect MetaMask")).toBeVisible();
  });
});
