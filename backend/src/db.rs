use anyhow::{Context, Result};
use bdk_wallet::bitcoin::Amount;
use turso_serverless::{Builder, Connection, Database};
use uuid::Uuid;

/// `bridge_orders` tracks each ECX->wECX bridge order from creation through payout: the ECX
/// amount deposited, the wECX amount owed, the Solana recipient, and the deposit/payout
/// signatures once they exist. No AMM/price columns - unlike the old BTC/ECX swap this replaces,
/// the bridge is a flat 1:1 peg (see `convert::ecx_sat_to_wecx_base_units`), so `amount_out_*` is
/// always a deterministic function of `amount_in_sat`, never a live-market quote that can drift.
///
/// `id` is a random (v4) UUID, generated in application code with the `uuid` crate rather than a
/// SQL `DEFAULT` expression, since there's no auth on order lookups - a sequential id would let
/// anyone enumerate other users' orders just by incrementing it.
const SCHEMA: &str = "
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS bridge_order_statuses (
    code TEXT PRIMARY KEY
);
INSERT OR IGNORE INTO bridge_order_statuses (code) VALUES
    ('pending'),
    ('deposit_seen'),
    ('deposit_confirmed'),
    ('paying_out'),
    ('paid_out'),
    ('expired'),
    ('failed'),
    ('refund_requested'),
    ('refunding'),
    ('refunded');

CREATE TABLE IF NOT EXISTS bridge_orders (
    id TEXT PRIMARY KEY NOT NULL,
    amount_in_sat INTEGER NOT NULL CHECK (amount_in_sat > 0),
    amount_out_base_units INTEGER NOT NULL CHECK (amount_out_base_units >= 0),
    deposit_address TEXT NOT NULL,
    solana_recipient TEXT NOT NULL,
    refund_address TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending' REFERENCES bridge_order_statuses (code),
    deposit_txid TEXT,
    payout_signature TEXT,
    refund_txid TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS bridge_orders_status_idx ON bridge_orders (status);

-- Each fresh ECX deposit address (see wallet::ChainWallet::fresh_address) is handed out to
-- exactly one order, so two concurrently pending orders never share a deposit target and become
-- ambiguous about which order a given deposit belongs to.
CREATE UNIQUE INDEX IF NOT EXISTS bridge_orders_deposit_address_uidx
    ON bridge_orders (deposit_address);

-- `pegout_orders` is the wECX->ECX mirror of `bridge_orders`. Solana has no analogue of a fresh
-- per-order receive address (every peg-out deposit lands in the same treasury ATA), so orders are
-- told apart by a `reference_pubkey` - a bare pubkey generated per order with no keypair kept,
-- included as an extra account in the customer's transfer per the Solana Pay convention (see
-- solana::SolanaWallet::find_reference_deposit). `depositor_owner` is extracted from that same
-- transaction (its wECX balance decreased) rather than asked of the customer, so - unlike
-- bridge_orders.refund_address - peg-out orders never need a separate add-a-refund-destination
-- step.
CREATE TABLE IF NOT EXISTS pegout_order_statuses (
    code TEXT PRIMARY KEY
);
INSERT OR IGNORE INTO pegout_order_statuses (code) VALUES
    ('pending'),
    ('deposit_seen'),
    ('deposit_confirmed'),
    ('paying_out'),
    ('paid_out'),
    ('expired'),
    ('failed'),
    ('refund_requested'),
    ('refunding'),
    ('refunded');

CREATE TABLE IF NOT EXISTS pegout_orders (
    id TEXT PRIMARY KEY NOT NULL,
    amount_in_base_units INTEGER NOT NULL CHECK (amount_in_base_units > 0),
    amount_out_sat INTEGER NOT NULL CHECK (amount_out_sat >= 0),
    ecx_recipient TEXT NOT NULL,
    reference_pubkey TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' REFERENCES pegout_order_statuses (code),
    deposit_signature TEXT,
    depositor_owner TEXT,
    payout_txid TEXT,
    refund_signature TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS pegout_orders_status_idx ON pegout_orders (status);

-- Mirrors `bridge_orders_deposit_address_uidx`: each reference key is generated fresh per order
-- (see server::create_pegout_order) and handed out to exactly one, so a deposit transaction can
-- never be ambiguous about which order it belongs to.
CREATE UNIQUE INDEX IF NOT EXISTS pegout_orders_reference_uidx
    ON pegout_orders (reference_pubkey);
";

/// SQLite integers are signed 64-bit; a bare `as i64` on a `u64` above `i64::MAX` silently goes
/// negative instead of failing, so every u64 -> column write goes through this.
fn to_i64(value: u64) -> Result<i64> {
    i64::try_from(value).context("amount does not fit in a signed 64-bit database column")
}

/// Connects directly to Turso Cloud via `TURSO_DATABASE_URL`/`TURSO_AUTH_TOKEN` and applies the
/// schema. Deliberately using `turso_serverless` rather than `turso`'s local-embedded-replica
/// mode: order data involves money, so every write must land durably on Turso Cloud immediately
/// rather than sitting in a local replica file until something remembers to call `db.push()`.
pub async fn open() -> Result<(Database, Connection)> {
    let database_url = std::env::var("TURSO_DATABASE_URL").context(
        "TURSO_DATABASE_URL must be set - see https://docs.turso.tech/sdk/rust/quickstart",
    )?;
    let auth_token = std::env::var("TURSO_AUTH_TOKEN").context(
        "TURSO_AUTH_TOKEN must be set - see https://docs.turso.tech/sdk/rust/quickstart",
    )?;

    let db = Builder::new_remote(database_url)
        .with_auth_token(auth_token)
        .build()
        .await
        .context("failed to open Turso database")?;
    let conn = db
        .connect()
        .context("failed to connect to Turso database")?;

    conn.execute_batch(SCHEMA)
        .await
        .context("failed to apply bridge_orders schema")?;

    Ok((db, conn))
}

/// Mirrors the `bridge_order_statuses` lookup table - kept as a Rust enum (rather than comparing
/// `Order.status` strings directly) so status-transition logic can't typo its way into a value the
/// DB's `FOREIGN KEY` would otherwise reject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    DepositSeen,
    DepositConfirmed,
    /// Transitional claim state: one poller has committed to sending the wECX payout but hasn't
    /// confirmed success yet. Exists so concurrent `GET /orders/{id}` polls can't both send - see
    /// `claim_for_payout`.
    PayingOut,
    PaidOut,
    Expired,
    Failed,
    /// Refund chosen (currently only by the poller, for a deposit whose repriced payout rounds to
    /// zero wECX - see `server::detect_deposit`), not yet sent. Mirrors what `DepositConfirmed`
    /// is to a payout.
    RefundRequested,
    /// Transitional claim state while a refund send is in flight - mirrors `PayingOut`. See
    /// `claim_for_refund`.
    Refunding,
    Refunded,
}

impl OrderStatus {
    pub fn label(self) -> &'static str {
        match self {
            OrderStatus::Pending => "pending",
            OrderStatus::DepositSeen => "deposit_seen",
            OrderStatus::DepositConfirmed => "deposit_confirmed",
            OrderStatus::PayingOut => "paying_out",
            OrderStatus::PaidOut => "paid_out",
            OrderStatus::Expired => "expired",
            OrderStatus::Failed => "failed",
            OrderStatus::RefundRequested => "refund_requested",
            OrderStatus::Refunding => "refunding",
            OrderStatus::Refunded => "refunded",
        }
    }

    pub fn from_label(label: &str) -> Result<OrderStatus> {
        Ok(match label {
            "pending" => OrderStatus::Pending,
            "deposit_seen" => OrderStatus::DepositSeen,
            "deposit_confirmed" => OrderStatus::DepositConfirmed,
            "paying_out" => OrderStatus::PayingOut,
            "paid_out" => OrderStatus::PaidOut,
            "expired" => OrderStatus::Expired,
            "failed" => OrderStatus::Failed,
            "refund_requested" => OrderStatus::RefundRequested,
            "refunding" => OrderStatus::Refunding,
            "refunded" => OrderStatus::Refunded,
            other => anyhow::bail!("unknown order status {other:?}"),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Order {
    pub id: String,
    pub amount_in_sat: i64,
    pub amount_out_base_units: i64,
    pub deposit_address: String,
    pub solana_recipient: String,
    /// Where the ECX deposit gets sent back if it can't be bridged (currently: a repriced deposit
    /// that rounds to zero wECX - see `server::detect_deposit`). `None` if the customer didn't
    /// provide one at creation. Stored as `TEXT NOT NULL DEFAULT ''` (empty string means "not
    /// provided") rather than a nullable column, mapped to `Option` at this boundary - see
    /// `row_to_order`/`insert_order`.
    pub refund_address: Option<String>,
    pub status: String,
    pub deposit_txid: Option<String>,
    pub payout_signature: Option<String>,
    pub refund_txid: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct NewOrder {
    pub amount_in_sat: u64,
    pub amount_out_base_units: u64,
    pub deposit_address: String,
    pub solana_recipient: String,
    pub refund_address: Option<String>,
}

const ORDER_COLUMNS: &str = "id, amount_in_sat, amount_out_base_units, deposit_address, \
    solana_recipient, refund_address, status, deposit_txid, payout_signature, refund_txid, \
    created_at, updated_at";

fn row_to_order(row: &turso_serverless::Row) -> Result<Order> {
    let refund_address: String = row.get(5)?;
    let refund_address = if refund_address.is_empty() {
        None
    } else {
        Some(refund_address)
    };

    Ok(Order {
        id: row.get(0)?,
        amount_in_sat: row.get(1)?,
        amount_out_base_units: row.get(2)?,
        deposit_address: row.get(3)?,
        solana_recipient: row.get(4)?,
        refund_address,
        status: row.get(6)?,
        deposit_txid: row.get(7)?,
        payout_signature: row.get(8)?,
        refund_txid: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

/// Inserts a new order with a random (v4) UUID id and returns the row as stored (including the
/// defaulted `status`/`created_at`/`updated_at` columns).
pub async fn insert_order(conn: &Connection, order: &NewOrder) -> Result<Order> {
    let id = Uuid::new_v4().to_string();
    let amount_in_sat = to_i64(order.amount_in_sat)?;
    let amount_out_base_units = to_i64(order.amount_out_base_units)?;

    conn.execute(
        "INSERT INTO bridge_orders \
            (id, amount_in_sat, amount_out_base_units, deposit_address, solana_recipient, \
             refund_address) \
         VALUES (?, ?, ?, ?, ?, ?)",
        (
            id.as_str(),
            amount_in_sat,
            amount_out_base_units,
            order.deposit_address.as_str(),
            order.solana_recipient.as_str(),
            order.refund_address.as_deref().unwrap_or(""),
        ),
    )
    .await
    .context("failed to insert bridge order")?;

    get_order(conn, &id)
        .await?
        .context("just-inserted bridge order not found")
}

pub async fn get_order(conn: &Connection, id: &str) -> Result<Option<Order>> {
    let mut rows = conn
        .query(
            &format!("SELECT {ORDER_COLUMNS} FROM bridge_orders WHERE id = ?"),
            (id,),
        )
        .await
        .context("failed to query bridge order")?;

    let Some(row) = rows.next().await.context("failed to read bridge order row")? else {
        return Ok(None);
    };

    Ok(Some(row_to_order(&row)?))
}

/// Rows for the background poller: orders still awaiting a deposit, payout, or refund - i.e.
/// everything `check_deposit` (deposit detection + `try_fulfill` + `try_refund`) or the
/// max-age expiry check in `poll_orders_once` can still act on. Includes `paying_out` so an order
/// whose payout outcome was ambiguous (see `server::try_resolve_payout`) gets resolved instead of
/// sitting there forever.
pub async fn list_active_orders(conn: &Connection) -> Result<Vec<Order>> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT {ORDER_COLUMNS} FROM bridge_orders \
                 WHERE status IN ('pending', 'deposit_seen', 'deposit_confirmed', \
                                   'paying_out', 'refund_requested')"
            ),
            (),
        )
        .await
        .context("failed to query active bridge orders")?;

    let mut orders = Vec::new();
    while let Some(row) = rows.next().await.context("failed to read bridge order row")? {
        orders.push(row_to_order(&row)?);
    }
    Ok(orders)
}

/// Number of orders still awaiting a deposit - used to cap how many deposit addresses (and
/// therefore poller sync work) unauthenticated `POST /orders` calls can pile up.
pub async fn count_pending_orders(conn: &Connection) -> Result<u64> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM bridge_orders WHERE status = ?",
            (OrderStatus::Pending.label(),),
        )
        .await
        .context("failed to count pending bridge orders")?;
    let count: i64 = match rows.next().await.context("failed to read pending order count")? {
        Some(row) => row.get(0)?,
        None => 0,
    };
    Ok(count.max(0) as u64)
}

/// Marks an order `expired`, but only while it's still `pending` or `deposit_seen` - a conditional
/// UPDATE so a poll that raced a deposit just being detected/confirmed can't clobber that newer
/// state. Returns `true` iff this call actually expired the row.
pub async fn expire_order(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE bridge_orders SET status = ?, updated_at = unixepoch() \
             WHERE id = ? AND status IN (?, ?)",
            (
                OrderStatus::Expired.label(),
                id,
                OrderStatus::Pending.label(),
                OrderStatus::DepositSeen.label(),
            ),
        )
        .await
        .context("failed to expire bridge order")?;
    Ok(rows_affected > 0)
}

/// `amount_in_sat` is written here too, not just `status`/`deposit_txid` - it's set to whatever
/// actually landed at the deposit address, which may differ from the amount quoted at order
/// creation. Keeping it in sync is what makes every downstream consumer of `amount_in_sat`
/// (refunds, `reserved_wecx_amount`) act on the real deposit rather than a stale quote.
///
/// `amount_out_base_units` is corrected in the same write, always paired with `amount_in_sat` so a
/// wrong-amount deposit's `status` transition and its repriced payout land atomically.
///
/// Conditional on the order still being `pending`/`deposit_seen`. `server::detect_deposit` decides
/// what to write from an in-memory snapshot that can be arbitrarily stale (a concurrent
/// `GET /orders/{id}`, or the poller working through a batch, may have already advanced the order);
/// an unconditional write here used to be able to drag a `paying_out`/`paid_out` order back to
/// `deposit_confirmed`, which `claim_for_payout` would then happily claim a second time. Returns
/// `true` iff this call won the write - on `false`, the caller must re-read the order rather than
/// act on its snapshot.
pub async fn update_deposit(
    conn: &Connection,
    id: &str,
    status: OrderStatus,
    deposit_txid: &str,
    amount_in_sat: Amount,
    amount_out_base_units: u64,
) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE bridge_orders SET status = ?, deposit_txid = ?, amount_in_sat = ?, \
             amount_out_base_units = ?, updated_at = unixepoch() \
             WHERE id = ? AND status IN (?, ?)",
            (
                status.label(),
                deposit_txid,
                to_i64(amount_in_sat.to_sat())?,
                to_i64(amount_out_base_units)?,
                id,
                OrderStatus::Pending.label(),
                OrderStatus::DepositSeen.label(),
            ),
        )
        .await
        .context("failed to update bridge order deposit status")?;
    Ok(rows_affected > 0)
}

/// Sum of `amount_out_base_units` for orders not yet paid out but already a real claim on the
/// treasury's wECX balance: `deposit_seen` (a deposit is visible on-chain, even if unconfirmed)
/// through `paying_out` (claimed but not yet confirmed sent). Excludes `pending` (no deposit yet,
/// nothing real to protect against) and any terminal status. Used to keep `create_order` and
/// `detect_deposit`'s repricing from promising wECX that's already committed to other in-flight
/// orders.
pub async fn reserved_wecx_amount(conn: &Connection) -> Result<u64> {
    let mut rows = conn
        .query(
            "SELECT COALESCE(SUM(amount_out_base_units), 0) FROM bridge_orders \
             WHERE status IN (?, ?, ?)",
            (
                OrderStatus::DepositSeen.label(),
                OrderStatus::DepositConfirmed.label(),
                OrderStatus::PayingOut.label(),
            ),
        )
        .await
        .context("failed to query reserved wECX amount")?;
    let sat: i64 = match rows.next().await.context("failed to read reserved wECX amount row")? {
        Some(row) => row.get(0)?,
        None => 0,
    };
    Ok(sat as u64)
}

/// Atomically claims an order for payout by transitioning `deposit_confirmed -> paying_out`.
/// Returns `true` iff this call won the claim (i.e. the row was still `deposit_confirmed`) - the
/// caller should only proceed to send funds when this is `true`. Concurrent `GET /orders/{id}`
/// polls calling this racily is exactly what this guards against.
pub async fn claim_for_payout(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE bridge_orders SET status = ?, updated_at = unixepoch() \
             WHERE id = ? AND status = ?",
            (
                OrderStatus::PayingOut.label(),
                id,
                OrderStatus::DepositConfirmed.label(),
            ),
        )
        .await
        .context("failed to claim bridge order for payout")?;
    Ok(rows_affected > 0)
}

/// Records the signature of the payout transaction *before* it is broadcast, while the order is
/// still `paying_out`. The signature is deterministic once the transaction is signed, so writing it
/// first means an ambiguous send outcome (RPC timeout mid-confirmation, process crash) leaves a
/// durable handle to look the transaction up by, instead of a bare `paying_out` row nobody can
/// distinguish from "never sent" - see `server::try_resolve_payout`. Also bumps `updated_at`, which
/// that resolver uses as the "attempt started" time. Returns `true` iff the order was still
/// `paying_out`.
pub async fn record_payout_signature(conn: &Connection, id: &str, payout_signature: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE bridge_orders SET payout_signature = ?, updated_at = unixepoch() \
             WHERE id = ? AND status = ?",
            (payout_signature, id, OrderStatus::PayingOut.label()),
        )
        .await
        .context("failed to record bridge order payout signature")?;
    Ok(rows_affected > 0)
}

/// Marks a claimed order as paid out with its Solana payout transaction signature. Conditional on
/// the order still being `paying_out`; returns `true` iff this call made the transition.
pub async fn update_payout(conn: &Connection, id: &str, payout_signature: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE bridge_orders SET status = ?, payout_signature = ?, updated_at = unixepoch() \
             WHERE id = ? AND status = ?",
            (
                OrderStatus::PaidOut.label(),
                payout_signature,
                id,
                OrderStatus::PayingOut.label(),
            ),
        )
        .await
        .context("failed to update bridge order payout")?;
    Ok(rows_affected > 0)
}

/// Reverts a `paying_out` claim back to `deposit_confirmed` so the next poll retries, and clears
/// any recorded payout signature. Only for outcomes where the payout is *known* not to have
/// happened (never broadcast, landed and failed on-chain, or its blockhash expired unconfirmed) -
/// never for an ambiguous send error, where the transaction may still land.
pub async fn revert_claim(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE bridge_orders SET status = ?, payout_signature = NULL, updated_at = unixepoch() \
         WHERE id = ? AND status = ?",
        (
            OrderStatus::DepositConfirmed.label(),
            id,
            OrderStatus::PayingOut.label(),
        ),
    )
    .await
    .context("failed to revert bridge order payout claim")?;
    Ok(())
}

/// Routes a `deposit_confirmed` order to a refund instead of a payout - currently only used when a
/// deposit reprices to zero wECX (see `server::detect_deposit`). Returns `true` iff this call won
/// the transition.
pub async fn request_refund(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE bridge_orders SET status = ?, updated_at = unixepoch() \
             WHERE id = ? AND status = ?",
            (
                OrderStatus::RefundRequested.label(),
                id,
                OrderStatus::DepositConfirmed.label(),
            ),
        )
        .await
        .context("failed to request bridge order refund")?;
    Ok(rows_affected > 0)
}

/// Sets `refund_address` for an order that doesn't have one yet - conditional on the column still
/// being empty (see `row_to_order`'s `''` = "not provided" convention) so this can't clobber an
/// address already on file, whether from order creation or a prior call here racing this one.
/// Returns `true` iff this call won the write.
pub async fn set_refund_address(conn: &Connection, id: &str, refund_address: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE bridge_orders SET refund_address = ?, updated_at = unixepoch() \
             WHERE id = ? AND refund_address = ''",
            (refund_address, id),
        )
        .await
        .context("failed to set bridge order refund address")?;
    Ok(rows_affected > 0)
}

/// Atomically claims an order for refund by transitioning `refund_requested -> refunding`.
/// Mirrors `claim_for_payout` - returns `true` iff this call won the claim, guarding against two
/// concurrent polls both sending the refund.
pub async fn claim_for_refund(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE bridge_orders SET status = ?, updated_at = unixepoch() \
             WHERE id = ? AND status = ?",
            (
                OrderStatus::Refunding.label(),
                id,
                OrderStatus::RefundRequested.label(),
            ),
        )
        .await
        .context("failed to claim bridge order for refund")?;
    Ok(rows_affected > 0)
}

/// Marks a claimed order as refunded with its ECX refund txid.
pub async fn update_refund(conn: &Connection, id: &str, refund_txid: &str) -> Result<()> {
    conn.execute(
        "UPDATE bridge_orders SET status = ?, refund_txid = ?, updated_at = unixepoch() WHERE id = ?",
        (OrderStatus::Refunded.label(), refund_txid, id),
    )
    .await
    .context("failed to update bridge order refund")?;
    Ok(())
}

/// Reverts a `refunding` claim back to `refund_requested` so the next poll retries - used when the
/// refund send fails, or the live-payout safety gate blocks it (a refund is just as real an
/// automatic outbound send as a payout).
pub async fn revert_refund_claim(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE bridge_orders SET status = ?, updated_at = unixepoch() WHERE id = ? AND status = ?",
        (
            OrderStatus::RefundRequested.label(),
            id,
            OrderStatus::Refunding.label(),
        ),
    )
    .await
    .context("failed to revert bridge order refund claim")?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// pegout_orders: the wECX->ECX mirror of everything above. Same state-machine shape as
// `bridge_orders` (see `PegoutOrderStatus`), with the roles of the two chains swapped: the
// deposit lands on Solana (matched via `reference_pubkey`, see
// `solana::SolanaWallet::find_reference_deposit`, rather than a fresh per-order address) and the
// payout is an ECX send; a refund (if the deposit can't be bridged) sends wECX back on Solana
// instead of ECX. See `server::check_pegout_deposit` for the orchestration this supports.
// ---------------------------------------------------------------------------------------------

/// Mirrors `OrderStatus` for the peg-out direction - see that type's doc comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PegoutOrderStatus {
    Pending,
    DepositSeen,
    DepositConfirmed,
    /// Transitional claim state for an in-flight ECX payout - mirrors `OrderStatus::PayingOut`.
    /// Unlike the Solana-side payout in `bridge_orders`, an ECX send in this codebase is a single
    /// synchronous sign-and-broadcast call (see `wallet::ChainWallet::send`) with no ambiguous
    /// "unknown outcome" case to resolve later, so there's no `paying_out`-specific resolver
    /// mirroring `server::try_resolve_payout` here.
    PayingOut,
    PaidOut,
    Expired,
    Failed,
    /// Deposit confirmed but not worth paying out (see `server::pegout_payout_viable`) - routed to
    /// a wECX refund using the `depositor_owner` extracted from the deposit transaction itself, so
    /// (unlike `bridge_orders.refund_address`) no customer input is needed to reach this state.
    RefundRequested,
    /// Transitional claim state while a wECX refund send is in flight - mirrors
    /// `OrderStatus::Refunding`. This one *is* two-phase like the peg-in payout (see
    /// `record_pegout_refund_signature`), because it's a Solana send with the same ambiguous-RPC-
    /// outcome problem `server::try_resolve_payout` exists for.
    Refunding,
    Refunded,
}

impl PegoutOrderStatus {
    pub fn label(self) -> &'static str {
        match self {
            PegoutOrderStatus::Pending => "pending",
            PegoutOrderStatus::DepositSeen => "deposit_seen",
            PegoutOrderStatus::DepositConfirmed => "deposit_confirmed",
            PegoutOrderStatus::PayingOut => "paying_out",
            PegoutOrderStatus::PaidOut => "paid_out",
            PegoutOrderStatus::Expired => "expired",
            PegoutOrderStatus::Failed => "failed",
            PegoutOrderStatus::RefundRequested => "refund_requested",
            PegoutOrderStatus::Refunding => "refunding",
            PegoutOrderStatus::Refunded => "refunded",
        }
    }

    pub fn from_label(label: &str) -> Result<PegoutOrderStatus> {
        Ok(match label {
            "pending" => PegoutOrderStatus::Pending,
            "deposit_seen" => PegoutOrderStatus::DepositSeen,
            "deposit_confirmed" => PegoutOrderStatus::DepositConfirmed,
            "paying_out" => PegoutOrderStatus::PayingOut,
            "paid_out" => PegoutOrderStatus::PaidOut,
            "expired" => PegoutOrderStatus::Expired,
            "failed" => PegoutOrderStatus::Failed,
            "refund_requested" => PegoutOrderStatus::RefundRequested,
            "refunding" => PegoutOrderStatus::Refunding,
            "refunded" => PegoutOrderStatus::Refunded,
            other => anyhow::bail!("unknown pegout order status {other:?}"),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PegoutOrder {
    pub id: String,
    pub amount_in_base_units: i64,
    pub amount_out_sat: i64,
    pub ecx_recipient: String,
    pub reference_pubkey: String,
    pub status: String,
    pub deposit_signature: Option<String>,
    /// Solana wallet owner pubkey the deposit's wECX balance decrease was attributed to (see
    /// `solana::SolanaWallet::find_reference_deposit`) - where a refund goes if this deposit can't
    /// be bridged. `None` if it couldn't be determined from the transaction (e.g. an exotic
    /// multisig source) - such an order can't be auto-refunded, only reviewed manually.
    pub depositor_owner: Option<String>,
    pub payout_txid: Option<String>,
    pub refund_signature: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct NewPegoutOrder {
    pub amount_in_base_units: u64,
    pub amount_out_sat: u64,
    pub ecx_recipient: String,
    pub reference_pubkey: String,
}

const PEGOUT_ORDER_COLUMNS: &str = "id, amount_in_base_units, amount_out_sat, ecx_recipient, \
    reference_pubkey, status, deposit_signature, depositor_owner, payout_txid, refund_signature, \
    created_at, updated_at";

fn row_to_pegout_order(row: &turso_serverless::Row) -> Result<PegoutOrder> {
    Ok(PegoutOrder {
        id: row.get(0)?,
        amount_in_base_units: row.get(1)?,
        amount_out_sat: row.get(2)?,
        ecx_recipient: row.get(3)?,
        reference_pubkey: row.get(4)?,
        status: row.get(5)?,
        deposit_signature: row.get(6)?,
        depositor_owner: row.get(7)?,
        payout_txid: row.get(8)?,
        refund_signature: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

pub async fn insert_pegout_order(conn: &Connection, order: &NewPegoutOrder) -> Result<PegoutOrder> {
    let id = Uuid::new_v4().to_string();
    let amount_in_base_units = to_i64(order.amount_in_base_units)?;
    let amount_out_sat = to_i64(order.amount_out_sat)?;

    conn.execute(
        "INSERT INTO pegout_orders \
            (id, amount_in_base_units, amount_out_sat, ecx_recipient, reference_pubkey) \
         VALUES (?, ?, ?, ?, ?)",
        (
            id.as_str(),
            amount_in_base_units,
            amount_out_sat,
            order.ecx_recipient.as_str(),
            order.reference_pubkey.as_str(),
        ),
    )
    .await
    .context("failed to insert pegout order")?;

    get_pegout_order(conn, &id)
        .await?
        .context("just-inserted pegout order not found")
}

pub async fn get_pegout_order(conn: &Connection, id: &str) -> Result<Option<PegoutOrder>> {
    let mut rows = conn
        .query(
            &format!("SELECT {PEGOUT_ORDER_COLUMNS} FROM pegout_orders WHERE id = ?"),
            (id,),
        )
        .await
        .context("failed to query pegout order")?;

    let Some(row) = rows.next().await.context("failed to read pegout order row")? else {
        return Ok(None);
    };

    Ok(Some(row_to_pegout_order(&row)?))
}

/// Rows for the background poller - mirrors `list_active_orders`.
pub async fn list_active_pegout_orders(conn: &Connection) -> Result<Vec<PegoutOrder>> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT {PEGOUT_ORDER_COLUMNS} FROM pegout_orders \
                 WHERE status IN ('pending', 'deposit_seen', 'deposit_confirmed', \
                                   'paying_out', 'refund_requested', 'refunding')"
            ),
            (),
        )
        .await
        .context("failed to query active pegout orders")?;

    let mut orders = Vec::new();
    while let Some(row) = rows.next().await.context("failed to read pegout order row")? {
        orders.push(row_to_pegout_order(&row)?);
    }
    Ok(orders)
}

/// Mirrors `count_pending_orders` - caps how many reference keys (and therefore poller RPC calls)
/// unauthenticated `POST /pegout/orders` calls can pile up.
pub async fn count_pending_pegout_orders(conn: &Connection) -> Result<u64> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM pegout_orders WHERE status = ?",
            (PegoutOrderStatus::Pending.label(),),
        )
        .await
        .context("failed to count pending pegout orders")?;
    let count: i64 = match rows.next().await.context("failed to read pending pegout order count")? {
        Some(row) => row.get(0)?,
        None => 0,
    };
    Ok(count.max(0) as u64)
}

/// Mirrors `expire_order`.
pub async fn expire_pegout_order(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE pegout_orders SET status = ?, updated_at = unixepoch() \
             WHERE id = ? AND status IN (?, ?)",
            (
                PegoutOrderStatus::Expired.label(),
                id,
                PegoutOrderStatus::Pending.label(),
                PegoutOrderStatus::DepositSeen.label(),
            ),
        )
        .await
        .context("failed to expire pegout order")?;
    Ok(rows_affected > 0)
}

/// Mirrors `reserved_wecx_amount`, but for the ECX side: `amount_out_sat` already committed to
/// peg-out orders whose wECX deposit has landed but hasn't been paid out in ECX yet. Used to keep
/// `create_pegout_order` from promising more ECX than the treasury wallet can actually cover -
/// unlike a peg-in refund (which just returns ECX that was itself the deposit, net-zero on the
/// wallet's balance), a peg-out payout draws down the treasury's ECX balance for real.
pub async fn pegout_reserved_ecx_amount(conn: &Connection) -> Result<u64> {
    let mut rows = conn
        .query(
            "SELECT COALESCE(SUM(amount_out_sat), 0) FROM pegout_orders WHERE status IN (?, ?, ?)",
            (
                PegoutOrderStatus::DepositSeen.label(),
                PegoutOrderStatus::DepositConfirmed.label(),
                PegoutOrderStatus::PayingOut.label(),
            ),
        )
        .await
        .context("failed to query reserved ECX amount")?;
    let sat: i64 = match rows.next().await.context("failed to read reserved ECX amount row")? {
        Some(row) => row.get(0)?,
        None => 0,
    };
    Ok(sat as u64)
}

/// Records a newly-found deposit (see `solana::SolanaWallet::find_reference_deposit`) and reprices
/// `amount_out_sat` to what actually landed, exactly like `update_deposit` does for the ECX side -
/// the customer's wallet may not have sent precisely the quoted `amount_in_base_units`. Conditional
/// on the order still being `pending`, for the same staleness reason `update_deposit` guards
/// against. Returns `true` iff this call won the write.
pub async fn record_pegout_deposit(
    conn: &Connection,
    id: &str,
    status: PegoutOrderStatus,
    deposit_signature: &str,
    depositor_owner: Option<&str>,
    amount_in_base_units: u64,
    amount_out_sat: u64,
) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE pegout_orders SET status = ?, deposit_signature = ?, depositor_owner = ?, \
             amount_in_base_units = ?, amount_out_sat = ?, updated_at = unixepoch() \
             WHERE id = ? AND status = ?",
            (
                status.label(),
                deposit_signature,
                depositor_owner,
                to_i64(amount_in_base_units)?,
                to_i64(amount_out_sat)?,
                id,
                PegoutOrderStatus::Pending.label(),
            ),
        )
        .await
        .context("failed to record pegout order deposit")?;
    Ok(rows_affected > 0)
}

/// Flips a previously-seen deposit to confirmed once its transaction reaches `finalized` (see
/// `solana::SolanaWallet::signature_finalized`) - the peg-out mirror of the confirmation-depth
/// check `detect_deposit` does on the ECX side. Conditional on the order still being
/// `deposit_seen`. Returns `true` iff this call won the write.
pub async fn confirm_pegout_deposit(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE pegout_orders SET status = ?, updated_at = unixepoch() WHERE id = ? AND status = ?",
            (
                PegoutOrderStatus::DepositConfirmed.label(),
                id,
                PegoutOrderStatus::DepositSeen.label(),
            ),
        )
        .await
        .context("failed to confirm pegout order deposit")?;
    Ok(rows_affected > 0)
}

/// Mirrors `claim_for_payout`.
pub async fn claim_for_pegout_payout(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE pegout_orders SET status = ?, updated_at = unixepoch() WHERE id = ? AND status = ?",
            (
                PegoutOrderStatus::PayingOut.label(),
                id,
                PegoutOrderStatus::DepositConfirmed.label(),
            ),
        )
        .await
        .context("failed to claim pegout order for payout")?;
    Ok(rows_affected > 0)
}

/// Marks a claimed order paid out with its ECX payout txid. Unlike `update_payout` on the peg-in
/// side, there's no separately-recorded-before-broadcast signature to match against: an ECX send
/// (see `wallet::ChainWallet::send`) only ever returns a txid once it has actually broadcast, so
/// there's no ambiguous "did it send" case here to resolve later.
pub async fn update_pegout_payout(conn: &Connection, id: &str, payout_txid: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE pegout_orders SET status = ?, payout_txid = ?, updated_at = unixepoch() \
             WHERE id = ? AND status = ?",
            (
                PegoutOrderStatus::PaidOut.label(),
                payout_txid,
                id,
                PegoutOrderStatus::PayingOut.label(),
            ),
        )
        .await
        .context("failed to update pegout order payout")?;
    Ok(rows_affected > 0)
}

/// Mirrors `revert_claim`.
pub async fn revert_pegout_claim(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE pegout_orders SET status = ?, updated_at = unixepoch() WHERE id = ? AND status = ?",
        (
            PegoutOrderStatus::DepositConfirmed.label(),
            id,
            PegoutOrderStatus::PayingOut.label(),
        ),
    )
    .await
    .context("failed to revert pegout order payout claim")?;
    Ok(())
}

/// Mirrors `request_refund`.
pub async fn request_pegout_refund(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE pegout_orders SET status = ?, updated_at = unixepoch() WHERE id = ? AND status = ?",
            (
                PegoutOrderStatus::RefundRequested.label(),
                id,
                PegoutOrderStatus::DepositConfirmed.label(),
            ),
        )
        .await
        .context("failed to request pegout order refund")?;
    Ok(rows_affected > 0)
}

/// Mirrors `claim_for_refund`.
pub async fn claim_for_pegout_refund(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE pegout_orders SET status = ?, updated_at = unixepoch() WHERE id = ? AND status = ?",
            (
                PegoutOrderStatus::Refunding.label(),
                id,
                PegoutOrderStatus::RefundRequested.label(),
            ),
        )
        .await
        .context("failed to claim pegout order for refund")?;
    Ok(rows_affected > 0)
}

/// Mirrors `record_payout_signature`: durable *before* broadcast, since this refund is a Solana
/// send with the same ambiguous-RPC-outcome problem the peg-in payout has (see
/// `server::try_resolve_pegout_refund`).
pub async fn record_pegout_refund_signature(conn: &Connection, id: &str, refund_signature: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE pegout_orders SET refund_signature = ?, updated_at = unixepoch() \
             WHERE id = ? AND status = ?",
            (refund_signature, id, PegoutOrderStatus::Refunding.label()),
        )
        .await
        .context("failed to record pegout order refund signature")?;
    Ok(rows_affected > 0)
}

/// Marks a claimed refund as sent. Conditional on the order still being `refunding`.
pub async fn update_pegout_refund(conn: &Connection, id: &str, refund_signature: &str) -> Result<bool> {
    let rows_affected = conn
        .execute(
            "UPDATE pegout_orders SET status = ?, refund_signature = ?, updated_at = unixepoch() \
             WHERE id = ? AND status = ?",
            (
                PegoutOrderStatus::Refunded.label(),
                refund_signature,
                id,
                PegoutOrderStatus::Refunding.label(),
            ),
        )
        .await
        .context("failed to update pegout order refund")?;
    Ok(rows_affected > 0)
}

/// Mirrors `revert_refund_claim`.
pub async fn revert_pegout_refund_claim(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE pegout_orders SET status = ?, refund_signature = NULL, updated_at = unixepoch() \
         WHERE id = ? AND status = ?",
        (
            PegoutOrderStatus::RefundRequested.label(),
            id,
            PegoutOrderStatus::Refunding.label(),
        ),
    )
    .await
    .context("failed to revert pegout order refund claim")?;
    Ok(())
}
