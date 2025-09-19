import {ethers} from 'ethers';
import {readWarpRouteConfig, readWarpRouteIdOnRollup, zeroPad20To32} from "./utils";
import {ANVIL_KEY_3, deployerAddress, ROLLUP_STARTER_DOMAIN} from "./consts";
// Contract configuration
const CONTRACT_ADDRESS = readWarpRouteConfig();
const RPC_URL = 'http://localhost:8545';
const PRIVATE_KEY = ANVIL_KEY_3;

// Function parameters
const DOMAIN = ROLLUP_STARTER_DOMAIN;
const ROUTER_ADDRESS = readWarpRouteIdOnRollup();

// ABI for the enrollRemoteRouter function
const ABI = [
    'function transferRemote(uint32 destination, bytes32 recipient, uint256 amount) payable returns (bytes32 messageId)',
    'function quoteGasPayment(uint32 destination) view returns (uint256)',
];

try {
    const provider = new ethers.JsonRpcProvider(RPC_URL);
    const wallet = new ethers.Wallet(PRIVATE_KEY, provider);
    const warpRoute = new ethers.Contract(CONTRACT_ADDRESS, ABI, wallet);
    const recipient = zeroPad20To32(deployerAddress);
    console.log('[*] Making inbound warp transfer...');
    console.log(`  Contract:  ${CONTRACT_ADDRESS}`);
    console.log(`  Domain:    ${DOMAIN}`);
    console.log(`  Router:    ${ROUTER_ADDRESS}`);
    console.log(`  Recipient: ${recipient}`);

    const transferAmount = ethers.parseEther('0.01');
    const gasPayment = await warpRoute.quoteGasPayment(DOMAIN);
    const totalValue = transferAmount + gasPayment;
    
    console.log(`  Amount:    ${ethers.formatEther(transferAmount)} ETH`);
    console.log(`  Gas:       ${ethers.formatEther(gasPayment)} ETH`);
    console.log(`  Total:     ${ethers.formatEther(totalValue)} ETH`);

    const tx = await warpRoute.transferRemote(
        DOMAIN,
        recipient,
        transferAmount,
        {value: totalValue}
    );
    console.log(`[] Transaction sent: ${tx.hash}`);
    // Wait for confirmation
    const receipt = await tx.wait();
    console.log(`[] Transaction confirmed in block: ${receipt.blockNumber}`);
    console.log(`  Gas used: ${receipt.gasUsed.toString()}`);

} catch (error) {
    console.error(`[] Error: ${error}`);
    process.exit(1);
}