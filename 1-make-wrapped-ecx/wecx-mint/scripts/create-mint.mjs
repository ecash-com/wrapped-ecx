import "dotenv/config";
import { Connection, Keypair, clusterApiUrl } from "@solana/web3.js";
import { createMint } from "@solana/spl-token";
import { readFileSync, writeFileSync, existsSync } from "fs";

const RPC_URL = process.env.RPC_URL || clusterApiUrl("mainnet-beta");
const KEYPAIR_PATH = process.env.KEYPAIR_PATH || "keys/authority.json";
const DECIMALS = Number(process.env.DECIMALS || 8);

if (!existsSync(KEYPAIR_PATH)) {
  console.error(`No keypair found at ${KEYPAIR_PATH}. Run: npm run generate-keypair`);
  process.exit(1);
}

const secret = JSON.parse(readFileSync(KEYPAIR_PATH, "utf8"));
const payer = Keypair.fromSecretKey(Uint8Array.from(secret));

const connection = new Connection(RPC_URL, "confirmed");

console.log(`Cluster: ${RPC_URL}`);
console.log(`Payer / mint authority: ${payer.publicKey.toBase58()}`);

const balance = await connection.getBalance(payer.publicKey);
if (balance === 0) {
  console.error("Payer has 0 SOL. Fund it first (see generate-keypair.mjs output).");
  process.exit(1);
}

// Freeze authority is intentionally null (see PLAN.md — no compliance reason to keep one).
// Mint authority starts as this keypair; transfer it to a multisig (e.g. Squads) before
// going live with real liquidity — see EXECUTION.md step 5.
const mint = await createMint(
  connection,
  payer,
  payer.publicKey,
  null,
  DECIMALS
);

console.log(`\nMint created: ${mint.toBase58()}`);

writeFileSync(
  "mint-info.json",
  JSON.stringify(
    {
      mint: mint.toBase58(),
      decimals: DECIMALS,
      mintAuthority: payer.publicKey.toBase58(),
      freezeAuthority: null,
      cluster: RPC_URL,
    },
    null,
    2
  )
);
console.log("Saved details to mint-info.json");
