use anchor_lang::prelude::*;

use crate::state::{Solve, SolvesConfig};

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct SetRewardAuthorityBySuperAuthority<'info> {
    pub solves_config: Account<'info, SolvesConfig>,

    #[account(mut, has_one = solves_config)]
    pub solve: Account<'info, Solve>,

    #[account(address = solves_config.reward_emissions_super_authority)]
    pub reward_emissions_super_authority: Signer<'info>,

    /// CHECK: safe, the account that will be new authority can be arbitrary
    pub new_reward_authority: UncheckedAccount<'info>,
}

/// Set the solve reward authority at the provided `reward_index`.
/// Only the current reward emissions super authority has permission to invoke this instruction.
pub fn handler(ctx: Context<SetRewardAuthorityBySuperAuthority>, reward_index: u8) -> Result<()> {
    ctx.accounts.solve.update_reward_authority(
        reward_index as usize,
        ctx.accounts.new_reward_authority.key(),
    )
}
