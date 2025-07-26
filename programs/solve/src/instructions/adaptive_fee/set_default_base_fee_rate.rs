use anchor_lang::prelude::*;

use crate::state::{AdaptiveFeeTier, SolvesConfig};

#[derive(Accounts)]
pub struct SetDefaultBaseFeeRate<'info> {
    pub solves_config: Account<'info, SolvesConfig>,

    #[account(mut, has_one = solves_config)]
    pub adaptive_fee_tier: Account<'info, AdaptiveFeeTier>,

    #[account(address = solves_config.fee_authority)]
    pub fee_authority: Signer<'info>,
}

pub fn handler(ctx: Context<SetDefaultBaseFeeRate>, default_base_fee_rate: u16) -> Result<()> {
    ctx.accounts
        .adaptive_fee_tier
        .update_default_base_fee_rate(default_base_fee_rate)
}
