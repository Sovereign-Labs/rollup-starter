import { ethers } from "ethers";

const RPC_URL = process.env.SOV_RPC_URL || "http://localhost:12346/rpc";
const PRIVATE_KEY = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

const STORAGE_TESTER_ABI = [
  "function slot() view returns (uint256)",
  "function setSlot(uint256 value)",
  "function alternatingReadWrite(uint256 iterations) returns (uint256 checksum)",
  "function randomSlotWrites(uint256 iterations, uint256 seed)",
];

const STORAGE_TESTER_BYTECODE =
  "0x6080604052348015600e575f5ffd5b506103d58061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061004a575f3560e01c8063177d23641461004e5780631a88bc661461007e578063ace803e81461009c578063ffe0e3a1146100b8575b5f5ffd5b610068600480360381019061006391906101f5565b6100d4565b604051610075919061022f565b60405180910390f35b610086610151565b604051610093919061022f565b60405180910390f35b6100b660048036038101906100b191906101f5565b610156565b005b6100d260048036038101906100cd9190610248565b61015f565b005b5f5f5f90505b8281101561014b57805f819055505f5f54905081811461012f576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401610126906102e0565b60405180910390fd5b808361013b919061032b565b92505080806001019150506100da565b50919050565b5f5481565b805f8190555050565b5f8190505f5f90505b8381101561018c5761017982610192565b9150602a82558080600101915050610168565b50505050565b5f63ffffffff633c6ef35f836219660d6101ac919061035e565b6101b6919061032b565b169050919050565b5f5ffd5b5f819050919050565b6101d4816101c2565b81146101de575f5ffd5b50565b5f813590506101ef816101cb565b92915050565b5f6020828403121561020a576102096101be565b5b5f610217848285016101e1565b91505092915050565b610229816101c2565b82525050565b5f6020820190506102425f830184610220565b92915050565b5f5f6040838503121561025e5761025d6101be565b5b5f61026b858286016101e1565b925050602061027c858286016101e1565b9150509250929050565b5f82825260208201905092915050565b7f56616c7565206d69736d617463680000000000000000000000000000000000005f82015250565b5f6102ca600e83610286565b91506102d582610296565b602082019050919050565b5f6020820190508181035f8301526102f7816102be565b9050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52601160045260245ffd5b5f610335826101c2565b9150610340836101c2565b9250828201905080821115610358576103576102fe565b5b92915050565b5f610368826101c2565b9150610373836101c2565b9250828202610381816101c2565b91508282048414831517610398576103976102fe565b5b509291505056fea2646970667358221220cae3e1a4e4fbb7b5c56a3cdde98efe94de35474a141baf86705f3754ffd1df9e64736f6c634300081e0033";

async function main() {
  console.log("=== Deploying StorageTester ===\n");
  console.log("RPC URL:", RPC_URL);

  const provider = new ethers.JsonRpcProvider(RPC_URL);
  const wallet = new ethers.Wallet(PRIVATE_KEY, provider);

  console.log("Deployer address:", wallet.address);

  // Deploy StorageTester
  console.log("\nDeploying StorageTester...");
  const factory = new ethers.ContractFactory(STORAGE_TESTER_ABI, STORAGE_TESTER_BYTECODE, wallet);
  const contract = await factory.deploy();
  await contract.waitForDeployment();

  const contractAddress = await contract.getAddress();
  console.log("StorageTester deployed at:", contractAddress);

  // Send transactions
  console.log("\n--- Sending transactions ---\n");

  // Transaction 1: Set slot to 42
  console.log("1. Setting slot to 42...");
  let tx = await contract.setSlot(42);
  await tx.wait();
  console.log("   Tx hash:", tx.hash);

  // Transaction 2: Set slot to 100
  console.log("2. Setting slot to 100...");
  tx = await contract.setSlot(100);
  await tx.wait();
  console.log("   Tx hash:", tx.hash);

  // Transaction 3: Alternating read-write with 10 iterations
  console.log("3. Alternating read-write (10 iterations)...");
  tx = await contract.alternatingReadWrite(10);
  const receipt = await tx.wait();
  console.log("   Tx hash:", tx.hash);

  // Transaction 4: Random slot writes with 5 iterations
  console.log("4. Random slot writes (5 iterations, seed=12345)...");
  tx = await contract.randomSlotWrites(5, 12345);
  await tx.wait();
  console.log("   Tx hash:", tx.hash);

  // Read final slot value
  const finalSlot = await contract.slot();
  console.log("\nFinal slot value:", finalSlot.toString());

  console.log("\n=== Done ===");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
