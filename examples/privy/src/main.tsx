import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { PrivyProvider } from "@privy-io/react-auth";
import "./index.css";
import App from "./App.tsx";

const appId = import.meta.env.VITE_PRIVY_APP_ID || "cmkfhuxig00izl40crx0audoz";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <PrivyProvider
      appId={appId}
      config={{
        appearance: {
          theme: "light",
          accentColor: "#676FFF",
          logo: "https://your-logo-url.png",
          walletList: ["metamask", "rabby_wallet", "wallet_connect"],
        },
        loginMethods: ["email", "wallet"],
        embeddedWallets: {
          createOnLogin: "all-users",
          requireUserPasswordOnCreate: false,
        },
      }}
    >
      <App />
    </PrivyProvider>
  </StrictMode>,
);
