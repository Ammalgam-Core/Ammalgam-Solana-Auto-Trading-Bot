use crate::types::events::MirrorIntent;
use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tracing::debug;

/// Very lightweight heuristic:
/// - Look at token balance changes in `meta.preTokenBalances`/`postTokenBalances`.
/// - If TARGET ends up with MORE of some mint after tx => treat as BUY of that mint.
///
/// This avoids parsing all instructions/programs and still works for most swaps.
/// Limitations: it can mis-detect non-swap token receives.
pub fn infer_intent_from_tx(json_msg: &serde_json::Value, max_buy_sol: f64) -> Result<Option<MirrorIntent>> {
    // Expected Solana WS shape:
    // { "method":"transactionNotification", "params": { "result": { "transaction": [...], "meta": {...} } } }
    let result = json_msg
        .pointer("/params/result")
        .or_else(|| json_msg.pointer("/result"));

    let Some(r) = result else { return Ok(None); };

    let meta = r.get("meta");
    let Some(meta) = meta else { return Ok(None); };

    let pre = meta.get("preTokenBalances").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let post = meta.get("postTokenBalances").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    // Build map mint -> (pre_ui, post_ui)
    // We don't need exact decimals conversion for decision; just compare uiAmount.
    use std::collections::HashMap;
    let mut pre_map: HashMap<String, f64> = HashMap::new();
    let mut post_map: HashMap<String, f64> = HashMap::new();

    for x in pre {
        if let (Some(mint), Some(ui)) = (x.get("mint"), x.pointer("/uiTokenAmount/uiAmount")) {
            if let (Some(m), Some(v)) = (mint.as_str(), ui.as_f64()) {
                pre_map.insert(m.to_string(), v);
            }
        }
    }
    for x in post {
        if let (Some(mint), Some(ui)) = (x.get("mint"), x.pointer("/uiTokenAmount/uiAmount")) {
            if let (Some(m), Some(v)) = (mint.as_str(), ui.as_f64()) {
                post_map.insert(m.to_string(), v);
            }
        }
    }

    // Find mint where post > pre by meaningful delta
    let mut best: Option<(String, f64)> = None;
    for (mint, post_v) in &post_map {
        let pre_v = pre_map.get(mint).copied().unwrap_or(0.0);
        let delta = post_v - pre_v;
        if delta > 0.0 {
            // ignore dust
            if delta < 0.0000001 { continue; }
            best = match best {
                None => Some((mint.clone(), delta)),
                Some((bm, bd)) => {
                    if delta > bd { Some((mint.clone(), delta)) } else { Some((bm, bd)) }
                }
            };
        }
    }

    let Some((mint, delta)) = best else {
        debug!("No positive token delta detected; skip");
        return Ok(None);
    };

    let output_mint = Pubkey::from_str(&mint)?;
    debug!("Heuristic intent: BUY mint={mint}, delta_ui={delta}");

    Ok(Some(MirrorIntent::Buy {
        output_mint,
        max_input_sol: max_buy_sol,
    }))
}
