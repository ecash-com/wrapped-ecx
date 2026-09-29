# swap-backend

Custodial bridge service between ECX (an eCash-derived chain, currently on
betanet) and wECX, an SPL token on Solana. Rust/axum HTTP API plus a CLI for
manual wallet operations.

Both ECX betanet and Solana mainnet-beta hold real value. There is no test
network for either side of this bridge — every automatic payout and every
manual CLI send requires an explicit opt-in (see **Safety** below).

## How it works

- **Peg-in (ECX → wECX):** a user requests an order, the backend hands back a
  fresh ECX deposit address, a background poller waits for the deposit to
  confirm, then sends `amount - fee` in wECX from the treasury's associated
  token account to the user's Solana address.
- **Peg-out (wECX → ECX):** a user requests an order and sends wECX to the
  treasury with a Solana Pay reference key; the backend matches the deposit
  by that reference (Solana has no per-order address scheme) and sends ECX
  back from the treasury's ECX wallet.

The ECX wallet is a [BDK](https://bitcoindevkit.org/) wallet derived from a
BIP39 mnemonic. The Solana treasury is a normal keypair holding an
already-minted SPL token — this service has no mint or burn authority over
wECX, only control of its own treasury account. Order state is persisted in
a local SQLite database file.

## Setup

1. Copy `.env.example` to `.env` and fill it in. Every variable is
   documented inline; at minimum you need:
   - `DATABASE_PATH` (optional, defaults to `data/bridge.db`)
   - `WALLET_MNEMONIC` — a fresh BIP39 mnemonic dedicated to this service.
     Do not reuse a mnemonic that controls funds elsewhere.
   - `SOLANA_KEYPAIR` — base58-encoded 64-byte secret key. Generate one with
     `cargo run -- solana-keygen` (prints the key and address, stores
     nothing).
   - `WECX_MINT_ADDRESS`
2. `cargo build`
3. `cargo run -- serve` to start the HTTP API (default `127.0.0.1:3000`).

Run `cargo run -- --help` for the full CLI, which also covers wallet
inspection (`address`, `balance`, `utxos`, `sync`), manual sends
(`send`, `solana-send`), and quoting (`quote`, `pegout-quote`).

## Safety

- `ALLOW_LIVE_PAYOUTS` gates every automatic outbound send (wECX payouts,
  ECX refunds, ECX payouts, wECX refunds) the background poller makes.
  It defaults off. Only set it to `1` once you've verified the bridge
  end-to-end with small amounts.
- Manual CLI sends (`send`, `solana-send`) each require their own explicit
  `--confirm-live` flag on top of that.
- The abuse-limit and treasury-floor variables documented in `.env.example`
  (`MAX_DEPOSIT_AMOUNT_SAT`, `TREASURY_MIN_SOL_LAMPORTS`,
  `ORDERS_PER_MINUTE`, `MAX_PENDING_ORDERS`, ...) are global backend limits.
  Put per-IP rate limiting at your reverse proxy — the backend only sees
  requests from its proxy, not client IPs.
- This is custodial software: whoever holds `WALLET_MNEMONIC` and
  `SOLANA_KEYPAIR` controls the funds in both wallets. Treat both as you
  would any hot wallet key.
- Not built yet: a cap on how much of total supply can be unwrapped per
  window (e.g. 10% per week), and splitting payout authority across
  multiple servers/keys (multisig) so no single one can drain the treasury.
  Both were flagged as needed guardrails but aren't enforced by this code.

## Development

- `cargo test`, `cargo clippy`, `cargo fmt`
- `cargo audit` is recommended before any production deployment; it isn't
  run in CI here yet.

## License

MIT — see [LICENSE](LICENSE).
