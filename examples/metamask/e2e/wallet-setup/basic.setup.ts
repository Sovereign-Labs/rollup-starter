import { defineWalletSetup } from "@synthetixio/synpress";
import { MetaMask } from "@synthetixio/synpress/playwright";

// Standard test mnemonic (Hardhat/Anvil default - DO NOT use for real funds!)
const TEST_SEED_PHRASE =
  "test test test test test test test test test test test junk";
const WALLET_PASSWORD = "Tester@1234";

export default defineWalletSetup(WALLET_PASSWORD, async (context, walletPage) => {
  const metamask = new MetaMask(context, walletPage, WALLET_PASSWORD);
  await metamask.importWallet(TEST_SEED_PHRASE);
});

export { WALLET_PASSWORD };
