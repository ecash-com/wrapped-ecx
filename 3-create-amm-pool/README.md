# 3. Create the AMM pool

Create the wECX/USD liquidity pool. This is step 3 of the [three-part guide](../README.md) —
by this point you should already have the wECX mint (step 1) and a working wrap/unwrap bridge
(step 2), since the pool is only useful once real ECX can be minted into and redeemed from it.

## Creating the pool

No script here does this for you — use one of these hosted tools instead:

- **[Orca](https://www.orca.so/create-pool)** — the one we've actually used.
- **[Raydium (CLMM)](https://raydium.io/clmm/create-pool/)** or
  **[Meteora (DLMM)](https://www.meteora.ag/create/dlmm/standard)** — untested alternatives, in
  case Orca doesn't fit (e.g. different fee tiers or concentrated-liquidity behavior).

All three are constant-product/concentrated-liquidity AMMs, so the pricing math below applies
regardless of which one you pick. Whichever you use, seed the pool with wECX and USD (or USDT/USDC)
in a ratio matching your intended ECX price, and keep the LP tokens somewhere you can prove are
locked — see [wecx-mint](../1-make-wrapped-ecx/wecx-mint/)'s notes on publishing a lock proof.

It doesn't matter much which of these you pick, since most wallets route swaps through
**[Jupiter](https://jup.ag/)**, which aggregates liquidity across all of them anyway. We should
probably point users at Jupiter directly rather than at a wallet's built-in swap: most wallets take
a cut on top of the swap when you trade through them, which you avoid by trading on Jupiter directly.

## Estimating price impact beforehand

[amm-calculator.svelte](amm-calculator.svelte) is a standalone constant-product (`x * y = k`)
simulator: set the ECX/USD reserves you're planning to seed, then see the resulting spot price,
and how much a given buy or sell would move it. Use it to sanity-check pool size against expected
trade sizes before committing liquidity — a pool that's too shallow will show large price impact
even for modest trades.

No install needed: paste the file's contents into the
[Svelte Playground](https://svelte.dev/playground/) and run it there.
