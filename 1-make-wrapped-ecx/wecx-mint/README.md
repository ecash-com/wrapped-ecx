# wecx-mint

This repo is a set of Node scripts, not a service. There is no deposit watcher or relayer.
Minting and burning are manual steps that you do only after checking the matching ECX
deposit or withdrawal against your reserve.

## What's here

| Script                                               | Command                           | Purpose                                                                  |
| ---------------------------------------------------- | --------------------------------- | ------------------------------------------------------------------------ |
| [generate-keypair.mjs](scripts/generate-keypair.mjs) | `npm run generate-keypair [path]` | Create a keypair (default `keys/authority.json`)                         |
| [create-mint.mjs](scripts/create-mint.mjs)           | `npm run create-mint`             | Create the SPL mint (freeze authority = none) and write `mint-info.json` |
| [create-metadata.mjs](scripts/create-metadata.mjs)   | `npm run create-metadata`         | Attach Metaplex on-chain metadata (name, symbol, URI)                    |
| [update-metadata.mjs](scripts/update-metadata.mjs)   | `npm run update-metadata`         | Update that metadata later                                               |
| [mint-to.mjs](scripts/mint-to.mjs)                   | `npm run mint-to`                 | Mint tokens to a recipient wallet                                        |
| [burn.mjs](scripts/burn.mjs)                         | `npm run burn`                    | Burn tokens from the owner's token account                               |

## Setup

```bash
npm install
npm run generate-keypair     # writes keys/authority.json
```

Requires Node.js with ES module support. The scripts use top-level `await`, so use a
current LTS release. Fund the keypair's public key with SOL before running anything that sends a
transaction.

## Usage

**The scripts default to Solana mainnet-beta and spend real SOL.** To test on devnet, set
`RPC_URL=https://api.devnet.solana.com` on each command.

```bash
# 1. Create the mint (8 decimals by default)
npm run create-mint

# 2. Attach metadata (host the JSON first; see config/dev-token-metadata.json for the shape)
TOKEN_NAME=<name> TOKEN_SYMBOL=<symbol> METADATA_URI=https://your-host/token-metadata.json npm run create-metadata

# 3. Mints AMOUNT to RECIPIENT.
RECIPIENT=<wallet address> AMOUNT=100 npm run mint-to

# Burn tokens (e.g. after paying out a matching ECX withdrawal)
AMOUNT=100 npm run burn
```

### Configuration

Set these as environment variables. A `.env` file also works, since `dotenv` is loaded.

| Variable                      | Used by                              | Default                                                          |
| ----------------------------- | ------------------------------------ | ---------------------------------------------------------------- |
| `RPC_URL`                     | all scripts                          | mainnet-beta public RPC                                          |
| `KEYPAIR_PATH`                | all scripts                          | `keys/authority.json`                                            |
| `DECIMALS`                    | `create-mint`                        | `8`                                                              |
| `METADATA_URI`                | `create-metadata`, `update-metadata` | none, required                                                   |
| `TOKEN_NAME`, `TOKEN_SYMBOL`  | `create-metadata`, `update-metadata` | none, required                                                   |
| `RECIPIENT`                   | `mint-to`                            | none, required                                                   |
| `AMOUNT`                      | `mint-to`, `burn`                    | none, required (human units)                                     |
| `OWNER_KEYPAIR_PATH`          | `burn`                               | same as `KEYPAIR_PATH`                                           |
| `TOKEN_AMOUNT`, `USDT_AMOUNT` | `create-pool`                        | none, required (human units; their ratio sets the initial price) |
| `USDT_MINT`, `USDT_DECIMALS`  | `create-pool`                        | real mainnet USDT / `6`. `USDT_MINT` is required on devnet       |
| `POOL_ID`                     | `lock-liquidity`                     | `poolId` from `pool-info.json`                                   |

### Local files

These are gitignored:

- `keys/`: keypairs. **Never commit or share these.**
- `mint-info.json`: the mint address, decimals and authorities. The scripts read it, and `create-mint` writes it.
- `pool-info.json`, `lock-info.json`: pool and LP-lock details. Publish the lock proof.
- `.env`

## Before production:

- **If possible, move the mint authority to a multisig** (e.g. Squads) before seeding real liquidity. A single
  key that can mint is a single point of failure for the peg. This is not scripted here yet.
- **Back every mint with reserved ECX.** `mint-to` checks nothing on the ECX side. That check is up
  to you.
- The canonical token identifier is the **mint address**, not the symbol. Expect copycat tokens.
  Publish the full address.

## Status

Done: mint creation, metadata, mint/burn.

Not built yet: the ECX deposit watcher / mint relayer, the burn/redeem workflow, multisig authority transfer, and a test suite.
