use anchor_lang::prelude::*;
use anchor_spl::memo::Memo;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{
    constants::transfer_memo,
    errors::ErrorCode,
    events::*,
    manager::swap_manager::*,
    state::*,
    util::{
        calculate_transfer_fee_excluded_amount, calculate_transfer_fee_included_amount,
        parse_remaining_accounts, to_timestamp_u64, v2::update_and_swap_solve_v2, AccountsType,
        RemainingAccountsInfo, SparseSwapTickSequenceBuilder, SwapTickSequence,
    },
};

#[derive(Accounts)]
pub struct SwapV2<'info> {
    #[account(address = *token_mint_a.to_account_info().owner)]
    pub token_program_a: Interface<'info, TokenInterface>,
    #[account(address = *token_mint_b.to_account_info().owner)]
    pub token_program_b: Interface<'info, TokenInterface>,

    pub memo_program: Program<'info, Memo>,

    pub token_authority: Signer<'info>,

    #[account(mut)]
    pub solve: Box<Account<'info, Solve>>,

    #[account(address = solve.token_mint_a)]
    pub token_mint_a: InterfaceAccount<'info, Mint>,
    #[account(address = solve.token_mint_b)]
    pub token_mint_b: InterfaceAccount<'info, Mint>,

    #[account(mut, constraint = token_owner_account_a.mint == solve.token_mint_a)]
    pub token_owner_account_a: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = solve.token_vault_a)]
    pub token_vault_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, constraint = token_owner_account_b.mint == solve.token_mint_b)]
    pub token_owner_account_b: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = solve.token_vault_b)]
    pub token_vault_b: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_0: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_1: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_2: UncheckedAccount<'info>,

    #[account(mut, seeds = [b"oracle", solve.key().as_ref()], bump)]
    /// CHECK: Oracle is currently unused and will be enabled on subsequent updates
    pub oracle: UncheckedAccount<'info>,
    // remaining accounts
    // - accounts for transfer hook program of token_mint_a
    // - accounts for transfer hook program of token_mint_b
    // - supplemental TickArray accounts
}

pub fn handler<'info>(
    ctx: Context<'_, '_, '_, 'info, SwapV2<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit: u128,
    amount_specified_is_input: bool,
    a_to_b: bool, // Zero for one
    remaining_accounts_info: Option<RemainingAccountsInfo>,
) -> Result<()> {
    let solve = &mut ctx.accounts.solve;
    let clock = Clock::get()?;
    // Update the global reward growth which increases as a function of time.
    let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

    // Process remaining accounts
    let remaining_accounts = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[
            AccountsType::TransferHookA,
            AccountsType::TransferHookB,
            AccountsType::SupplementalTickArrays,
        ],
    )?;

    let swap_tick_sequence_builder = SparseSwapTickSequenceBuilder::new(
        vec![
            ctx.accounts.tick_array_0.to_account_info(),
            ctx.accounts.tick_array_1.to_account_info(),
            ctx.accounts.tick_array_2.to_account_info(),
        ],
        remaining_accounts.supplemental_tick_arrays,
    );
    let mut swap_tick_sequence = swap_tick_sequence_builder.try_build(solve, a_to_b)?;

    let oracle_accessor = OracleAccessor::new(solve, ctx.accounts.oracle.to_account_info())?;
    if !oracle_accessor.is_trade_enabled(timestamp)? {
        return Err(ErrorCode::TradeIsNotEnabled.into());
    }
    let adaptive_fee_info = oracle_accessor.get_adaptive_fee_info()?;

    let swap_update = swap_with_transfer_fee_extension(
        solve,
        &ctx.accounts.token_mint_a,
        &ctx.accounts.token_mint_b,
        &mut swap_tick_sequence,
        amount,
        sqrt_price_limit,
        amount_specified_is_input,
        a_to_b,
        timestamp,
        &adaptive_fee_info,
    )?;

    if amount_specified_is_input {
        let transfer_fee_excluded_output_amount = if a_to_b {
            calculate_transfer_fee_excluded_amount(
                &ctx.accounts.token_mint_b,
                swap_update.amount_b,
            )?
            .amount
        } else {
            calculate_transfer_fee_excluded_amount(
                &ctx.accounts.token_mint_a,
                swap_update.amount_a,
            )?
            .amount
        };
        if transfer_fee_excluded_output_amount < other_amount_threshold {
            return Err(ErrorCode::AmountOutBelowMinimum.into());
        }
    } else {
        let transfer_fee_included_input_amount = if a_to_b {
            swap_update.amount_a
        } else {
            swap_update.amount_b
        };
        if transfer_fee_included_input_amount > other_amount_threshold {
            return Err(ErrorCode::AmountInAboveMaximum.into());
        }
    }

    oracle_accessor.update_adaptive_fee_variables(&swap_update.next_adaptive_fee_info)?;

    let pre_sqrt_price = solve.sqrt_price;
    let (input_amount, output_amount) = if a_to_b {
        (swap_update.amount_a, swap_update.amount_b)
    } else {
        (swap_update.amount_b, swap_update.amount_a)
    };
    let (token_mint_input, token_mint_output) = if a_to_b {
        (&ctx.accounts.token_mint_a, &ctx.accounts.token_mint_b)
    } else {
        (&ctx.accounts.token_mint_b, &ctx.accounts.token_mint_a)
    };
    let input_transfer_fee =
        calculate_transfer_fee_excluded_amount(token_mint_input, input_amount)?.transfer_fee;
    let output_transfer_fee =
        calculate_transfer_fee_excluded_amount(token_mint_output, output_amount)?.transfer_fee;
    let (lp_fee, protocol_fee) = (swap_update.lp_fee, swap_update.next_protocol_fee);

    update_and_swap_solve_v2(
        solve,
        &ctx.accounts.token_authority,
        &ctx.accounts.token_mint_a,
        &ctx.accounts.token_mint_b,
        &ctx.accounts.token_owner_account_a,
        &ctx.accounts.token_owner_account_b,
        &ctx.accounts.token_vault_a,
        &ctx.accounts.token_vault_b,
        &remaining_accounts.transfer_hook_a,
        &remaining_accounts.transfer_hook_b,
        &ctx.accounts.token_program_a,
        &ctx.accounts.token_program_b,
        &ctx.accounts.memo_program,
        &swap_update,
        a_to_b,
        timestamp,
        transfer_memo::TRANSFER_MEMO_SWAP.as_bytes(),
    )?;

    emit!(Traded {
        solve: solve.key(),
        a_to_b,
        pre_sqrt_price,
        post_sqrt_price: solve.sqrt_price,
        input_amount,
        output_amount,
        input_transfer_fee,
        output_transfer_fee,
        lp_fee,
        protocol_fee,
    });

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn swap_with_transfer_fee_extension<'info>(
    solve: &Solve,
    token_mint_a: &InterfaceAccount<'info, Mint>,
    token_mint_b: &InterfaceAccount<'info, Mint>,
    swap_tick_sequence: &mut SwapTickSequence,
    amount: u64,
    sqrt_price_limit: u128,
    amount_specified_is_input: bool,
    a_to_b: bool,
    timestamp: u64,
    adaptive_fee_info: &Option<AdaptiveFeeInfo>,
) -> Result<Box<PostSwapUpdate>> {
    let (input_token_mint, output_token_mint) = if a_to_b {
        (token_mint_a, token_mint_b)
    } else {
        (token_mint_b, token_mint_a)
    };

    // ExactIn
    if amount_specified_is_input {
        let transfer_fee_included_input = amount;
        let transfer_fee_excluded_input =
            calculate_transfer_fee_excluded_amount(input_token_mint, transfer_fee_included_input)?
                .amount;

        let swap_update = swap(
            solve,
            swap_tick_sequence,
            transfer_fee_excluded_input,
            sqrt_price_limit,
            amount_specified_is_input,
            a_to_b,
            timestamp,
            adaptive_fee_info,
        )?;

        let (swap_update_amount_input, swap_update_amount_output) = if a_to_b {
            (swap_update.amount_a, swap_update.amount_b)
        } else {
            (swap_update.amount_b, swap_update.amount_a)
        };

        let fullfilled = swap_update_amount_input == transfer_fee_excluded_input;

        let adjusted_transfer_fee_included_input = if fullfilled {
            transfer_fee_included_input
        } else {
            calculate_transfer_fee_included_amount(input_token_mint, swap_update_amount_input)?
                .amount
        };

        let transfer_fee_included_output = swap_update_amount_output;

        let (amount_a, amount_b) = if a_to_b {
            (
                adjusted_transfer_fee_included_input,
                transfer_fee_included_output,
            )
        } else {
            (
                transfer_fee_included_output,
                adjusted_transfer_fee_included_input,
            )
        };
        return Ok(Box::new(PostSwapUpdate {
            amount_a, // updated (transfer fee included)
            amount_b, // updated (transfer fee included)
            lp_fee: swap_update.lp_fee,
            next_liquidity: swap_update.next_liquidity,
            next_tick_index: swap_update.next_tick_index,
            next_sqrt_price: swap_update.next_sqrt_price,
            next_fee_growth_global: swap_update.next_fee_growth_global,
            next_reward_infos: swap_update.next_reward_infos,
            next_protocol_fee: swap_update.next_protocol_fee,
            next_adaptive_fee_info: swap_update.next_adaptive_fee_info,
        }));
    }

    // ExactOut
    let transfer_fee_excluded_output = amount;
    let transfer_fee_included_output =
        calculate_transfer_fee_included_amount(output_token_mint, transfer_fee_excluded_output)?
            .amount;

    let swap_update = swap(
        solve,
        swap_tick_sequence,
        transfer_fee_included_output,
        sqrt_price_limit,
        amount_specified_is_input,
        a_to_b,
        timestamp,
        adaptive_fee_info,
    )?;

    let (swap_update_amount_input, swap_update_amount_output) = if a_to_b {
        (swap_update.amount_a, swap_update.amount_b)
    } else {
        (swap_update.amount_b, swap_update.amount_a)
    };

    let transfer_fee_included_input =
        calculate_transfer_fee_included_amount(input_token_mint, swap_update_amount_input)?.amount;

    let adjusted_transfer_fee_included_output = swap_update_amount_output;

    let (amount_a, amount_b) = if a_to_b {
        (
            transfer_fee_included_input,
            adjusted_transfer_fee_included_output,
        )
    } else {
        (
            adjusted_transfer_fee_included_output,
            transfer_fee_included_input,
        )
    };
    Ok(Box::new(PostSwapUpdate {
        amount_a, // updated (transfer fee included)
        amount_b, // updated (transfer fee included)
        lp_fee: swap_update.lp_fee,
        next_liquidity: swap_update.next_liquidity,
        next_tick_index: swap_update.next_tick_index,
        next_sqrt_price: swap_update.next_sqrt_price,
        next_fee_growth_global: swap_update.next_fee_growth_global,
        next_reward_infos: swap_update.next_reward_infos,
        next_protocol_fee: swap_update.next_protocol_fee,
        next_adaptive_fee_info: swap_update.next_adaptive_fee_info,
    }))
}
