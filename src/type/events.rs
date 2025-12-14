use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

/// What we decided from the observed target transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MirrorIntent {
    /// Target likely bought a token using SOL.
    Buy {
        output_mint: Pubkey,
        max_input_sol: f64,
    },
    /// Target likely sold a token into SOL (optional; disabled by default).
    Sell {
        input_mint: Pubkey,
        // fraction 0..1 of our balance (not implemented in this minimal build)
        _fraction: f64,
    },
}
