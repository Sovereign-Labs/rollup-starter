import { TurnkeyClient } from "@turnkey/http";
import { ApiKeyStamper } from "@turnkey/api-key-stamper";
import { ethers } from "ethers"; // v6 helpers
import type { Signer } from "@sovereign-sdk/signers";
import { hexToBytes, bytesToHex } from "@sovereign-sdk/utils";
import { Point } from "@noble/secp256k1";
import { keccak_256 } from "@noble/hashes/sha3";

export interface TurnkeyConfig {
    organizationId: string;
    apiPublicKey: string;
    apiPrivateKey: string;
    keyId: string;
}

// Helper function to wait for a certain amount of time.
const wait = (ms: number) => new Promise(r => setTimeout(r, ms));

// A signer key from Turnkey.
export class TurnkeySigner implements Signer {
    private tk: TurnkeyClient;
    private config: TurnkeyConfig;
    public readonly _publicKey: Uint8Array;
    public readonly curve: string;
    constructor(
        publicKey: Uint8Array,
        turnkeyClient: TurnkeyClient,
        config: TurnkeyConfig,
        curve: string
    ) {
        this._publicKey = publicKey;
        this.tk = turnkeyClient;
        this.config = config;
        this.curve = curve;
    }

    public static async create(config: TurnkeyConfig): Promise<TurnkeySigner> {

        const stamper = new ApiKeyStamper({
            apiPublicKey: config.apiPublicKey,
            apiPrivateKey: config.apiPrivateKey,
        });

        const turnkeyClient = new TurnkeyClient({ baseUrl: "https://api.turnkey.com" }, stamper);

        const response = await turnkeyClient.getPrivateKey({
            organizationId: config.organizationId,
            privateKeyId: config.keyId,
        });

        if (response.privateKey.curve === "CURVE_SECP256K1") {
            const uncompressedPublicKey = response.privateKey.publicKey;
            if (!uncompressedPublicKey) {
                throw new Error(`Could not retrieve public key for Turnkey key ${config.keyId}`);
            }
            // The public key from Turnkey is uncompressed (prefixed with 0x04, 65 bytes).
            // The Sovereign SDK expects a compressed (33-byte) public key.
            const point   = Point.fromHex(uncompressedPublicKey);
            const compressedPublicKey = point.toRawBytes(true);
            return new TurnkeySigner(compressedPublicKey, turnkeyClient, config, "secp256k1");
        } 

        if (response.privateKey.curve === "CURVE_ED25519") {
            const publicKeyHex = response.privateKey.publicKey;
            if (!publicKeyHex) {
                throw new Error(`Could not retrieve public key for Turnkey key ${config.keyId}`);
            }
            // ED25519 public keys are 32 bytes in raw format
            // Convert from hex string to Uint8Array
            const publicKeyBytes = hexToBytes(publicKeyHex);
            return new TurnkeySigner(publicKeyBytes, turnkeyClient, config, "ed25519");
        } 

        throw new Error(`Unsupported curve: ${response.privateKey.curve}`);
        

    }

    public async publicKey(): Promise<Uint8Array> {
        return this._publicKey;
    }

    async sign(message: Uint8Array): Promise<Uint8Array> {
        // For ed255519, the hashing is internal to turnkey. Their docs state that only `HASH_FUNCTION_NOT_APPLICABLE` is supported.
        const payload = this.curve === "secp256k1" ? bytesToHex(keccak_256(message)) : bytesToHex(message);

        const response = await this.tk.signRawPayload({
            type: "ACTIVITY_TYPE_SIGN_RAW_PAYLOAD_V2",
            organizationId: this.config.organizationId,
            timestampMs: String(Date.now()),
            parameters: {
                signWith: this.config.keyId,
                payload,
                encoding: "PAYLOAD_ENCODING_HEXADECIMAL",
                hashFunction: this.curve === "secp256k1" ? "HASH_FUNCTION_NO_OP" : "HASH_FUNCTION_NOT_APPLICABLE",
            },
        });
        await wait(1000);

        // Poll every 60s until COMPLETED
        while (true) {
            const { activity } = await this.tk.getActivity({
                organizationId: this.config.organizationId,
                activityId: response.activity.id,
            });

            if (activity.status === "ACTIVITY_STATUS_COMPLETED") {
                if (activity.result?.signRawPayloadResult) {
                    const { r, s } = activity.result.signRawPayloadResult;

                    // The Sovereign SDK expects a 64-byte signature (r || s).
                    // The recovery ID (v) is handled separately by the SDK.
                    const sigBytes = ethers.concat([
                        ethers.getBytes(`0x${r}`),
                        ethers.getBytes(`0x${s}`),
                    ]);

                    return hexToBytes(sigBytes);
                } 

                throw new Error(`Turnkey activity completed but missing signature result. Full activity: ${JSON.stringify(activity, null, 2)}`);
            }

            if (activity.status === "ACTIVITY_STATUS_REJECTED" || activity.status === "ACTIVITY_STATUS_FAILED") {
                throw new Error(`Turnkey activity was not completed. Status: ${activity.status}`);
            }
            await wait(60000);

        }
    }
}
