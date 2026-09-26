import "dotenv/config";
import { readFileSync } from "fs";
import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import { getOrCreateAssociatedTokenAccount, mintTo } from "@solana/spl-token";

// Mints wrapped tokens to an explicit RECIPIENT (required — no default). Use this
// per PLAN.md Section A's deposit->mint workflow — i.e. only after a corresponding
// real ECX deposit has been confirmed in your reserve, or (once, deliberately) to seed
// initial DEX liquidity. This does not check anything on the ECX side itself; that
// verification is on you / the not-yet-built deposit watcher.
const RPC_URL = process.env.RPC_URL || clusterApiUrl("mainnet-beta");
const KEYPAIR_PATH = process.env.KEYPAIR_PATH || "keys/authority.json";
const AMOUNT = process.env.AMOUNT; // human units
const RECIPIENT = process.env.RECIPIENT; // base58 pubkey of the wallet to receive the tokens

if (!AMOUNT) {
  console.error("Set AMOUNT (human units of the wrapped token) before running this.");
  process.exit(1);
}
if (!RECIPIENT) {
  console.error("Set RECIPIENT (base58 wallet address to receive the tokens) before running this.");
  process.exit(1);
}

const mintInfo = JSON.parse(readFileSync("mint-info.json", "utf8"));
const secret = JSON.parse(readFileSync(KEYPAIR_PATH, "utf8"));
const payer = Keypair.fromSecretKey(Uint8Array.from(secret));
const connection = new Connection(RPC_URL, "confirmed");

const mint = new PublicKey(mintInfo.mint);
const recipient = new PublicKey(RECIPIENT);

function toBaseUnits(amount, decimals) {
  const [whole, frac = ""] = String(amount).split(".");
  const fracPadded = (frac + "0".repeat(decimals)).slice(0, decimals);
  return BigInt(whole + fracPadded);
}

console.log(`Minting ${AMOUNT} tokens (${mintInfo.mint}) to ${recipient.toBase58()}`);

const ata = await getOrCreateAssociatedTokenAccount(connection, payer, mint, recipient);
const sig = await mintTo(
  connection,
  payer,
  mint,
  ata.address,
  payer, // mint authority — must be this keypair (or update MintAuthority elsewhere first)
  toBaseUnits(AMOUNT, mintInfo.decimals)
);

console.log(`Minted. Tx: ${sig}`);
console.log(`Recipient token account: ${ata.address.toBase58()}`);
