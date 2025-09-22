import {ethers} from 'ethers';
import {readWarpRouteConfig, readWarpRouteIdOnRollup} from "./utils";
import {ANVIL_KEY_0, ROLLUP_STARTER_DOMAIN} from "./consts";
// Contract configuration
const CONTRACT_ADDRESS = readWarpRouteConfig();
const RPC_URL = 'http://localhost:8545';
const PRIVATE_KEY = ANVIL_KEY_0;

// Function parameters
const DOMAIN = ROLLUP_STARTER_DOMAIN;
const ROUTER_ADDRESS = readWarpRouteIdOnRollup();

// ABI for the enrollRemoteRouter function
const ABI = [
    'function enrollRemoteRouter(uint32 domain, bytes32 routerAddress)'
];

try {
    const provider = new ethers.JsonRpcProvider(RPC_URL);
    const wallet = new ethers.Wallet(PRIVATE_KEY, provider);
    const contract = new ethers.Contract(CONTRACT_ADDRESS, ABI, wallet);

    console.log('[✓] Enrolling remote router...');
    console.log(`  Contract: ${CONTRACT_ADDRESS}`);
    console.log(`  Domain: ${DOMAIN}`);
    console.log(`  Router: ${ROUTER_ADDRESS}`);

    // Send the transaction
    const tx = await contract.enrollRemoteRouter(DOMAIN, ROUTER_ADDRESS);
    console.log(`[✓] Transaction sent: ${tx.hash}`);

    // Wait for confirmation
    const receipt = await tx.wait();
    console.log(`[✓] Transaction confirmed in block: ${receipt.blockNumber}`);
    console.log(`  Gas used: ${receipt.gasUsed.toString()}`);
} catch (error) {
    console.error(`[✓] Error: ${error}`);
    process.exit(1);
}