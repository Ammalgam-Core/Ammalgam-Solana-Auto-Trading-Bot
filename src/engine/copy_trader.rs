use crate::common::utils::{build_state, env_bool, env_f64, env_u16, env_var, parse_pubkey};
use crate::dex::jupiter::{jupiter_quote, jupiter_swap_tx, sign_and_send_swap, SOL_MINT};
use crate::engine::intent::infer_intent_from_tx;
use crate::helius::ws::connect_forever;
use anyhow::{anyhow, Result};
use reqwest::Client;
use solana_sdk::pubkey::Pubkey;
use tracing::{debug, error, info};

pub async fn run_copy_trader() -> Result<()> {
    let state = build_state().await?;

    let ws = env_var("RPC_WEBSOCKET_ENDPOINT")?;
    let target_str = env_var("TARGET_PUBKEY")?;
    let target: Pubkey = parse_pubkey("TARGET_PUBKEY", &target_str)?;

    let slippage_bps: u16 = env_u16("SLIPPAGE_BPS", 500);
    let max_buy_sol: f64 = env_f64("MAX_BUY_SOL", 0.02);
    let mirror_buys_only: bool = env_bool("MIRROR_BUYS_ONLY", true);

    info!("Ammalgram Assistant started");
    info!("Wallet: {}", state.wallet_pubkey);
    info!("Target: {}", target);
    info!("SLIPPAGE_BPS={slippage_bps}, MAX_BUY_SOL={max_buy_sol}, MIRROR_BUYS_ONLY={mirror_buys_only}");

    let http = Client::new();

    // WS stream (auto reconnect)
    let mut stream = connect_forever(ws, target_str).await?;

    // To avoid rapid duplicate triggers, keep last signature seen
    let mut last_sig: Option<String> = None;

    while let Some(msg) = stream.next().await {
        // Extract signature if exists
        let sig = msg
            .pointer("/params/result/signature")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(s) = &sig {
            if last_sig.as_deref() == Some(s.as_str()) {
                continue;
            }
            last_sig = Some(s.clone());
        }

        debug!("WS msg: {}", msg);

        let intent = match infer_intent_from_tx(&msg, max_buy_sol) {
            Ok(v) => v,
            Err(e) => {
                error!("Intent infer error: {e}");
                continue;
            }
        };

        let Some(intent) = intent else { continue; };

        match intent {
            crate::types::events::MirrorIntent::Buy { output_mint, max_input_sol } => {
                // Safety: mirror only BUYs by default
                if !mirror_buys_only {
                    info!("BUY intent detected but MIRROR_BUYS_ONLY=false; continuing anyway");
                }

                // Convert SOL to lamports
                let lamports = sol_to_lamports(max_input_sol)?;
                info!("Mirroring BUY: spend up to {max_input_sol} SOL ({lamports} lamports) -> mint {output_mint}");

                let quote = jupiter_quote(
                    &http,
                    SOL_MINT,
                    &output_mint.to_string(),
                    lamports,
                    slippage_bps,
                )
                .await;

                let quote = match quote {
                    Ok(q) => q,
                    Err(e) => {
                        error!("Quote failed: {e}");
                        continue;
                    }
                };

                let swap = jupiter_swap_tx(
                    &http,
                    quote.clone(),
                    state.wallet_pubkey,
                    0, // you can set tip/priority fee if you want
                )
                .await;

                let swap = match swap {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Swap tx build failed: {e}");
                        continue;
                    }
                };

                let sent = sign_and_send_swap(
                    &state.rpc_nonblocking_client,
                    &state.wallet,
                    &swap.swap_transaction,
                )
                .await;

                match sent {
                    Ok(sig) => info!("Mirrored BUY sent: {sig}"),
                    Err(e) => error!("Send failed: {e}"),
                }
            }
            crate::types::events::MirrorIntent::Sell { .. } => {
                info!("SELL intent detected (not implemented in minimal safe build). Skipping.");
            }
        }
    }

    Ok(())
}

fn sol_to_lamports(sol: f64) -> Result<u64> {
    if !(0.0..=1000.0).contains(&sol) {
        return Err(anyhow!("SOL amount out of safe range"));
    }
    // 1 SOL = 1_000_000_000 lamports
    let lamports = (sol * 1_000_000_000.0).round();
    Ok(lamports as u64)
}

// StreamExt import
use futures_util::StreamExt;
