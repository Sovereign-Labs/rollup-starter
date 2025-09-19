import {ethers} from 'ethers';
import fs from 'fs';
import path from 'path';
import {fileURLToPath} from 'url';
import {readWarpRouteConfig, testDataFile} from "./utils";
import {ANVIL_KEY_0, ROLLUP_STARTER_DOMAIN} from "./consts";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Read router address from test data file
function readRouterAddress(): string {


    try {
        if (!fs.existsSync(testDataFile)) {
            throw new Error(`Test data file not found at ${testDataFile}`);
        }

        const fileContent = fs.readFileSync(testDataFile, 'utf8');
        const data = JSON.parse(fileContent);

        if (!data.warp_route_id) {
            throw new Error('warp_route_id not found in test data file');
        }

        console.log(`[✓] Read router address from test data: ${data.warp_route_id}`);
        return data.warp_route_id;

    } catch (error) {
        console.error(`[✗] Error reading router address: ${error}`);
        process.exit(1);
    }
}

// Contract configuration
const CONTRACT_ADDRESS = readWarpRouteConfig();
const RPC_URL = 'http://localhost:8545';
const PRIVATE_KEY = ANVIL_KEY_0;

// Function parameters
const DOMAIN = ROLLUP_STARTER_DOMAIN;
const ROUTER_ADDRESS = readRouterAddress();

// ABI for the enrollRemoteRouter function
const ABI = [
    'function enrollRemoteRouter(uint32 domain, bytes32 routerAddress)'
];


try {
    const provider = new ethers.JsonRpcProvider(RPC_URL);
    const wallet = new ethers.Wallet(PRIVATE_KEY, provider);
    const contract = new ethers.Contract(CONTRACT_ADDRESS, ABI, wallet);

    console.log('[*] Enrolling remote router...');
    console.log(`  Contract: ${CONTRACT_ADDRESS}`);
    console.log(`  Domain: ${DOMAIN}`);
    console.log(`  Router: ${ROUTER_ADDRESS}`);

    // Send the transaction
    const tx = await contract.enrollRemoteRouter(DOMAIN, ROUTER_ADDRESS);
    console.log(`[] Transaction sent: ${tx.hash}`);

    // Wait for confirmation
    const receipt = await tx.wait();
    console.log(`[] Transaction confirmed in block: ${receipt.blockNumber}`);
    console.log(`  Gas used: ${receipt.gasUsed.toString()}`);

} catch (error) {
    console.error(`[] Error: ${error}`);
    process.exit(1);
}