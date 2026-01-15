import { test, expect } from "@playwright/test";

// Test credentials from environment variables (set from Privy Dashboard)
// For test accounts to work:
// 1. Enable test accounts in Privy Dashboard (User management → Authentication → Advanced)
// 2. Create a test user with a static OTP
// 3. Set PRIVY_TEST_EMAIL and PRIVY_TEST_OTP environment variables
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
    await expect(page.getByRole("button", { name: "Connect" })).toBeVisible();
  });

  test("should open Privy login modal", async ({ page }) => {
    await page.goto("/");

    // Wait for Privy to load
    await expect(page.locator("text=Loading Privy...")).toBeHidden({
      timeout: 10000,
    });

    // Click Connect button
    await page.getByRole("button", { name: "Connect" }).click();

    // Verify Privy modal opens - check for email input which appears in the modal
    await expect(page.locator('input[type="email"]')).toBeVisible({ timeout: 10000 });
  });

  test.describe("with test account", () => {
    test.skip(!TEST_EMAIL || !TEST_OTP, "Privy test credentials not configured - see https://docs.privy.io/recipes/using-test-accounts");

    test("should authenticate and send transaction", async ({ page }) => {
      await page.goto("/");

      // Wait for Privy to load
      await expect(page.locator("text=Loading Privy...")).toBeHidden({
        timeout: 10000,
      });

      // Click Connect button
      await page.getByRole("button", { name: "Connect" }).click();

      // Wait for Privy modal to appear - try iframe first, then direct DOM
      let privyModal: any;
      const iframe = page.frameLocator('iframe[title*="privy" i]');
      const directModal = page.locator('[data-privy-dialog]').or(page.locator('[role="dialog"]'));

      // Check which one is available
      try {
        await page.locator('iframe').first().waitFor({ timeout: 3000 });
        privyModal = iframe;
      } catch {
        privyModal = directModal;
      }

      // Click on email login option if available
      const emailOption = page.getByText("Email", { exact: true });
      if (await emailOption.isVisible({ timeout: 5000 }).catch(() => false)) {
        await emailOption.click();
      }

      // Enter test email - try multiple selectors
      const emailInput = page.locator('input[type="email"]').or(page.locator('input[name="email"]'));
      await emailInput.waitFor({ timeout: 10000 });
      await emailInput.fill(TEST_EMAIL!);

      // Click submit button
      await page.getByRole("button", { name: "Submit" }).click();

      // Wait for OTP screen - look for the "Enter confirmation code" heading
      await expect(page.getByRole('heading', { name: /confirmation code/i })).toBeVisible({
        timeout: 15000,
      });

      // Find textboxes within the Privy dialog
      const dialog = page.getByRole('dialog');
      const otpInputs = dialog.getByRole('textbox');
      const otpCount = await otpInputs.count();
      console.log(`Found ${otpCount} OTP inputs in dialog`);

      // Click the first input to focus, then type all digits
      await otpInputs.first().click();
      await page.waitForTimeout(200);

      // Type each digit - Privy auto-advances to next input
      for (const digit of TEST_OTP!) {
        await page.keyboard.press(`Digit${digit}`);
        await page.waitForTimeout(150);
      }
      console.log(`Typed OTP: ${TEST_OTP}`);

      // Press Enter to submit (in case auto-submit doesn't trigger)
      await page.keyboard.press('Enter');

      // Wait for Privy to process
      await page.waitForTimeout(3000);

      // Wait for authentication to complete - check for transaction section heading
      await expect(
        page.getByRole("heading", { name: "Send Transaction" })
      ).toBeVisible({
        timeout: 30000,
      });

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
