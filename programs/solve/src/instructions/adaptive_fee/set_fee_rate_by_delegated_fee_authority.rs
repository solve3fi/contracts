use anchor_lang::prelude::*;

use crate::state::{AdaptiveFeeTier, Solve};

#[derive(Accounts)]
pub struct SetFeeRateByDelegatedFeeAuthority<'info> {
    #[account(mut,
        constraint = solve.is_initialized_with_adaptive_fee_tier(),
    )]
    pub solve: Account<'info, Solve>,

    #[account(
        constraint = adaptive_fee_tier.solves_config == solve.solves_config,
        constraint = adaptive_fee_tier.fee_tier_index == solve.fee_tier_index(),
    )]
    pub adaptive_fee_tier: Account<'info, AdaptiveFeeTier>,

    #[account(address = adaptive_fee_tier.delegated_fee_authority)]
    pub delegated_fee_authority: Signer<'info>,
}

pub fn handler(ctx: Context<SetFeeRateByDelegatedFeeAuthority>, fee_rate: u16) -> Result<()> {
    ctx.accounts.solve.update_fee_rate(fee_rate)
}
