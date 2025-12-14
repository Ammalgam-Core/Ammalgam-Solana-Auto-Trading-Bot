use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_client::nonblocking::rpc_client::RpcClient as AsyncRpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair};
use std::{env, str::FromStr, sync::Arc};

#[derive(Clone)]
pub struct AppState {
    pub rpc_client: Arc<RpcClient>,
    pub rpc_nonblocking_client: Arc<AsyncRpcClient>,
    pub wallet: Arc<Keypair>,
    pub wallet_pubkey: Pubkey,
}

pub fn env_var(key: &str) -> Result<String> {
    env::var(key).map_err(|_| anyhow!("Environment variable {key} is not set"))
}

pub fn env_var_opt(key: &str) -> Option<String> {
    env::var(key).ok()
}

pub fn env_bool(key: &str, default: bool) -> bool {
    match env_var_opt(key).map(|s| s.to_lowercase()) {
        Some(v) if v == "true" || v == "1" || v == "yes" || v == "y" => true,
        Some(v) if v == "false" || v == "0" || v == "no" || v == "n" => false,
        Some(_) => default,
        None => default,
    }
}

pub fn env_f64(key: &str, default: f64) -> f64 {
    env_var_opt(key)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

pub fn env_u64(key: &str, default: u64) -> u64 {
    env_var_opt(key)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

pub fn env_u16(key: &str, default: u16) -> u16 {
    env_var_opt(key)
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(default)
}

pub fn parse_pubkey(key: &str, value: &str) -> Result<Pubkey> {
    Pubkey::from_str(value).map_err(|e| anyhow!("Invalid pubkey in {key}: {e}"))
}

pub fn create_rpc_client() -> Result<Arc<RpcClient>> {
    let rpc_https = env_var("RPC_ENDPOINT")?;
    Ok(Arc::new(RpcClient::new_with_commitment(
        rpc_https,
        CommitmentConfig::processed(),
    )))
}

pub async fn create_nonblocking_rpc_client() -> Result<Arc<AsyncRpcClient>> {
    let rpc_https = env_var("RPC_ENDPOINT")?;
    Ok(Arc::new(AsyncRpcClient::new_with_commitment(
        rpc_https,
        CommitmentConfig::processed(),
    )))
}

/// PRIVATE_KEY can be either:
/// - base58-encoded 64-byte secret key, OR
/// - a JSON array from Solana CLI id.json (e.g. "[12,34,...]").
///
/// Supports both formats.
pub fn import_wallet() -> Result<Arc<Keypair>> {
    let raw = env_var("PRIVATE_KEY")?.trim().to_string();

    if raw.starts_with('[') {
        let bytes: Vec<u8> = serde_json::from_str(&raw)?;
        let kp = Keypair::from_bytes(&bytes)?;
        return Ok(Arc::new(kp));
    }

    Ok(Arc::new(Keypair::from_base58_string(&raw)))
}

pub async fn build_state() -> Result<AppState> {
    let rpc_client = create_rpc_client()?;
    let rpc_nonblocking_client = create_nonblocking_rpc_client().await?;
    let wallet = import_wallet()?;
    let wallet_pubkey = wallet.pubkey();

    Ok(AppState {
        rpc_client,
        rpc_nonblocking_client,
        wallet,
        wallet_pubkey,
    })
}
