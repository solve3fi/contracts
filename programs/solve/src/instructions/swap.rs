use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount};

use crate::{
    errors::ErrorCode,
    events::*,
    manager::swap_manager::*,
    state::{OracleAccessor, Solve},
    util::{to_timestamp_u64, update_and_swap_solve, SparseSwapTickSequenceBuilder},
};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,

    pub token_authority: Signer<'info>,

    #[account(mut)]
    pub solve: Box<Account<'info, Solve>>,

    #[account(mut, constraint = token_owner_account_a.mint == solve.token_mint_a)]
    pub token_owner_account_a: Box<Account<'info, TokenAccount>>,
    #[account(mut, address = solve.token_vault_a)]
    pub token_vault_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, constraint = token_owner_account_b.mint == solve.token_mint_b)]
    pub token_owner_account_b: Box<Account<'info, TokenAccount>>,
    #[account(mut, address = solve.token_vault_b)]
    pub token_vault_b: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_0: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_1: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_2: UncheckedAccount<'info>,

    #[account(seeds = [b"oracle", solve.key().as_ref()], bump)]
    /// CHECK: Oracle is currently unused and will be enabled on subsequent updates
    pub oracle: UncheckedAccount<'info>,
    // Special notes to support pools with AdaptiveFee:
    // - For trades on pool using AdaptiveFee, pass oracle as writable accounts in the remaining accounts.
    // - If you want to avoid using the remaining accounts, you can pass oracle as writable accounts directly.

    // remaining accounts
    // - [mut] oracle
}

pub fn handler(
    ctx: Context<Swap>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit: u128,
    amount_specified_is_input: bool,
    a_to_b: bool, // Zero for one
) -> Result<()> {
    let solve = &mut ctx.accounts.solve;
    let clock = Clock::get()?;
    // Update the global reward growth which increases as a function of time.
    let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

    let swap_tick_sequence_builder = SparseSwapTickSequenceBuilder::new(
        vec![
            ctx.accounts.tick_array_0.to_account_info(),
            ctx.accounts.tick_array_1.to_account_info(),
            ctx.accounts.tick_array_2.to_account_info(),
        ],
        None,
    );
    let mut swap_tick_sequence = swap_tick_sequence_builder.try_build(solve, a_to_b)?;

    let oracle_accessor = OracleAccessor::new(solve, ctx.accounts.oracle.to_account_info())?;
    if !oracle_accessor.is_trade_enabled(timestamp)? {
        return Err(ErrorCode::TradeIsNotEnabled.into());
    }
    let adaptive_fee_info = oracle_accessor.get_adaptive_fee_info()?;

    let swap_update = swap(
        solve,
        &mut swap_tick_sequence,
        amount,
        sqrt_price_limit,
        amount_specified_is_input,
        a_to_b,
        timestamp,
        &adaptive_fee_info,
    )?;

    if amount_specified_is_input {
        if (a_to_b && other_amount_threshold > swap_update.amount_b)
            || (!a_to_b && other_amount_threshold > swap_update.amount_a)
        {
            return Err(ErrorCode::AmountOutBelowMinimum.into());
        }
    } else if (a_to_b && other_amount_threshold < swap_update.amount_a)
        || (!a_to_b && other_amount_threshold < swap_update.amount_b)
    {
        return Err(ErrorCode::AmountInAboveMaximum.into());
    }

    oracle_accessor.update_adaptive_fee_variables(&swap_update.next_adaptive_fee_info)?;

    let pre_sqrt_price = solve.sqrt_price;
    let (input_amount, output_amount) = if a_to_b {
        (swap_update.amount_a, swap_update.amount_b)
    } else {
        (swap_update.amount_b, swap_update.amount_a)
    };
    let (lp_fee, protocol_fee) = (swap_update.lp_fee, swap_update.next_protocol_fee);

    update_and_swap_solve(
        solve,
        &ctx.accounts.token_authority,
        &ctx.accounts.token_owner_account_a,
        &ctx.accounts.token_owner_account_b,
        &ctx.accounts.token_vault_a,
        &ctx.accounts.token_vault_b,
        &ctx.accounts.token_program,
        &swap_update,
        a_to_b,
        timestamp,
    )?;

    emit!(Traded {
        solve: solve.key(),
        a_to_b,
        pre_sqrt_price,
        post_sqrt_price: solve.sqrt_price,
        input_amount,
        output_amount,
        input_transfer_fee: 0,
        output_transfer_fee: 0,
        lp_fee,
        protocol_fee,
    });

    Ok(())
}
