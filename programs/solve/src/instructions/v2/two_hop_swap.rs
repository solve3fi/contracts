use anchor_lang::prelude::*;
use anchor_spl::memo::Memo;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::swap_with_transfer_fee_extension;
use crate::{
    constants::transfer_memo,
    errors::ErrorCode,
    events::*,
    state::{OracleAccessor, Solve},
    util::{
        calculate_transfer_fee_excluded_amount, parse_remaining_accounts, to_timestamp_u64,
        update_and_two_hop_swap_solve_v2, AccountsType, RemainingAccountsInfo,
        SparseSwapTickSequenceBuilder,
    },
};

#[derive(Accounts)]
#[instruction(
    amount: u64,
    other_amount_threshold: u64,
    amount_specified_is_input: bool,
    a_to_b_one: bool,
    a_to_b_two: bool,
)]
pub struct TwoHopSwapV2<'info> {
    #[account(mut)]
    pub solve_one: Box<Account<'info, Solve>>,
    #[account(mut)]
    pub solve_two: Box<Account<'info, Solve>>,

    #[account(address = solve_one.input_token_mint(a_to_b_one))]
    pub token_mint_input: InterfaceAccount<'info, Mint>,
    #[account(address = solve_one.output_token_mint(a_to_b_one))]
    pub token_mint_intermediate: InterfaceAccount<'info, Mint>,
    #[account(address = solve_two.output_token_mint(a_to_b_two))]
    pub token_mint_output: InterfaceAccount<'info, Mint>,

    #[account(address = *token_mint_input.to_account_info().owner)]
    pub token_program_input: Interface<'info, TokenInterface>,
    #[account(address = *token_mint_intermediate.to_account_info().owner)]
    pub token_program_intermediate: Interface<'info, TokenInterface>,
    #[account(address = *token_mint_output.to_account_info().owner)]
    pub token_program_output: Interface<'info, TokenInterface>,

    #[account(mut, constraint = token_owner_account_input.mint == token_mint_input.key())]
    pub token_owner_account_input: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = solve_one.input_token_vault(a_to_b_one))]
    pub token_vault_one_input: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = solve_one.output_token_vault(a_to_b_one))]
    pub token_vault_one_intermediate: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, address = solve_two.input_token_vault(a_to_b_two))]
    pub token_vault_two_intermediate: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = solve_two.output_token_vault(a_to_b_two))]
    pub token_vault_two_output: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, constraint = token_owner_account_output.mint == token_mint_output.key())]
    pub token_owner_account_output: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_authority: Signer<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_one_0: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_one_1: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_one_2: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_two_0: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_two_1: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_two_2: UncheckedAccount<'info>,

    #[account(mut, seeds = [b"oracle", solve_one.key().as_ref()], bump)]
    /// CHECK: Oracle is currently unused and will be enabled on subsequent updates
    pub oracle_one: UncheckedAccount<'info>,

    #[account(mut, seeds = [b"oracle", solve_two.key().as_ref()], bump)]
    /// CHECK: Oracle is currently unused and will be enabled on subsequent updates
    pub oracle_two: UncheckedAccount<'info>,

    pub memo_program: Program<'info, Memo>,
    // remaining accounts
    // - accounts for transfer hook program of token_mint_input
    // - accounts for transfer hook program of token_mint_intermediate
    // - accounts for transfer hook program of token_mint_output
    // - supplemental TickArray accounts for solve_one
    // - supplemental TickArray accounts for solve_two
}

#[allow(clippy::too_many_arguments)]
pub fn handler<'info>(
    ctx: Context<'_, '_, '_, 'info, TwoHopSwapV2<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    amount_specified_is_input: bool,
    a_to_b_one: bool,
    a_to_b_two: bool,
    sqrt_price_limit_one: u128,
    sqrt_price_limit_two: u128,
    remaining_accounts_info: Option<RemainingAccountsInfo>,
) -> Result<()> {
    let clock = Clock::get()?;
    // Update the global reward growth which increases as a function of time.
    let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

    let solve_one = &mut ctx.accounts.solve_one;
    let solve_two = &mut ctx.accounts.solve_two;

    // Don't allow swaps on the same solve
    if solve_one.key() == solve_two.key() {
        return Err(ErrorCode::DuplicateTwoHopPool.into());
    }

    let swap_one_output_mint = if a_to_b_one {
        solve_one.token_mint_b
    } else {
        solve_one.token_mint_a
    };

    let swap_two_input_mint = if a_to_b_two {
        solve_two.token_mint_a
    } else {
        solve_two.token_mint_b
    };
    if swap_one_output_mint != swap_two_input_mint {
        return Err(ErrorCode::InvalidIntermediaryMint.into());
    }

    // Process remaining accounts
    let remaining_accounts = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[
            AccountsType::TransferHookInput,
            AccountsType::TransferHookIntermediate,
            AccountsType::TransferHookOutput,
            AccountsType::SupplementalTickArraysOne,
            AccountsType::SupplementalTickArraysTwo,
        ],
    )?;

    let swap_tick_sequence_one = SparseSwapTickSequenceBuilder::new(
        vec![
            ctx.accounts.tick_array_one_0.to_account_info(),
            ctx.accounts.tick_array_one_1.to_account_info(),
            ctx.accounts.tick_array_one_2.to_account_info(),
        ],
        remaining_accounts.supplemental_tick_arrays_one,
    );
    let mut swap_tick_sequence_one = swap_tick_sequence_one.try_build(solve_one, a_to_b_one)?;

    let swap_tick_sequence_two = SparseSwapTickSequenceBuilder::new(
        vec![
            ctx.accounts.tick_array_two_0.to_account_info(),
            ctx.accounts.tick_array_two_1.to_account_info(),
            ctx.accounts.tick_array_two_2.to_account_info(),
        ],
        remaining_accounts.supplemental_tick_arrays_two,
    );
    let mut swap_tick_sequence_two = swap_tick_sequence_two.try_build(solve_two, a_to_b_two)?;

    let oracle_accessor_one =
        OracleAccessor::new(solve_one, ctx.accounts.oracle_one.to_account_info())?;
    if !oracle_accessor_one.is_trade_enabled(timestamp)? {
        return Err(ErrorCode::TradeIsNotEnabled.into());
    }
    let adaptive_fee_info_one = oracle_accessor_one.get_adaptive_fee_info()?;

    let oracle_accessor_two =
        OracleAccessor::new(solve_two, ctx.accounts.oracle_two.to_account_info())?;
    if !oracle_accessor_two.is_trade_enabled(timestamp)? {
        return Err(ErrorCode::TradeIsNotEnabled.into());
    }
    let adaptive_fee_info_two = oracle_accessor_two.get_adaptive_fee_info()?;

    // TODO: WLOG, we could extend this to N-swaps, but the account inputs to the instruction would
    // need to be jankier and we may need to programatically map/verify rather than using anchor constraints
    let (swap_update_one, swap_update_two) = if amount_specified_is_input {
        // If the amount specified is input, this means we are doing exact-in
        // and the swap calculations occur from Swap 1 => Swap 2
        // and the swaps occur from Swap 1 => Swap 2
        let swap_calc_one = swap_with_transfer_fee_extension(
            solve_one,
            if a_to_b_one {
                &ctx.accounts.token_mint_input
            } else {
                &ctx.accounts.token_mint_intermediate
            },
            if a_to_b_one {
                &ctx.accounts.token_mint_intermediate
            } else {
                &ctx.accounts.token_mint_input
            },
            &mut swap_tick_sequence_one,
            amount,
            sqrt_price_limit_one,
            amount_specified_is_input, // true
            a_to_b_one,
            timestamp,
            &adaptive_fee_info_one,
        )?;

        // Swap two input is the output of swap one
        // We use vault to vault transfer, so transfer fee will be collected once.
        let swap_two_input_amount = if a_to_b_one {
            swap_calc_one.amount_b
        } else {
            swap_calc_one.amount_a
        };

        let swap_calc_two = swap_with_transfer_fee_extension(
            solve_two,
            if a_to_b_two {
                &ctx.accounts.token_mint_intermediate
            } else {
                &ctx.accounts.token_mint_output
            },
            if a_to_b_two {
                &ctx.accounts.token_mint_output
            } else {
                &ctx.accounts.token_mint_intermediate
            },
            &mut swap_tick_sequence_two,
            swap_two_input_amount,
            sqrt_price_limit_two,
            amount_specified_is_input, // true
            a_to_b_two,
            timestamp,
            &adaptive_fee_info_two,
        )?;
        (swap_calc_one, swap_calc_two)
    } else {
        // If the amount specified is output, this means we need to invert the ordering of the calculations
        // and the swap calculations occur from Swap 2 => Swap 1
        // but the actual swaps occur from Swap 1 => Swap 2 (to ensure that the intermediate token exists in the account)
        let swap_calc_two = swap_with_transfer_fee_extension(
            solve_two,
            if a_to_b_two {
                &ctx.accounts.token_mint_intermediate
            } else {
                &ctx.accounts.token_mint_output
            },
            if a_to_b_two {
                &ctx.accounts.token_mint_output
            } else {
                &ctx.accounts.token_mint_intermediate
            },
            &mut swap_tick_sequence_two,
            amount,
            sqrt_price_limit_two,
            amount_specified_is_input, // false
            a_to_b_two,
            timestamp,
            &adaptive_fee_info_two,
        )?;

        // The output of swap 1 is input of swap_calc_two
        let swap_one_output_amount = if a_to_b_two {
            calculate_transfer_fee_excluded_amount(
                &ctx.accounts.token_mint_intermediate,
                swap_calc_two.amount_a,
            )?
            .amount
        } else {
            calculate_transfer_fee_excluded_amount(
                &ctx.accounts.token_mint_intermediate,
                swap_calc_two.amount_b,
            )?
            .amount
        };

        let swap_calc_one = swap_with_transfer_fee_extension(
            solve_one,
            if a_to_b_one {
                &ctx.accounts.token_mint_input
            } else {
                &ctx.accounts.token_mint_intermediate
            },
            if a_to_b_one {
                &ctx.accounts.token_mint_intermediate
            } else {
                &ctx.accounts.token_mint_input
            },
            &mut swap_tick_sequence_one,
            swap_one_output_amount,
            sqrt_price_limit_one,
            amount_specified_is_input, // false
            a_to_b_one,
            timestamp,
            &adaptive_fee_info_one,
        )?;
        (swap_calc_one, swap_calc_two)
    };

    // All output token should be consumed by the second swap
    let swap_calc_one_output = if a_to_b_one {
        swap_update_one.amount_b
    } else {
        swap_update_one.amount_a
    };
    let swap_calc_two_input = if a_to_b_two {
        swap_update_two.amount_a
    } else {
        swap_update_two.amount_b
    };
    if swap_calc_one_output != swap_calc_two_input {
        return Err(ErrorCode::IntermediateTokenAmountMismatch.into());
    }

    if amount_specified_is_input {
        // If amount_specified_is_input == true, then we have a variable amount of output
        // The slippage we care about is the output of the second swap.
        let output_amount = if a_to_b_two {
            calculate_transfer_fee_excluded_amount(
                &ctx.accounts.token_mint_output,
                swap_update_two.amount_b,
            )?
            .amount
        } else {
            calculate_transfer_fee_excluded_amount(
                &ctx.accounts.token_mint_output,
                swap_update_two.amount_a,
            )?
            .amount
        };

        // If we have received less than the minimum out, throw an error
        if output_amount < other_amount_threshold {
            return Err(ErrorCode::AmountOutBelowMinimum.into());
        }
    } else {
        // amount_specified_is_output == false, then we have a variable amount of input
        // The slippage we care about is the input of the first swap
        let input_amount = if a_to_b_one {
            swap_update_one.amount_a
        } else {
            swap_update_one.amount_b
        };
        if input_amount > other_amount_threshold {
            return Err(ErrorCode::AmountInAboveMaximum.into());
        }
    }

    oracle_accessor_one.update_adaptive_fee_variables(&swap_update_one.next_adaptive_fee_info)?;

    oracle_accessor_two.update_adaptive_fee_variables(&swap_update_two.next_adaptive_fee_info)?;

    let pre_sqrt_price_one = solve_one.sqrt_price;
    let (input_amount_one, output_amount_one) = if a_to_b_one {
        (swap_update_one.amount_a, swap_update_one.amount_b)
    } else {
        (swap_update_one.amount_b, swap_update_one.amount_a)
    };
    let input_transfer_fee_one =
        calculate_transfer_fee_excluded_amount(&ctx.accounts.token_mint_input, input_amount_one)?
            .transfer_fee;
    let output_transfer_fee_one = calculate_transfer_fee_excluded_amount(
        &ctx.accounts.token_mint_intermediate,
        output_amount_one,
    )?
    .transfer_fee;
    let (lp_fee_one, protocol_fee_one) =
        (swap_update_one.lp_fee, swap_update_one.next_protocol_fee);

    let pre_sqrt_price_two = solve_two.sqrt_price;
    let (input_amount_two, output_amount_two) = if a_to_b_two {
        (swap_update_two.amount_a, swap_update_two.amount_b)
    } else {
        (swap_update_two.amount_b, swap_update_two.amount_a)
    };
    let input_transfer_fee_two = calculate_transfer_fee_excluded_amount(
        &ctx.accounts.token_mint_intermediate,
        input_amount_two,
    )?
    .transfer_fee;
    let output_transfer_fee_two =
        calculate_transfer_fee_excluded_amount(&ctx.accounts.token_mint_output, output_amount_two)?
            .transfer_fee;
    let (lp_fee_two, protocol_fee_two) =
        (swap_update_two.lp_fee, swap_update_two.next_protocol_fee);

    update_and_two_hop_swap_solve_v2(
        &swap_update_one,
        &swap_update_two,
        solve_one,
        solve_two,
        a_to_b_one,
        a_to_b_two,
        &ctx.accounts.token_mint_input,
        &ctx.accounts.token_mint_intermediate,
        &ctx.accounts.token_mint_output,
        &ctx.accounts.token_program_input,
        &ctx.accounts.token_program_intermediate,
        &ctx.accounts.token_program_output,
        &ctx.accounts.token_owner_account_input,
        &ctx.accounts.token_vault_one_input,
        &ctx.accounts.token_vault_one_intermediate,
        &ctx.accounts.token_vault_two_intermediate,
        &ctx.accounts.token_vault_two_output,
        &ctx.accounts.token_owner_account_output,
        &remaining_accounts.transfer_hook_input,
        &remaining_accounts.transfer_hook_intermediate,
        &remaining_accounts.transfer_hook_output,
        &ctx.accounts.token_authority,
        &ctx.accounts.memo_program,
        timestamp,
        transfer_memo::TRANSFER_MEMO_SWAP.as_bytes(),
    )?;

    emit!(Traded {
        solve: solve_one.key(),
        a_to_b: a_to_b_one,
        pre_sqrt_price: pre_sqrt_price_one,
        post_sqrt_price: solve_one.sqrt_price,
        input_amount: input_amount_one,
        output_amount: output_amount_one,
        input_transfer_fee: input_transfer_fee_one,
        output_transfer_fee: output_transfer_fee_one,
        lp_fee: lp_fee_one,
        protocol_fee: protocol_fee_one,
    });

    emit!(Traded {
        solve: solve_two.key(),
        a_to_b: a_to_b_two,
        pre_sqrt_price: pre_sqrt_price_two,
        post_sqrt_price: solve_two.sqrt_price,
        input_amount: input_amount_two,
        output_amount: output_amount_two,
        input_transfer_fee: input_transfer_fee_two,
        output_transfer_fee: output_transfer_fee_two,
        lp_fee: lp_fee_two,
        protocol_fee: protocol_fee_two,
    });

    Ok(())
}
