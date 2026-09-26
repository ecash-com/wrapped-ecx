import "dotenv/config";
import { readFileSync } from "fs";
import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import { getAssociatedTokenAddress, burn } from "@solana/spl-token";

// Burns wrapped tokens from the authority's own token account (or OWNER_KEYPAIR_PATH's,
// if set). Use this per PLAN.md Section A's withdraw->burn workflow — i.e. only after
// the corresponding real ECX withdrawal has been queued/paid out from your reserve.
// This does not touch the ECX side itself; that's on you / the not-yet-built withdrawal
// watcher.
const RPC_URL = process.env.RPC_URL || clusterApiUrl("mainnet-beta");
const KEYPAIR_PATH = process.env.KEYPAIR_PATH || "keys/authority.json";
const OWNER_KEYPAIR_PATH = process.env.OWNER_KEYPAIR_PATH || KEYPAIR_PATH; // owner of the tokens being burned
const AMOUNT = process.env.AMOUNT; // human units

if (!AMOUNT) {
  console.error("Set AMOUNT (human units of the wrapped token) before running this.");
  process.exit(1);
}

const mintInfo = JSON.parse(readFileSync("mint-info.json", "utf8"));
const secret = JSON.parse(readFileSync(OWNER_KEYPAIR_PATH, "utf8"));
const owner = Keypair.fromSecretKey(Uint8Array.from(secret));
const connection = new Connection(RPC_URL, "confirmed");

const mint = new PublicKey(mintInfo.mint);

function toBaseUnits(amount, decimals) {
  const [whole, frac = ""] = String(amount).split(".");
  const fracPadded = (frac + "0".repeat(decimals)).slice(0, decimals);
  return BigInt(whole + fracPadded);
}

console.log(`Burning ${AMOUNT} tokens (${mintInfo.mint}) from ${owner.publicKey.toBase58()}`);

const ata = await getAssociatedTokenAddress(mint, owner.publicKey);
const sig = await burn(
  connection,
  owner, // payer
  ata,
  mint,
  owner, // token account owner — must be this keypair
  toBaseUnits(AMOUNT, mintInfo.decimals)
);

console.log(`Burned. Tx: ${sig}`);
console.log(`Token account: ${ata.toBase58()}`);
