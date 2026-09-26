import { Keypair } from "@solana/web3.js";
import { writeFileSync, existsSync, mkdirSync } from "fs";

const outPath = process.argv[2] || "keys/authority.json";

if (existsSync(outPath)) {
  console.error(`Refusing to overwrite existing keypair at ${outPath}`);
  process.exit(1);
}

mkdirSync("keys", { recursive: true });

const keypair = Keypair.generate();
writeFileSync(outPath, JSON.stringify(Array.from(keypair.secretKey)));

console.log(`Saved new keypair to ${outPath}`);
console.log(`Public key: ${keypair.publicKey.toBase58()}`);
console.log(`\nFund this address before minting:`);
console.log(`  Devnet: solana airdrop 2 ${keypair.publicKey.toBase58()} --url devnet`);
console.log(`  Mainnet: send SOL to it from an exchange/wallet you control`);
