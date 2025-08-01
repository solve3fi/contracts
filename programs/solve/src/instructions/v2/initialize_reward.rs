use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::{
    state::Solve,
    util::{initialize_vault_token_account, verify_supported_token_mint},
};

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct InitializeRewardV2<'info> {
    #[account(address = solve.reward_infos[reward_index as usize].authority)]
    pub reward_authority: Signer<'info>,

    #[account(mut)]
    pub funder: Signer<'info>,

    #[account(mut)]
    pub solve: Box<Account<'info, Solve>>,

    pub reward_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(seeds = [b"token_badge", solve.solves_config.as_ref(), reward_mint.key().as_ref()], bump)]
    /// CHECK: checked in the handler
    pub reward_token_badge: UncheckedAccount<'info>,

    /// CHECK: initialized in the handler
    #[account(mut)]
    pub reward_vault: Signer<'info>,

    #[account(address = *reward_mint.to_account_info().owner)]
    pub reward_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<InitializeRewardV2>, reward_index: u8) -> Result<()> {
    let solve = &mut ctx.accounts.solve;

    // Don't allow initializing a reward with an unsupported token mint
    verify_supported_token_mint(
        &ctx.accounts.reward_mint,
        solve.solves_config,
        &ctx.accounts.reward_token_badge,
    )?;

    initialize_vault_token_account(
        solve,
        &ctx.accounts.reward_vault,
        &ctx.accounts.reward_mint,
        &ctx.accounts.funder,
        &ctx.accounts.reward_token_program,
        &ctx.accounts.system_program,
    )?;

    solve.initialize_reward(
        reward_index as usize,
        ctx.accounts.reward_mint.key(),
        ctx.accounts.reward_vault.key(),
    )
}
