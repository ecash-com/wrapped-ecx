# 2. Wrap/unwrap bridge

Run the custodial bridge service that converts between ECX and the wECX minted in
[step 1](../1-make-wrapped-ecx/), so users can move value in either direction. This is step 2 of
the [three-part guide](../README.md) — it assumes the wECX mint from step 1 already exists, and
feeds step 3 (the AMM pool) by being the thing that keeps wECX redeemable for real ECX.

## Parts

- **[backend](backend/)** — the Rust/axum service: HTTP API plus CLI, holds both the ECX (BDK)
  wallet and the Solana treasury, and runs the poller that matches deposits to orders and sends
  the corresponding payout. See its README for how peg-in and peg-out each work.
- **[frontend](frontend/)** — the SvelteKit UI, talks to the backend's HTTP API and never sees
  its secrets.

Run the backend first — the frontend needs a live `BACKEND_URL` to talk to. Each has its own
`.env.example` and setup instructions in its README.
