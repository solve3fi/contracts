use anchor_lang::prelude::*;

use crate::constants::transfer_memo;
use crate::errors::ErrorCode;
use crate::events::*;
use crate::manager::liquidity_manager::{
    calculate_liquidity_token_deltas, calculate_modify_liquidity, sync_modify_liquidity_values,
};
use crate::manager::tick_array_manager::update_tick_array_accounts;
use crate::math::convert_to_liquidity_delta;
use crate::state::TickArraysMut;
use crate::util::{
    calculate_transfer_fee_excluded_amount, is_locked_position, parse_remaining_accounts,
    AccountsType, RemainingAccountsInfo,
};
use crate::util::{
    to_timestamp_u64, v2::transfer_from_vault_to_owner_v2, verify_position_authority_interface,
};

use super::increase_liquidity::ModifyLiquidityV2;

/*
  Removes liquidity from an existing Solve Position.
*/
pub fn handler<'info>(
    ctx: Context<'_, '_, '_, 'info, ModifyLiquidityV2<'info>>,
    liquidity_amount: u128,
    token_min_a: u64,
    token_min_b: u64,
    remaining_accounts_info: Option<RemainingAccountsInfo>,
) -> Result<()> {
    verify_position_authority_interface(
        &ctx.accounts.position_token_account,
        &ctx.accounts.position_authority,
    )?;

    if is_locked_position(&ctx.accounts.position_token_account) {
        return Err(ErrorCode::OperationNotAllowedOnLockedPosition.into());
    }

    let clock = Clock::get()?;

    if liquidity_amount == 0 {
        return Err(ErrorCode::LiquidityZero.into());
    }

    // Process remaining accounts
    let remaining_accounts = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[AccountsType::TransferHookA, AccountsType::TransferHookB],
    )?;

    let liquidity_delta = convert_to_liquidity_delta(liquidity_amount, false)?;
    let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

    let mut tick_arrays = TickArraysMut::load(
        &ctx.accounts.tick_array_lower,
        &ctx.accounts.tick_array_upper,
        &ctx.accounts.solve.key(),
    )?;

    let (lower_tick_array, upper_tick_array) = tick_arrays.deref();
    let update = calculate_modify_liquidity(
        &ctx.accounts.solve,
        &ctx.accounts.position,
        lower_tick_array,
        upper_tick_array,
        liquidity_delta,
        timestamp,
    )?;

    let (lower_tick_array_mut, upper_tick_array_mut) = tick_arrays.deref_mut();
    sync_modify_liquidity_values(
        &mut ctx.accounts.solve,
        &mut ctx.accounts.position,
        lower_tick_array_mut,
        upper_tick_array_mut,
        &update,
        timestamp,
    )?;

    // Need to drop the tick arrays so we can potentially resize them
    drop(tick_arrays);

    update_tick_array_accounts(
        &ctx.accounts.position,
        ctx.accounts.tick_array_lower.to_account_info(),
        ctx.accounts.tick_array_upper.to_account_info(),
        &update.tick_array_lower_update,
        &update.tick_array_upper_update,
    )?;

    let (delta_a, delta_b) = calculate_liquidity_token_deltas(
        ctx.accounts.solve.tick_current_index,
        ctx.accounts.solve.sqrt_price,
        &ctx.accounts.position,
        liquidity_delta,
    )?;

    let transfer_fee_excluded_delta_a =
        calculate_transfer_fee_excluded_amount(&ctx.accounts.token_mint_a, delta_a)?;
    let transfer_fee_excluded_delta_b =
        calculate_transfer_fee_excluded_amount(&ctx.accounts.token_mint_b, delta_b)?;

    // token_min_a and token_min_b should be applied to the transfer fee excluded amount
    if transfer_fee_excluded_delta_a.amount < token_min_a {
        return Err(ErrorCode::TokenMinSubceeded.into());
    }
    if transfer_fee_excluded_delta_b.amount < token_min_b {
        return Err(ErrorCode::TokenMinSubceeded.into());
    }

    transfer_from_vault_to_owner_v2(
        &ctx.accounts.solve,
        &ctx.accounts.token_mint_a,
        &ctx.accounts.token_vault_a,
        &ctx.accounts.token_owner_account_a,
        &ctx.accounts.token_program_a,
        &ctx.accounts.memo_program,
        &remaining_accounts.transfer_hook_a,
        delta_a,
        transfer_memo::TRANSFER_MEMO_DECREASE_LIQUIDITY.as_bytes(),
    )?;

    transfer_from_vault_to_owner_v2(
        &ctx.accounts.solve,
        &ctx.accounts.token_mint_b,
        &ctx.accounts.token_vault_b,
        &ctx.accounts.token_owner_account_b,
        &ctx.accounts.token_program_b,
        &ctx.accounts.memo_program,
        &remaining_accounts.transfer_hook_b,
        delta_b,
        transfer_memo::TRANSFER_MEMO_DECREASE_LIQUIDITY.as_bytes(),
    )?;

    emit!(LiquidityDecreased {
        solve: ctx.accounts.solve.key(),
        position: ctx.accounts.position.key(),
        tick_lower_index: ctx.accounts.position.tick_lower_index,
        tick_upper_index: ctx.accounts.position.tick_upper_index,
        liquidity: liquidity_amount,
        token_a_amount: delta_a,
        token_b_amount: delta_b,
        token_a_transfer_fee: transfer_fee_excluded_delta_a.transfer_fee,
        token_b_transfer_fee: transfer_fee_excluded_delta_b.transfer_fee,
    });

    Ok(())
}
