use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::state::Solve;

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct InitializeReward<'info> {
    #[account(address = solve.reward_infos[reward_index as usize].authority)]
    pub reward_authority: Signer<'info>,

    #[account(mut)]
    pub funder: Signer<'info>,

    #[account(mut)]
    pub solve: Box<Account<'info, Solve>>,

    pub reward_mint: Box<Account<'info, Mint>>,

    #[account(
        init,
        payer = funder,
        token::mint = reward_mint,
        token::authority = solve
    )]
    pub reward_vault: Box<Account<'info, TokenAccount>>,

    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<InitializeReward>, reward_index: u8) -> Result<()> {
    let solve = &mut ctx.accounts.solve;

    solve.initialize_reward(
        reward_index as usize,
        ctx.accounts.reward_mint.key(),
        ctx.accounts.reward_vault.key(),
    )
}
