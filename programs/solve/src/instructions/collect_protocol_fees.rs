use crate::{state::*, util::transfer_from_vault_to_owner};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount};

#[derive(Accounts)]
pub struct CollectProtocolFees<'info> {
    pub solves_config: Box<Account<'info, SolvesConfig>>,

    #[account(mut, has_one = solves_config)]
    pub solve: Box<Account<'info, Solve>>,

    #[account(address = solves_config.collect_protocol_fees_authority)]
    pub collect_protocol_fees_authority: Signer<'info>,

    #[account(mut, address = solve.token_vault_a)]
    pub token_vault_a: Account<'info, TokenAccount>,

    #[account(mut, address = solve.token_vault_b)]
    pub token_vault_b: Account<'info, TokenAccount>,

    #[account(mut, constraint = token_destination_a.mint == solve.token_mint_a)]
    pub token_destination_a: Account<'info, TokenAccount>,

    #[account(mut, constraint = token_destination_b.mint == solve.token_mint_b)]
    pub token_destination_b: Account<'info, TokenAccount>,

    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<CollectProtocolFees>) -> Result<()> {
    let solve = &ctx.accounts.solve;

    transfer_from_vault_to_owner(
        solve,
        &ctx.accounts.token_vault_a,
        &ctx.accounts.token_destination_a,
        &ctx.accounts.token_program,
        solve.protocol_fee_owed_a,
    )?;

    transfer_from_vault_to_owner(
        solve,
        &ctx.accounts.token_vault_b,
        &ctx.accounts.token_destination_b,
        &ctx.accounts.token_program,
        solve.protocol_fee_owed_b,
    )?;

    ctx.accounts.solve.reset_protocol_fees_owed();
    Ok(())
}
