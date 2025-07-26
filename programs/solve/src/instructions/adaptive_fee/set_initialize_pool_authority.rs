use anchor_lang::prelude::*;

use crate::state::{AdaptiveFeeTier, SolvesConfig};

#[derive(Accounts)]
pub struct SetInitializePoolAuthority<'info> {
    pub solves_config: Account<'info, SolvesConfig>,

    #[account(mut, has_one = solves_config)]
    pub adaptive_fee_tier: Account<'info, AdaptiveFeeTier>,

    #[account(address = solves_config.fee_authority)]
    pub fee_authority: Signer<'info>,

    /// CHECK: safe, the account that will be new authority can be arbitrary
    pub new_initialize_pool_authority: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<SetInitializePoolAuthority>) -> Result<()> {
    ctx.accounts
        .adaptive_fee_tier
        .update_initialize_pool_authority(ctx.accounts.new_initialize_pool_authority.key());
    Ok(())
}
