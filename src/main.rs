// cbor-token — CBOR-Web Autonomous Token Server
// Hold-to-access model: agent holds ≥ 1 CBORW token → access to L1 content
// Runs standalone or alongside cbor-server on MiniPC/MacPro.
// This is the prototype ledger. Migrate to ERC-20 on mainnet when ready.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ── CLI ──

#[derive(Parser)]
#[command(name = "cbor-token", version, about = "CBOR-Web Autonomous Token Server")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:3002")]
    listen: String,

    #[arg(long, default_value = "ledger.json")]
    ledger_file: String,

    /// Initial supply in whole tokens
    #[arg(long, default_value = "100000000")]
    total_supply: u64,

    /// Founder address (hex eth-style: 0x...)
    #[arg(long)]
    founder: Option<String>,

    /// Minimum tokens to hold for access
    #[arg(long, default_value = "1")]
    min_hold: u64,

    /// Enable signature verification (requires eth wallet)
    #[arg(long, default_value = "false")]
    verify_signatures: bool,
}

// ── Types ──

type Address = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Ledger {
    total_supply: u64,
    balances: HashMap<Address, u64>,
}

impl Ledger {
    fn new(total_supply: u64) -> Self {
        Self { total_supply, balances: HashMap::new() }
    }
}

#[derive(Debug, Clone)]
struct AppState {
    ledger: Arc<RwLock<Ledger>>,
    ledger_path: String,
    min_hold: u64,
    verify_signatures: bool,
}

// ── Helpers ──

/// Verify an Ethereum wallet signature (EIP-191 personal_sign style) via real
/// ecrecover (k256). Previously a placeholder that accepted ANY well-formed
/// signature — critical flaw found by the 2026-09-01 external audit: with
/// verify_signatures=true, POST /transfer let anyone drain any address.
fn verify_wallet_signature(address: &str, message: &str, sig_hex: &str) -> bool {
    use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
    use k256::elliptic_curve::sec1::ToSec1Point;
    use k256::PublicKey;

    let addr = address.strip_prefix("0x").or_else(|| address.strip_prefix("0X")).unwrap_or(address);
    if addr.len() != 40 || hex::decode(addr).is_err() {
        return false;
    }
    let sig_hex = sig_hex.strip_prefix("0x").or_else(|| sig_hex.strip_prefix("0X")).unwrap_or(sig_hex);
    let sig_bytes = match hex::decode(sig_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };
    if sig_bytes.len() != 65 {
        return false; // r||s||v exactly
    }
    let v = sig_bytes[64];
    if v != 27 && v != 28 {
        return false;
    }
    let signature = match Signature::from_slice(&sig_bytes[..64]) {
        Ok(s) => s,
        Err(_) => return false,
    };
    // EIP-191 message hash
    let eip191_message = format!(
        "\x19Ethereum Signed Message:\n{}{}",
        message.len(),
        message
    );
    let hash = Keccak256::digest(eip191_message.as_bytes());
    let recovery_id = match RecoveryId::from_byte(v - 27) {
        Some(r) => r,
        None => return false,
    };
    let recovered_key = match VerifyingKey::recover_from_prehash(&hash, &signature, recovery_id) {
        Ok(k) => k,
        Err(_) => return false,
    };
    // Derive the Ethereum address from the recovered public key
    let public_key = PublicKey::from(&recovered_key);
    let public_key_bytes = public_key.to_sec1_point(false).as_bytes().to_vec();
    let addr_hash = Keccak256::digest(&public_key_bytes[1..]);
    let recovered = format!("0x{}", hex::encode(&addr_hash[12..]));
    recovered.to_lowercase() == address.to_lowercase()
}

/// Generate a semi-deterministic address from a string (for testing without wallets).
fn pseudo_address(seed: &str) -> String {
    let hash = Keccak256::digest(seed.as_bytes());
    format!("0x{}", hex::encode(&hash[..20]))
}

fn trunc(s: &str) -> String {
    if s.len() <= 12 { s.to_string() } else { format!("{}...", &s[..10]) }
}

// ── API Endpoints ──

/// GET /total-supply
async fn total_supply(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let ledger = state.ledger.read().await;
    Json(serde_json::json!({
        "total_supply": ledger.total_supply,
        "symbol": "CBORW",
        "decimals": 0,
        "min_hold_for_access": state.min_hold,
    }))
}

/// GET /balance?address=0x...
async fn balance(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Response {
    let address = params.get("address").cloned().unwrap_or_default();
    let ledger = state.ledger.read().await;
    let bal = ledger.balances.get(&address).copied().unwrap_or(0);
    let has_access = bal >= state.min_hold;
    Json(serde_json::json!({
        "address": address,
        "balance": bal,
        "has_access": has_access,
    })).into_response()
}

/// GET /verify-access?address=...&sig=...&msg=...
async fn verify_access(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Response {
    let address = params.get("address").cloned().unwrap_or_default();
    let ledger = state.ledger.read().await;
    let bal = ledger.balances.get(&address).copied().unwrap_or(0);
    let has_access = bal >= state.min_hold;

    if state.verify_signatures {
        let sig = params.get("sig").cloned().unwrap_or_default();
        let msg = params.get("msg").cloned().unwrap_or_default();
        if !verify_wallet_signature(&address, &msg, &sig) {
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({
                "error": "invalid signature",
                "has_access": false,
            }))).into_response();
        }
    }

    Json(serde_json::json!({
        "address": address,
        "balance": bal,
        "has_access": has_access,
        "min_hold": state.min_hold,
    })).into_response()
}

/// POST /transfer
#[derive(Deserialize)]
struct TransferRequest {
    from: String,
    to: String,
    amount: u64,
    #[serde(default)]
    sig: String,
}

async fn transfer(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TransferRequest>,
) -> Response {
    let mut ledger = state.ledger.write().await;

    // Check sender balance
    let sender_bal = ledger.balances.get(&req.from).copied().unwrap_or(0);
    if sender_bal < req.amount {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "insufficient balance",
            "balance": sender_bal,
            "required": req.amount,
        }))).into_response();
    }

    if state.verify_signatures {
        let _msg = format!("transfer {} {} {}", req.from, req.to, req.amount);
        if !verify_wallet_signature(&req.from, &_msg, &req.sig) {
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({
                "error": "invalid signature",
            }))).into_response();
        }
    }

    // Execute transfer
    *ledger.balances.entry(req.from.clone()).or_insert(0) -= req.amount;
    *ledger.balances.entry(req.to.clone()).or_insert(0) += req.amount;

    // Persist
    if let Err(e) = save_ledger(&state.ledger_path, &ledger).await {
        tracing::error!("Failed to save ledger: {}", e);
    }

    tracing::info!("Transfer: {} → {} {} CBORW", trunc(&req.from), trunc(&req.to), req.amount);

    Json(serde_json::json!({
        "success": true,
        "from_balance": ledger.balances.get(&req.from).copied().unwrap_or(0),
        "to_balance": ledger.balances.get(&req.to).copied().unwrap_or(0),
    })).into_response()
}

/// GET /holders — list top token holders
async fn holders(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let ledger = state.ledger.read().await;
    let mut holders: Vec<_> = ledger.balances.iter()
        .filter(|(_, &b)| b > 0)
        .map(|(a, &b)| (a.clone(), b))
        .collect();
    holders.sort_by(|a, b| b.1.cmp(&a.1));
    let top: Vec<_> = holders.into_iter().take(100)
        .map(|(addr, bal)| serde_json::json!({"address": addr, "balance": bal}))
        .collect();
    Json(serde_json::json!({
        "total_holders": ledger.balances.iter().filter(|(_, &b)| b > 0).count(),
        "top": top,
    }))
}

/// GET /stats
async fn stats(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let ledger = state.ledger.read().await;
    let circulating = ledger.total_supply.saturating_sub(
        ledger.balances.get("0x0000000000000000000000000000000000000000").copied().unwrap_or(0)
    );
    let holders_count = ledger.balances.iter().filter(|(_, &b)| b > 0).count();
    Json(serde_json::json!({
        "total_supply": ledger.total_supply,
        "circulating": circulating,
        "holders": holders_count,
        "symbol": "CBORW",
    }))
}

// ── Persistence ──

async fn save_ledger(path: &str, ledger: &Ledger) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_vec_pretty(ledger)?;
    tokio::fs::write(path, &json).await?;
    Ok(())
}

async fn load_ledger(path: &str, total_supply: u64) -> Ledger {
    match tokio::fs::read_to_string(path).await {
        Ok(data) => serde_json::from_str(&data).unwrap_or_else(|_| Ledger::new(total_supply)),
        Err(_) => Ledger::new(total_supply),
    }
}

// ── Main ──

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let mut ledger = load_ledger(&cli.ledger_file, cli.total_supply).await;

    // Allocate to founder if specified and not already allocated
    if let Some(ref founder) = cli.founder {
        if ledger.balances.get(founder).copied().unwrap_or(0) == 0 {
            ledger.balances.insert(founder.clone(), cli.total_supply);
            tracing::info!("Allocated {} CBORW to founder {}", cli.total_supply, trunc(&founder));
            save_ledger(&cli.ledger_file, &ledger).await
                .expect("Failed to save initial ledger");
        }
    }

    let state = Arc::new(AppState {
        ledger: Arc::new(RwLock::new(ledger)),
        ledger_path: cli.ledger_file,
        min_hold: cli.min_hold,
        verify_signatures: cli.verify_signatures,
    });

    let app = Router::new()
        .route("/total-supply", get(total_supply))
        .route("/balance", get(balance))
        .route("/verify-access", get(verify_access))
        .route("/transfer", post(transfer))
        .route("/holders", get(holders))
        .route("/stats", get(stats))
        .with_state(state);

    tracing::info!("CBOR-Web Token Server v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Total supply: {} CBORW", cli.total_supply);
    tracing::info!("Min hold for access: {} CBORW", cli.min_hold);
    tracing::info!("Sig verification: {}", if cli.verify_signatures { "on" } else { "off (prototype mode)" });
    tracing::info!("Listening on http://{}", cli.listen);

    let listener = tokio::net::TcpListener::bind(&cli.listen)
        .await
        .expect("Failed to bind");
    axum::serve(listener, app).await.expect("Server error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::signature::Signer;
    use k256::ecdsa::{Signature, SigningKey};
    use sha3::{Digest, Keccak256};

    fn addr_from_key(key: &SigningKey) -> String {
        use k256::elliptic_curve::sec1::ToSec1Point;
        use k256::PublicKey;
        let pk = PublicKey::from(key.verifying_key());
        let bytes = pk.to_sec1_point(false).as_bytes().to_vec();
        let h = Keccak256::digest(&bytes[1..]);
        format!("0x{}", hex::encode(&h[12..]))
    }

    fn sign_eip191(key: &SigningKey, message: &str) -> String {
        let full = format!("\x19Ethereum Signed Message:\n{}{}", message.len(), message);
        let hash = Keccak256::digest(full.as_bytes());
        let (sig, rid) = key.sign_prehash_recoverable(&hash);
        let rb = sig.to_bytes();
        let v = rid.to_byte() + 27;
        let mut out = Vec::with_capacity(65);
        out.extend_from_slice(&rb);
        out.push(v);
        format!("0x{}", hex::encode(out))
    }

    #[test]
    fn test_verify_wallet_signature_real_ecrecover() {
        let key = SigningKey::from_slice(&[7u8; 32]).unwrap();
        let addr = addr_from_key(&key);
        let msg = "transfer 0xabc 0xdef 100";
        let sig = sign_eip191(&key, msg);

        assert!(verify_wallet_signature(&addr, msg, &sig), "signature valide rejetée");
        // signature d'un autre wallet
        let other = SigningKey::from_slice(&[9u8; 32]).unwrap();
        let other_addr = addr_from_key(&other);
        assert!(!verify_wallet_signature(&addr, msg, &sign_eip191(&other, msg)), "usurpation acceptée");
        // signature sur un autre message (montant modifié)
        assert!(!verify_wallet_signature(&addr, "transfer 0xabc 0xdef 999999", &sig), "tampering accepté");
        // garbage
        assert!(!verify_wallet_signature(&addr, msg, "0xdeadbeef"));
        assert!(!verify_wallet_signature(&addr, msg, &format!("0x{}", "ab".repeat(64)))); // 64 bytes seulement
    }
}
