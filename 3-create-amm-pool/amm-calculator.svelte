<script>
	let ecx = 1000;
	let usd = 1000;

	let buyUsd = 100;
	let sellEcx = 100;

	// Constant product
	$: k = ecx * usd;

	// Current spot price
	$: ecxPriceUsd = ecx > 0 ? usd / ecx : 0;

	// --------------------------------------------------
	// BUY ECX
	// User adds USD to the pool and receives ECX.
	// --------------------------------------------------

	$: buyNewUsd = usd + Math.max(0, Number(buyUsd) || 0);

	$: buyNewEcx = buyNewUsd > 0 ? k / buyNewUsd : ecx;

	$: buyEcxReceived =
		Math.max(0, Number(buyUsd) || 0) > 0 ? ecx - buyNewEcx : 0;

	$: buyEffectivePrice = buyEcxReceived > 0 ? buyUsd / buyEcxReceived : 0;

	$: buyPriceImpact =
		ecxPriceUsd > 0
			? ((buyEffectivePrice - ecxPriceUsd) / ecxPriceUsd) * 100
			: 0;

	// --------------------------------------------------
	// SELL ECX
	// User adds ECX to the pool and receives USD.
	// --------------------------------------------------

	$: sellNewEcx = ecx + Math.max(0, Number(sellEcx) || 0);

	$: sellNewUsd = sellNewEcx > 0 ? k / sellNewEcx : usd;

	$: sellUsdReceived =
		Math.max(0, Number(sellEcx) || 0) > 0 ? usd - sellNewUsd : 0;

	$: sellEffectivePrice = sellEcx > 0 ? sellUsdReceived / sellEcx : 0;

	$: sellPriceImpact =
		ecxPriceUsd > 0
			? ((ecxPriceUsd - sellEffectivePrice) / ecxPriceUsd) * 100
			: 0;

	function format(value, digits = 8) {
		if (
			value === null ||
			value === undefined ||
			!Number.isFinite(Number(value))
		) {
			return "—";
		}

		return Number(value).toLocaleString("en-US", {
			maximumFractionDigits: digits,
		});
	}

	function formatUsd(value) {
		if (
			value === null ||
			value === undefined ||
			!Number.isFinite(Number(value))
		) {
			return "—";
		}

		return new Intl.NumberFormat("en-US", {
			style: "currency",
			currency: "USD",
			maximumFractionDigits: value < 1 ? 8 : 2,
		}).format(value);
	}
</script>

<svelte:head>
	<title>ECX / USD AMM</title>
	<meta name="description" content="ECX/USD constant-product AMM simulator" />
</svelte:head>

<div class="page">
	<div class="card">
		<h1>ECX / USD AMM</h1>
		<p class="subtitle">Constant product: X × Y = k</p>

		<!-- POOL RESERVES -->

		<div class="section-title">Pool Reserves</div>

		<div class="inputs">
			<label>
				<span>ECX Reserve</span>
				<input type="number" min="0" step="any" bind:value={ecx} />
			</label>

			<label>
				<span>USD Reserve</span>
				<input type="number" min="0" step="any" bind:value={usd} />
			</label>
		</div>

		<!-- CURRENT PRICE -->

		<div class="result">
			<div class="price-label">Current ECX Price</div>

			<div class="price">
				{formatUsd(ecxPriceUsd)}
			</div>

			<div class="sub-price">
				1 ECX = {formatUsd(ecxPriceUsd)}
			</div>
		</div>

		<!-- BUY -->

		<div class="trade">
			<div class="trade-header">
				<div>
					<h2>Buy ECX</h2>
					<p>Spend USD to receive ECX</p>
				</div>

				<div class="trade-icon buy">↓</div>
			</div>

			<label>
				<span>USD to Spend</span>
				<div class="input-prefix">
					<span>$</span>
					<input type="number" min="0" step="any" bind:value={buyUsd} />
				</div>
			</label>

			<div class="trade-result">
				<div>
					<span>You receive</span>
					<strong>{format(buyEcxReceived)} ECX</strong>
				</div>

				<div>
					<span>Effective price</span>
					<strong>{formatUsd(buyEffectivePrice)}</strong>
				</div>

				<div>
					<span>Price impact</span>
					<strong class:negative={buyPriceImpact > 0}>
						{format(buyPriceImpact, 4)}%
					</strong>
				</div>
			</div>

			<div class="after">
				<div class="after-title">Pool after trade</div>

				<div class="reserve-row">
					<span>ECX</span>
					<strong>{format(buyNewEcx)}</strong>
				</div>

				<div class="reserve-row">
					<span>USD</span>
					<strong>{formatUsd(buyNewUsd)}</strong>
				</div>
			</div>
		</div>

		<!-- SELL -->

		<div class="trade">
			<div class="trade-header">
				<div>
					<h2>Sell ECX</h2>
					<p>Sell ECX to receive USD</p>
				</div>

				<div class="trade-icon sell">↑</div>
			</div>

			<label>
				<span>ECX to Sell</span>
				<div class="input-prefix ecx-input">
					<span>ECX</span>
					<input type="number" min="0" step="any" bind:value={sellEcx} />
				</div>
			</label>

			<div class="trade-result">
				<div>
					<span>You receive</span>
					<strong>{formatUsd(sellUsdReceived)}</strong>
				</div>

				<div>
					<span>Effective price</span>
					<strong>{formatUsd(sellEffectivePrice)}</strong>
				</div>

				<div>
					<span>Price impact</span>
					<strong class:negative={sellPriceImpact > 0}>
						{format(sellPriceImpact, 4)}%
					</strong>
				</div>
			</div>

			<div class="after">
				<div class="after-title">Pool after trade</div>

				<div class="reserve-row">
					<span>ECX</span>
					<strong>{format(sellNewEcx)}</strong>
				</div>

				<div class="reserve-row">
					<span>USD</span>
					<strong>{formatUsd(sellNewUsd)}</strong>
				</div>
			</div>
		</div>

		<!-- POOL STATS -->

		<div class="stats">
			<div>
				<span>k</span>
				<strong>{format(k)}</strong>
			</div>

			<div>
				<span>Spot Price</span>
				<strong>{formatUsd(ecxPriceUsd)}</strong>
			</div>
		</div>

		<!-- FORMULA -->

		<div class="formula">
			<div>k = X × Y</div>
			<div>Price = Y ÷ X</div>
			<div>Buy: X₂ = k ÷ (Y + USD)</div>
			<div>Sell: Y₂ = k ÷ (X + ECX)</div>
		</div>
	</div>
</div>

<style>
	:global(*) {
		box-sizing: border-box;
	}

	:global(body) {
		margin: 0;
		font-family:
			Inter,
			ui-sans-serif,
			system-ui,
			-apple-system,
			BlinkMacSystemFont,
			"Segoe UI",
			sans-serif;
		background: #0b0d12;
		color: #f5f7fa;
	}

	.page {
		min-height: 100vh;
		display: grid;
		place-items: center;
		padding: 24px;
	}

	.card {
		width: min(520px, 100%);
		padding: 28px;
		border: 1px solid #252a35;
		border-radius: 20px;
		background: #12151c;
		box-shadow: 0 20px 60px rgba(0, 0, 0, 0.35);
	}

	h1 {
		margin: 0;
		font-size: 28px;
		letter-spacing: -0.5px;
	}

	.subtitle {
		margin: 6px 0 28px;
		color: #8d95a5;
		font-size: 14px;
	}

	.section-title {
		margin-bottom: 12px;
		color: #aeb5c2;
		font-size: 13px;
		font-weight: 600;
	}

	.inputs {
		display: grid;
		gap: 16px;
	}

	label {
		display: grid;
		gap: 8px;
	}

	label > span {
		font-size: 13px;
		color: #aeb5c2;
	}

	input {
		width: 100%;
		padding: 14px 16px;
		border: 1px solid #303644;
		border-radius: 10px;
		outline: none;
		background: #0d1016;
		color: #fff;
		font-size: 18px;
	}

	input:focus {
		border-color: #667cff;
		box-shadow: 0 0 0 3px rgba(102, 124, 255, 0.12);
	}

	.result {
		margin-top: 24px;
		padding: 24px;
		text-align: center;
		border-radius: 14px;
		background: linear-gradient(135deg, #1b2040, #15192b);
		border: 1px solid #30375f;
	}

	.price-label {
		color: #9da5bb;
		font-size: 13px;
		margin-bottom: 6px;
	}

	.price {
		font-size: 34px;
		font-weight: 700;
		letter-spacing: -1px;
	}

	.sub-price {
		margin-top: 8px;
		color: #858da2;
		font-size: 13px;
	}

	.trade {
		margin-top: 20px;
		padding: 20px;
		border: 1px solid #252a35;
		border-radius: 14px;
		background: #0d1016;
	}

	.trade-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 18px;
	}

	h2 {
		margin: 0;
		font-size: 18px;
	}

	.trade-header p {
		margin: 4px 0 0;
		color: #777f90;
		font-size: 12px;
	}

	.trade-icon {
		width: 34px;
		height: 34px;
		display: grid;
		place-items: center;
		border-radius: 9px;
		font-size: 18px;
		font-weight: bold;
	}

	.trade-icon.buy {
		background: rgba(55, 210, 130, 0.12);
		color: #45dc91;
	}

	.trade-icon.sell {
		background: rgba(255, 160, 70, 0.12);
		color: #ffa046;
	}

	.input-prefix {
		display: flex;
		align-items: center;
		border: 1px solid #303644;
		border-radius: 10px;
		background: #090c11;
		overflow: hidden;
	}

	.input-prefix > span {
		padding-left: 14px;
		color: #777f90;
		font-size: 16px;
		font-weight: 600;
	}

	.input-prefix input {
		border: 0;
		background: transparent;
	}

	.input-prefix input:focus {
		box-shadow: none;
	}

	.ecx-input > span {
		font-size: 12px;
	}

	.trade-result {
		display: grid;
		grid-template-columns: 1.3fr 1fr 1fr;
		gap: 10px;
		margin-top: 16px;
	}

	.trade-result > div {
		min-width: 0;
		padding: 12px;
		border-radius: 9px;
		background: #12151c;
		border: 1px solid #252a35;
	}

	.trade-result span {
		display: block;
		margin-bottom: 5px;
		color: #777f90;
		font-size: 11px;
	}

	.trade-result strong {
		display: block;
		font-size: 13px;
		overflow-wrap: anywhere;
	}

	.negative {
		color: #ff9d67;
	}

	.after {
		margin-top: 14px;
		padding-top: 14px;
		border-top: 1px solid #252a35;
	}

	.after-title {
		margin-bottom: 8px;
		color: #555d6d;
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
	}

	.reserve-row {
		display: flex;
		justify-content: space-between;
		padding: 4px 0;
		color: #858da2;
		font-size: 12px;
	}

	.reserve-row strong {
		color: #c8ced9;
	}

	.stats {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 12px;
		margin-top: 20px;
	}

	.stats > div {
		padding: 14px;
		border-radius: 10px;
		background: #0d1016;
		border: 1px solid #252a35;
	}

	.stats span {
		display: block;
		color: #777f90;
		font-size: 12px;
		margin-bottom: 5px;
	}

	.stats strong {
		font-size: 14px;
		overflow-wrap: anywhere;
	}

	.formula {
		margin-top: 20px;
		padding-top: 18px;
		border-top: 1px solid #252a35;
		color: #737b8c;
		font-family: monospace;
		font-size: 12px;
		line-height: 1.8;
	}

	@media (max-width: 480px) {
		.card {
			padding: 20px;
		}

		.trade-result {
			grid-template-columns: 1fr;
		}
	}
</style>
