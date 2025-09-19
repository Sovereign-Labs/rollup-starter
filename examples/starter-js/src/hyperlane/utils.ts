import path from "path";
import fs from "fs";
import yaml from "js-yaml";
import {fileURLToPath} from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
export const testDataFile = path.join(__dirname, '../../../../test-data/sovstarter-ethtest-warp.json');

export function readWarpRouteConfig(): string {
    const configPath = path.join(__dirname, '../../../../integrations/hyperlane/configs/deployments/warp_routes/ETH/warp-route-deployment-config.yaml');

    try {
        if (!fs.existsSync(configPath)) {
            throw new Error(`Configuration file not found at ${configPath}`);
        }

        const fileContent = fs.readFileSync(configPath, 'utf8');
        const config = yaml.load(fileContent) as any;

        if (!config?.tokens?.[0]?.addressOrDenom) {
            throw new Error('Invalid configuration format or missing addressOrDenom');
        }

        const addressOrDenom = config.tokens[0].addressOrDenom;
        console.log(`[✓] Successfully read addressOrDenom: ${addressOrDenom}`);
        return addressOrDenom;

    } catch (error) {
        console.error(`[✗] Error: ${error}`);
        process.exit(1);
    }
}

export function readWarpRouteIdOnRollup(): string {
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

export function zeroPad20To32(input: string): string {
    return "0x" + "00".repeat(12) + input.slice(2);
}