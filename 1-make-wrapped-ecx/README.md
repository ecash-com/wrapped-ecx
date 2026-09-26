# 1. Make wrapped ECX

Create the SPL token that represents ECX on Solana ("wECX"). This is step 1 of the
[three-part guide](../README.md) to bootstrapping USD/ECX liquidity — it only covers creating
the token itself, not the wrap/unwrap bridge (step 2) or the liquidity pool (step 3).

## Options

- **[Official Solana CLI](https://solana.com/docs/references/spl-token-cli)** — fine for a
  one-off, manual mint if you're comfortable with the command line and don't need the extra
  scripts below.
- **[wecx-mint](wecx-mint/)** — a set of Node scripts for the whole token lifecycle: generating
  an authority keypair, creating the mint, attaching Metaplex metadata, and minting/burning
  supply against ECX deposits and withdrawals. Use this if you'll be minting/burning more than
  once, or want metadata attached. See [wecx-mint/README.md](wecx-mint/README.md) for setup and
  usage.

Either approach produces the same thing: an SPL mint address that step 2 (the bridge) mints to
and burns from, and that step 3 (the AMM pool) pairs against USD.

## Already minted

The wbECX mint address is
[`EVHqNdzjCupKi4rQkbuYw52sa1m8A7jeUAMP23S9AVVq`](https://solscan.io/token/EVHqNdzjCupKi4rQkbuYw52sa1m8A7jeUAMP23S9AVVq).
The first 10,000 wbECX were created in
[this transaction](https://solscan.io/tx/2mBvieMaTmAxuNbGm9aJwP69qVwrLax1krCfapP9vUjywA74yT8icE44NAYntPH9xXDfwTzf5hPanB2bxZPJezXU).
