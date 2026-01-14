type ConnectButtonProps = {
  account: string | null;
  isConnecting: boolean;
  isMetaMaskInstalled: boolean;
  onConnect: () => void;
};

export default function ConnectButton({
  account,
  isConnecting,
  isMetaMaskInstalled,
  onConnect,
}: ConnectButtonProps) {
  if (!isMetaMaskInstalled) {
    return <button disabled>Install MetaMask</button>;
  }

  if (account) {
    return (
      <button disabled>
        Connected: {account.slice(0, 6)}…{account.slice(-4)}
      </button>
    );
  }

  return (
    <button onClick={onConnect} disabled={isConnecting}>
      {isConnecting ? "Connecting..." : "Connect MetaMask"}
    </button>
  );
}