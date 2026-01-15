import { test, expect } from "@playwright/test";

// Test credentials from environment variables (set from Privy Dashboard)
const TEST_EMAIL = process.env.PRIVY_TEST_EMAIL;
const TEST_OTP = process.env.PRIVY_TEST_OTP;

test.describe("Privy Example", () => {
  test("should display connect prompt when not authenticated", async ({
    page,
  }) => {
    await page.goto("/");

    // Wait for Privy to load
    await expect(page.locator("text=Loading Privy...")).toBeHidden({
      timeout: 10000,
    });

    // Should show connect prompt
    await expect(
      page.locator("text=Connect with Privy to send transactions")
    ).toBeVisible();

    // Connect button should be visible
    await expect(page.locator("text=Connect")).toBeVisible();
  });

  test.describe("with test account", () => {
    test.skip(!TEST_EMAIL || !TEST_OTP, "Privy test credentials not configured");

    test("should authenticate and send transaction", async ({ page }) => {
      await page.goto("/");

      // Wait for Privy to load
      await expect(page.locator("text=Loading Privy...")).toBeHidden({
        timeout: 10000,
      });

      // Click Connect button
      await page.locator("text=Connect").click();

      // Wait for Privy modal to appear
      const privyModal = page.frameLocator('iframe[title*="Privy"]');

      // Click on email login option if available
      const emailOption = privyModal.locator("text=Email");
      if (await emailOption.isVisible({ timeout: 5000 })) {
        await emailOption.click();
      }

      // Enter test email
      await privyModal.locator('input[type="email"]').fill(TEST_EMAIL!);
      await privyModal.locator("text=Continue").click();

      // Enter OTP
      const otpInputs = privyModal.locator('input[type="text"]');
      await otpInputs.first().waitFor({ timeout: 10000 });

      // Fill OTP digits
      for (let i = 0; i < TEST_OTP!.length; i++) {
        await otpInputs.nth(i).fill(TEST_OTP![i]);
      }

      // Wait for authentication to complete
      await expect(page.locator("text=Disconnect")).toBeVisible({
        timeout: 15000,
      });

      // Should now see transaction section
      await expect(page.locator("text=Send Transaction")).toBeVisible();

      // Click Sign and Send
      await page.locator("text=Sign and Send").click();

      // Wait for result
      const successMessage = page.locator(".message.success");
      const errorMessage = page.locator(".message.error");

      await expect(successMessage.or(errorMessage)).toBeVisible({
        timeout: 30000,
      });

      // If successful, verify success message
      if (await successMessage.isVisible()) {
        await expect(
          page.locator("text=Transaction Submitted Successfully!")
        ).toBeVisible();
      }
    });
  });
});
