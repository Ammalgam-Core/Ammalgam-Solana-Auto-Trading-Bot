use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient as AsyncRpcClient;
use solana_sdk::{
    hash::Hash,
    message::VersionedMessage,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::VersionedTransaction,
};
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize)]
pub struct QuoteRequest {
    pub input_mint: String,
    pub output_mint: String,
    pub amount: u64,
    pub slippage_bps: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuoteResponse {
    #[serde(rename = "inAmount")]
    pub in_amount: String,
    #[serde(rename = "outAmount")]
    pub out_amount: String,

    // we keep full route object for swap
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwapRequest {
    #[serde(rename = "quoteResponse")]
    pub quote_response: serde_json::Value,
    #[serde(rename = "userPublicKey")]
    pub user_public_key: String,
    #[serde(rename = "wrapAndUnwrapSol")]
    pub wrap_and_unwrap_sol: bool,
    #[serde(rename = "dynamicComputeUnitLimit")]
    pub dynamic_compute_unit_limit: bool,
    #[serde(rename = "prioritizationFeeLamports")]
    pub prioritization_fee_lamports: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SwapResponse {
    /// Base64-encoded versioned transaction
    #[serde(rename = "swapTransaction")]
    pub swap_transaction: String,
}

pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// Jupiter v6 quote endpoint
fn quote_url() -> &'static str {
    "https://quote-api.jup.ag/v6/quote"
}

/// Jupiter v6 swap endpoint
fn swap_url() -> &'static str {
    "https://quote-api.jup.ag/v6/swap"
}

pub async fn jupiter_quote(
    http: &Client,
    input_mint: &str,
    output_mint: &str,
    amount: u64,
    slippage_bps: u16,
) -> Result<serde_json::Value> {
    let url = reqwest::Url::parse_with_params(
        quote_url(),
        &[
            ("inputMint", input_mint),
            ("outputMint", output_mint),
            ("amount", &amount.to_string()),
            ("slippageBps", &slippage_bps.to_string()),
        ],
    )?;

    let res = http.get(url).send().await?;
    if !res.status().is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(anyhow!("Jupiter quote failed: {}", t));
    }
    Ok(res.json::<serde_json::Value>().await?)
}

pub async fn jupiter_swap_tx(
    http: &Client,
    quote_response: serde_json::Value,
    user_pubkey: Pubkey,
    prioritization_fee_lamports: u64,
) -> Result<SwapResponse> {
    let req = SwapRequest {
        quote_response,
        user_public_key: user_pubkey.to_string(),
        wrap_and_unwrap_sol: true,
        dynamic_compute_unit_limit: true,
        prioritization_fee_lamports,
    };

    let res = http.post(swap_url()).json(&req).send().await?;
    if !res.status().is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(anyhow!("Jupiter swap failed: {}", t));
    }
    Ok(res.json::<SwapResponse>().await?)
}

pub async fn sign_and_send_swap(
    rpc: &AsyncRpcClient,
    wallet: &Keypair,
    swap_b64: &str,
) -> Result<Signature> {
    let bytes = B64.decode(swap_b64)?;
    let mut tx: VersionedTransaction = bincode::deserialize(&bytes)?;

    // Ensure blockhash is fresh
    let latest: Hash = rpc.get_latest_blockhash().await?;

    // Replace recent blockhash inside message (both legacy and v0)
    // We must rebuild the message with updated blockhash.
    let msg = match &tx.message {
        VersionedMessage::Legacy(m) => {
            let mut m2 = m.clone();
            m2.recent_blockhash = latest;
            VersionedMessage::Legacy(m2)
        }
        VersionedMessage::V0(m) => {
            let mut m2 = m.clone();
            m2.recent_blockhash = latest;
            VersionedMessage::V0(m2)
        }
    };

    // Re-sign
    let signers: [&Keypair; 1] = [wallet];
    tx.message = msg;
    tx.sign(&signers, latest);

    debug!("Sending signed swap tx...");
    let sig = rpc.send_transaction(&tx).await?;
    info!("Sent swap tx: {sig}");
    Ok(sig)
}
