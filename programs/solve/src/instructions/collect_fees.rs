use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount};
use anchor_spl::token_interface::TokenAccount as TokenAccountInterface;

use crate::{
    state::*,
    util::{transfer_from_vault_to_owner, verify_position_authority_interface},
};

#[derive(Accounts)]
pub struct CollectFees<'info> {
    pub solve: Box<Account<'info, Solve>>,

    pub position_authority: Signer<'info>,

    #[account(mut, has_one = solve)]
    pub position: Box<Account<'info, Position>>,
    #[account(
        constraint = position_token_account.mint == position.position_mint,
        constraint = position_token_account.amount == 1
    )]
    pub position_token_account: Box<InterfaceAccount<'info, TokenAccountInterface>>,

    #[account(mut, constraint = token_owner_account_a.mint == solve.token_mint_a)]
    pub token_owner_account_a: Box<Account<'info, TokenAccount>>,
    #[account(mut, address = solve.token_vault_a)]
    pub token_vault_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, constraint = token_owner_account_b.mint == solve.token_mint_b)]
    pub token_owner_account_b: Box<Account<'info, TokenAccount>>,
    #[account(mut, address = solve.token_vault_b)]
    pub token_vault_b: Box<Account<'info, TokenAccount>>,

    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<CollectFees>) -> Result<()> {
    verify_position_authority_interface(
        &ctx.accounts.position_token_account,
        &ctx.accounts.position_authority,
    )?;

    let position = &mut ctx.accounts.position;

    // Store the fees owed to use as transfer amounts.
    let fee_owed_a = position.fee_owed_a;
    let fee_owed_b = position.fee_owed_b;

    position.reset_fees_owed();

    transfer_from_vault_to_owner(
        &ctx.accounts.solve,
        &ctx.accounts.token_vault_a,
        &ctx.accounts.token_owner_account_a,
        &ctx.accounts.token_program,
        fee_owed_a,
    )?;

    transfer_from_vault_to_owner(
        &ctx.accounts.solve,
        &ctx.accounts.token_vault_b,
        &ctx.accounts.token_owner_account_b,
        &ctx.accounts.token_program,
        fee_owed_b,
    )?;

    Ok(())
}
