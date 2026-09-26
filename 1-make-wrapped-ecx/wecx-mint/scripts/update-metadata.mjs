import "dotenv/config";
import { readFileSync } from "fs";
import { Keypair, clusterApiUrl } from "@solana/web3.js";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import {
  createSignerFromKeypair,
  signerIdentity,
  publicKey as toUmiPublicKey,
  percentAmount,
} from "@metaplex-foundation/umi";
import { fromWeb3JsKeypair } from "@metaplex-foundation/umi-web3js-adapters";
import { updateV1 } from "@metaplex-foundation/mpl-token-metadata";

const RPC_URL = process.env.RPC_URL || clusterApiUrl("mainnet-beta");
const KEYPAIR_PATH = process.env.KEYPAIR_PATH || "keys/authority.json";

const TOKEN_NAME = process.env.TOKEN_NAME;
const TOKEN_SYMBOL = process.env.TOKEN_SYMBOL;
const METADATA_URI = process.env.METADATA_URI;

if (!TOKEN_NAME || !TOKEN_SYMBOL) {
  console.error("Set TOKEN_NAME and TOKEN_SYMBOL before running this.");
  process.exit(1);
}
if (!METADATA_URI) {
  console.error(
    "Set METADATA_URI to the hosted JSON file (see config/token-metadata.json) before running this."
  );
  process.exit(1);
}

const mintInfo = JSON.parse(readFileSync("mint-info.json", "utf8"));
const secret = JSON.parse(readFileSync(KEYPAIR_PATH, "utf8"));
const web3Keypair = Keypair.fromSecretKey(Uint8Array.from(secret));

const umi = createUmi(RPC_URL);
const signer = createSignerFromKeypair(umi, fromWeb3JsKeypair(web3Keypair));
umi.use(signerIdentity(signer));

const mint = toUmiPublicKey(mintInfo.mint);

console.log(`Updating metadata for mint: ${mintInfo.mint}`);
console.log(`Name: ${TOKEN_NAME} | Symbol: ${TOKEN_SYMBOL}`);
console.log(`URI: ${METADATA_URI}`);

// updateV1 replaces the whole Data struct when `data` is provided — must resend every
// field (name/symbol/uri/etc.), not just the one that changed. authority must be the
// current update authority (set at create time — your authority keypair, until it's
// moved to a multisig per EXECUTION.md step 5).
const tx = await updateV1(umi, {
  mint,
  authority: signer,
  data: {
    name: TOKEN_NAME,
    symbol: TOKEN_SYMBOL,
    uri: METADATA_URI,
    sellerFeeBasisPoints: percentAmount(0),
    creators: null,
  },
}).sendAndConfirm(umi);

console.log(`\nMetadata updated. Signature: ${Buffer.from(tx.signature).toString("base64")}`);
