use solana_program::msg;

use crate::{
    errors::ErrorCode,
    manager::{solve_manager::next_solve_reward_infos, tick_manager::next_tick_cross_update},
    math::*,
    state::*,
    util::SwapTickSequence,
};
use anchor_lang::prelude::*;
use std::convert::TryInto;

#[derive(Debug)]
pub struct PostSwapUpdate {
    pub amount_a: u64,
    pub amount_b: u64,
    pub next_liquidity: u128,
    pub next_tick_index: i32,
    pub next_sqrt_price: u128,
    pub next_fee_growth_global: u128,
    pub next_reward_infos: [SolveRewardInfo; NUM_REWARDS],
    pub next_protocol_fee: u64,
    /// Value of this swap's fees in token B (raw units), for the pool's activity
    /// gate. The caller divides by the pool's vault value to get a yield.
    pub gate_fee_value_b: u128,
    /// Sum of `fee_amount` across every step of this swap, in the input token.
    /// `next_protocol_fee` is the protocol share of this same sum. Not an account field.
    pub total_fee_amount: u64,
}

pub fn swap(
    solve: &Solve,
    swap_tick_sequence: &mut SwapTickSequence,
    amount: u64,
    sqrt_price_limit: u128,
    amount_specified_is_input: bool,
    a_to_b: bool,
    timestamp: u64,
) -> Result<PostSwapUpdate> {
    let adjusted_sqrt_price_limit = if sqrt_price_limit == NO_EXPLICIT_SQRT_PRICE_LIMIT {
        if a_to_b {
            MIN_SQRT_PRICE_X64
        } else {
            MAX_SQRT_PRICE_X64
        }
    } else {
        sqrt_price_limit
    };

    if !(MIN_SQRT_PRICE_X64..=MAX_SQRT_PRICE_X64).contains(&adjusted_sqrt_price_limit) {
        return Err(ErrorCode::SqrtPriceOutOfBounds.into());
    }

    if a_to_b && adjusted_sqrt_price_limit > solve.sqrt_price
        || !a_to_b && adjusted_sqrt_price_limit < solve.sqrt_price
    {
        return Err(ErrorCode::InvalidSqrtPriceLimitDirection.into());
    }

    if amount == 0 {
        return Err(ErrorCode::ZeroTradableAmount.into());
    }

    let tick_spacing = solve.tick_spacing;
    let fee_rate = solve.fee_rate;
    let protocol_fee_rate = solve.protocol_fee_rate;
    let next_reward_infos = next_solve_reward_infos(solve, timestamp)?;

    let mut amount_remaining: u64 = amount;
    let mut amount_calculated: u64 = 0;
    let mut curr_sqrt_price = solve.sqrt_price;
    let mut curr_tick_index = solve.tick_current_index;
    let mut curr_liquidity = solve.liquidity;
    let mut curr_protocol_fee: u64 = 0;
    let mut total_fee_amount: u64 = 0;
    let mut gate_fee_value_b: u128 = 0;
    let mut curr_array_index: usize = 0;
    let mut curr_fee_growth_global_input = if a_to_b {
        solve.fee_growth_global_a
    } else {
        solve.fee_growth_global_b
    };

    while amount_remaining > 0 && adjusted_sqrt_price_limit != curr_sqrt_price {
        let (next_array_index, next_tick_index) = swap_tick_sequence
            .get_next_initialized_tick_index(
                curr_tick_index,
                tick_spacing,
                a_to_b,
                curr_array_index,
            )?;

        let (next_tick_sqrt_price, sqrt_price_target) =
            get_next_sqrt_prices(next_tick_index, adjusted_sqrt_price_limit, a_to_b);

        let swap_computation = compute_swap(
            amount_remaining,
            fee_rate,
            curr_liquidity,
            curr_sqrt_price,
            sqrt_price_target,
            amount_specified_is_input,
            a_to_b,
        )?;

        if amount_specified_is_input {
            amount_remaining = amount_remaining
                .checked_sub(swap_computation.amount_in)
                .ok_or(ErrorCode::AmountRemainingOverflow)?;
            amount_remaining = amount_remaining
                .checked_sub(swap_computation.fee_amount)
                .ok_or(ErrorCode::AmountRemainingOverflow)?;

            amount_calculated = amount_calculated
                .checked_add(swap_computation.amount_out)
                .ok_or(ErrorCode::AmountCalcOverflow)?;
        } else {
            amount_remaining = amount_remaining
                .checked_sub(swap_computation.amount_out)
                .ok_or(ErrorCode::AmountRemainingOverflow)?;

            amount_calculated = amount_calculated
                .checked_add(swap_computation.amount_in)
                .ok_or(ErrorCode::AmountCalcOverflow)?;
            amount_calculated = amount_calculated
                .checked_add(swap_computation.fee_amount)
                .ok_or(ErrorCode::AmountCalcOverflow)?;
        }

        total_fee_amount = total_fee_amount
            .checked_add(swap_computation.fee_amount)
            .ok_or(ErrorCode::AmountCalcOverflow)?;

        gate_fee_value_b = gate_fee_value_b.saturating_add(crate::state::gate::step_fee_value_b(
            swap_computation.fee_amount,
            curr_sqrt_price,
            a_to_b,
        ));

        let (next_protocol_fee, next_fee_growth_global_input) = calculate_fees(
            swap_computation.fee_amount,
            protocol_fee_rate,
            curr_liquidity,
            curr_protocol_fee,
            curr_fee_growth_global_input,
        );
        curr_protocol_fee = next_protocol_fee;
        curr_fee_growth_global_input = next_fee_growth_global_input;

        if swap_computation.next_price == next_tick_sqrt_price {
            let (next_tick, next_tick_initialized) = swap_tick_sequence
                .get_tick(next_array_index, next_tick_index, tick_spacing)
                .map_or_else(|_| (None, false), |tick| (Some(tick), tick.initialized));

            if next_tick_initialized {
                let (fee_growth_global_a, fee_growth_global_b) = if a_to_b {
                    (curr_fee_growth_global_input, solve.fee_growth_global_b)
                } else {
                    (solve.fee_growth_global_a, curr_fee_growth_global_input)
                };

                let (update, next_liquidity) = calculate_update(
                    next_tick.unwrap(),
                    a_to_b,
                    curr_liquidity,
                    fee_growth_global_a,
                    fee_growth_global_b,
                    &next_reward_infos,
                )?;

                curr_liquidity = next_liquidity;
                swap_tick_sequence.update_tick(
                    next_array_index,
                    next_tick_index,
                    tick_spacing,
                    &update,
                )?;
            }

            let tick_offset = swap_tick_sequence.get_tick_offset(
                next_array_index,
                next_tick_index,
                tick_spacing,
            )?;

            // Increment to the next tick array if either condition is true:
            //  - Price is moving left and the current tick is the start of the tick array
            //  - Price is moving right and the current tick is the end of the tick array
            curr_array_index = if (a_to_b && tick_offset == 0)
                || (!a_to_b && tick_offset == TICK_ARRAY_SIZE as isize - 1)
            {
                next_array_index + 1
            } else {
                next_array_index
            };

            // The get_init_tick search is inclusive of the current index in an a_to_b trade.
            // We therefore have to shift the index by 1 to advance to the next init tick to the left.
            curr_tick_index = if a_to_b {
                next_tick_index - 1
            } else {
                next_tick_index
            };
        } else if swap_computation.next_price != curr_sqrt_price {
            curr_tick_index = tick_index_from_sqrt_price(&swap_computation.next_price);
        }

        curr_sqrt_price = swap_computation.next_price;
    }

    // Reject partial fills if no explicit sqrt price limit is set and trade is exact out mode
    if amount_remaining > 0
        && !amount_specified_is_input
        && sqrt_price_limit == NO_EXPLICIT_SQRT_PRICE_LIMIT
    {
        return Err(ErrorCode::PartialFillError.into());
    }

    let (amount_a, amount_b) = if a_to_b == amount_specified_is_input {
        (amount - amount_remaining, amount_calculated)
    } else {
        (amount_calculated, amount - amount_remaining)
    };

    let fee_growth = if a_to_b {
        curr_fee_growth_global_input - solve.fee_growth_global_a
    } else {
        curr_fee_growth_global_input - solve.fee_growth_global_b
    };

    // Log delta in fee growth to track pool usage over time with off-chain analytics
    msg!("fee_growth: {}", fee_growth);

    Ok(PostSwapUpdate {
        amount_a,
        amount_b,
        next_liquidity: curr_liquidity,
        next_tick_index: curr_tick_index,
        next_sqrt_price: curr_sqrt_price,
        next_fee_growth_global: curr_fee_growth_global_input,
        next_reward_infos,
        next_protocol_fee: curr_protocol_fee,
        gate_fee_value_b,
        total_fee_amount,
    })
}

/// Pool fields a `PoolSwap` event must snapshot before `update_after_swap`.
#[derive(Clone, Copy, Debug)]
pub struct PoolSwapBefore {
    pub fee_rate: u16,
    pub protocol_fee_rate: u16,
    pub sqrt_price: u128,
    pub tick_index: i32,
    pub liquidity: u128,
    pub fee_growth_global: u128,
}

impl PoolSwapBefore {
    /// `fee_growth_global` is the input token's global fee growth (`a` when `a_to_b`).
    pub fn capture(solve: &Solve, a_to_b: bool) -> Self {
        Self {
            fee_rate: solve.fee_rate,
            protocol_fee_rate: solve.protocol_fee_rate,
            sqrt_price: solve.sqrt_price,
            tick_index: solve.tick_current_index,
            liquidity: solve.liquidity,
            fee_growth_global: if a_to_b {
                solve.fee_growth_global_a
            } else {
                solve.fee_growth_global_b
            },
        }
    }
}

/// Who a swap leg is attributed to. `hop` is [`crate::events::POOL_SWAP_HOP_SINGLE`],
/// [`crate::events::POOL_SWAP_HOP_FIRST`] or [`crate::events::POOL_SWAP_HOP_SECOND`].
#[derive(Clone, Copy, Debug)]
pub struct PoolSwapParties {
    pub solve: Pubkey,
    pub token_authority: Pubkey,
    pub input_account_owner: Pubkey,
    pub output_account_owner: Pubkey,
    pub a_to_b: bool,
    pub hop: u8,
}

/// Build a `PoolSwap` from the pre-update snapshot and the swap result.
/// `lp_fee` is `total_fee_amount - next_protocol_fee`. The protocol accumulator
/// starts at 0 for each swap, so the subtraction saturates only if that `u64` wrapped.
pub fn pool_swap_event(
    parties: PoolSwapParties,
    before: PoolSwapBefore,
    update: &PostSwapUpdate,
    transfer_fee_in: u64,
    transfer_fee_out: u64,
) -> crate::events::PoolSwap {
    crate::events::PoolSwap {
        solve: parties.solve,
        token_authority: parties.token_authority,
        input_account_owner: parties.input_account_owner,
        output_account_owner: parties.output_account_owner,
        a_to_b: parties.a_to_b,
        hop: parties.hop,
        amount_a: update.amount_a,
        amount_b: update.amount_b,
        transfer_fee_in,
        transfer_fee_out,
        total_fee: update.total_fee_amount,
        protocol_fee: update.next_protocol_fee,
        lp_fee: update
            .total_fee_amount
            .saturating_sub(update.next_protocol_fee),
        fee_rate: before.fee_rate,
        protocol_fee_rate: before.protocol_fee_rate,
        sqrt_price_before: before.sqrt_price,
        sqrt_price_after: update.next_sqrt_price,
        tick_before: before.tick_index,
        tick_after: update.next_tick_index,
        liquidity_before: before.liquidity,
        liquidity_after: update.next_liquidity,
        fee_growth_global_before: before.fee_growth_global,
        fee_growth_global_after: update.next_fee_growth_global,
    }
}

/// Owner of the input token account, then the output token account.
pub fn directed_owners(a_to_b: bool, owner_a: Pubkey, owner_b: Pubkey) -> (Pubkey, Pubkey) {
    if a_to_b {
        (owner_a, owner_b)
    } else {
        (owner_b, owner_a)
    }
}

fn calculate_fees(
    fee_amount: u64,
    protocol_fee_rate: u16,
    curr_liquidity: u128,
    curr_protocol_fee: u64,
    curr_fee_growth_global_input: u128,
) -> (u64, u128) {
    let mut next_protocol_fee = curr_protocol_fee;
    let mut next_fee_growth_global_input = curr_fee_growth_global_input;
    let mut global_fee = fee_amount;
    if protocol_fee_rate > 0 {
        let delta = calculate_protocol_fee(global_fee, protocol_fee_rate);
        global_fee -= delta;
        next_protocol_fee = next_protocol_fee.wrapping_add(delta);
    }

    if curr_liquidity > 0 {
        next_fee_growth_global_input = next_fee_growth_global_input
            .wrapping_add(((global_fee as u128) << Q64_RESOLUTION) / curr_liquidity);
    }
    (next_protocol_fee, next_fee_growth_global_input)
}

fn calculate_protocol_fee(global_fee: u64, protocol_fee_rate: u16) -> u64 {
    ((global_fee as u128) * (protocol_fee_rate as u128) / PROTOCOL_FEE_RATE_MUL_VALUE)
        .try_into()
        .unwrap()
}

fn calculate_update(
    tick: &Tick,
    a_to_b: bool,
    liquidity: u128,
    fee_growth_global_a: u128,
    fee_growth_global_b: u128,
    reward_infos: &[SolveRewardInfo; NUM_REWARDS],
) -> Result<(TickUpdate, u128)> {
    // Use updated fee_growth for crossing tick
    // Use -liquidity_net if going left, +liquidity_net going right
    let signed_liquidity_net = if a_to_b {
        -tick.liquidity_net
    } else {
        tick.liquidity_net
    };

    let update =
        next_tick_cross_update(tick, fee_growth_global_a, fee_growth_global_b, reward_infos)?;

    // Update the global liquidity to reflect the new current tick
    let next_liquidity = add_liquidity_delta(liquidity, signed_liquidity_net)?;

    Ok((update, next_liquidity))
}

fn get_next_sqrt_prices(
    next_tick_index: i32,
    sqrt_price_limit: u128,
    a_to_b: bool,
) -> (u128, u128) {
    let next_tick_price = sqrt_price_from_tick_index(next_tick_index);
    let next_sqrt_price_limit = if a_to_b {
        sqrt_price_limit.max(next_tick_price)
    } else {
        sqrt_price_limit.min(next_tick_price)
    };
    (next_tick_price, next_sqrt_price_limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_gauge_routes_all_fees_without_lp_fee_growth() {
        let (protocol_fee, fee_growth) =
            calculate_fees(1_000, MAX_PROTOCOL_FEE_RATE, 1u128 << Q64_RESOLUTION, 0, 0);

        assert_eq!(protocol_fee, 1_000);
        assert_eq!(fee_growth, 0);
        let (protocol_fee, fee_growth) = calculate_fees(1_000, 0, 1u128 << Q64_RESOLUTION, 0, 0);
        assert_eq!(protocol_fee, 0);
        assert_eq!(fee_growth, 1_000);
    }

    use crate::events::{POOL_SWAP_HOP_FIRST, POOL_SWAP_HOP_SECOND, POOL_SWAP_HOP_SINGLE};
    use crate::math::sqrt_price_from_tick_index;
    use crate::util::sparse_swap::ProxiedTickArray;
    use crate::util::SwapTickSequence;

    fn exact_in_swap(a_to_b: bool, protocol_fee_rate: u16, amount: u64) -> (Solve, PostSwapUpdate) {
        let tick = 1_000i32;
        let solve = Solve {
            tick_spacing: 64,
            fee_rate: 3_000,
            protocol_fee_rate,
            liquidity: 1_000_000_000,
            sqrt_price: sqrt_price_from_tick_index(tick),
            tick_current_index: tick,
            fee_growth_global_a: 11,
            fee_growth_global_b: 22,
            ..Solve::default()
        };
        // No initialized ticks: a small swap stays inside this array and does not cross.
        let array = ProxiedTickArray::new_uninitialized(0);
        let mut sequence = SwapTickSequence::new_with_proxy(array, None, None);
        let update = swap(&solve, &mut sequence, amount, 0, true, a_to_b, 0).unwrap();
        (solve, update)
    }

    fn event_for(
        solve: &Solve,
        update: &PostSwapUpdate,
        a_to_b: bool,
        hop: u8,
        transfer_fee_in: u64,
        transfer_fee_out: u64,
    ) -> crate::events::PoolSwap {
        let authority = Pubkey::new_from_array([9; 32]);
        let owner_a = Pubkey::new_from_array([10; 32]);
        let owner_b = Pubkey::new_from_array([11; 32]);
        let (input_owner, output_owner) = directed_owners(a_to_b, owner_a, owner_b);
        pool_swap_event(
            PoolSwapParties {
                solve: Pubkey::new_from_array([8; 32]),
                token_authority: authority,
                input_account_owner: input_owner,
                output_account_owner: output_owner,
                a_to_b,
                hop,
            },
            PoolSwapBefore::capture(solve, a_to_b),
            update,
            transfer_fee_in,
            transfer_fee_out,
        )
    }

    #[test]
    fn swap_event_splits_lp_and_protocol_fees() {
        for (rate, a_to_b) in [(0u16, true), (2_500, true), (10_000, false)] {
            let (solve, update) = exact_in_swap(a_to_b, rate, 1_000_000);
            let event = event_for(&solve, &update, a_to_b, POOL_SWAP_HOP_SINGLE, 0, 0);

            assert!(update.total_fee_amount > 0, "rate {rate}");
            assert!(update.total_fee_amount >= update.next_protocol_fee);
            assert_eq!(event.total_fee, update.total_fee_amount);
            assert_eq!(event.protocol_fee, update.next_protocol_fee);
            assert_eq!(event.lp_fee + event.protocol_fee, event.total_fee);
            assert_eq!(event.hop, 0);
            assert_eq!(event.transfer_fee_in, 0);
            assert_eq!(event.transfer_fee_out, 0);
            assert_eq!(event.a_to_b, a_to_b);
            assert_eq!(event.fee_rate, 3_000);
            assert_eq!(event.protocol_fee_rate, rate);
            assert_eq!(event.sqrt_price_before, solve.sqrt_price);
            assert_eq!(event.sqrt_price_after, update.next_sqrt_price);
            assert_eq!(event.tick_before, solve.tick_current_index);
            assert_eq!(event.tick_after, update.next_tick_index);
            assert_eq!(event.liquidity_before, solve.liquidity);
            assert_eq!(event.liquidity_after, update.next_liquidity);
            assert_eq!(event.amount_a, update.amount_a);
            assert_eq!(event.amount_b, update.amount_b);
            // Exact-in, fully filled: the input side is the whole specified amount.
            let input_amount = if a_to_b {
                event.amount_a
            } else {
                event.amount_b
            };
            assert_eq!(input_amount, 1_000_000);

            let growth_before = if a_to_b {
                solve.fee_growth_global_a
            } else {
                solve.fee_growth_global_b
            };
            assert_eq!(event.fee_growth_global_before, growth_before);
            let expected_growth =
                growth_before + ((event.lp_fee as u128) << Q64_RESOLUTION) / solve.liquidity;
            assert_eq!(event.fee_growth_global_after, expected_growth);
            assert_eq!(event.fee_growth_global_after, update.next_fee_growth_global);

            let mut settled = solve;
            settled.update_after_swap(
                update.next_liquidity,
                update.next_tick_index,
                update.next_sqrt_price,
                update.next_fee_growth_global,
                update.next_reward_infos,
                update.next_protocol_fee,
                a_to_b,
                0,
                update.gate_fee_value_b,
                0,
                0,
            );
            let (growth, owed) = if a_to_b {
                (settled.fee_growth_global_a, settled.protocol_fee_owed_a)
            } else {
                (settled.fee_growth_global_b, settled.protocol_fee_owed_b)
            };
            assert_eq!(event.fee_growth_global_after, growth);
            assert_eq!(event.protocol_fee, owed);

            if rate == 0 {
                assert_eq!(event.protocol_fee, 0);
                assert_eq!(event.lp_fee, event.total_fee);
                assert!(event.fee_growth_global_after > event.fee_growth_global_before);
            }
            if rate == MAX_PROTOCOL_FEE_RATE {
                assert_eq!(event.protocol_fee, event.total_fee);
                assert_eq!(event.lp_fee, 0);
                assert_eq!(
                    event.fee_growth_global_after,
                    event.fee_growth_global_before
                );
            }
            let (input_owner, output_owner) = directed_owners(
                a_to_b,
                Pubkey::new_from_array([10; 32]),
                Pubkey::new_from_array([11; 32]),
            );
            assert_eq!(event.input_account_owner, input_owner);
            assert_eq!(event.output_account_owner, output_owner);
            assert_eq!(event.token_authority, Pubkey::new_from_array([9; 32]));
            emit!(event);
        }
    }

    #[test]
    fn two_hop_builder_emits_one_event_per_leg() {
        let (first_pool, first) = exact_in_swap(true, 1_000, 500_000);
        let (second_pool, second) = exact_in_swap(false, 1_000, 500_000);
        let leg_one = event_for(&first_pool, &first, true, POOL_SWAP_HOP_FIRST, 3, 4);
        let leg_two = event_for(&second_pool, &second, false, POOL_SWAP_HOP_SECOND, 0, 6);

        assert_eq!(leg_one.hop, 1);
        assert_eq!(leg_two.hop, 2);
        assert!(leg_one.a_to_b);
        assert!(!leg_two.a_to_b);
        assert_eq!(
            leg_one.fee_growth_global_before,
            first_pool.fee_growth_global_a
        );
        assert_eq!(
            leg_two.fee_growth_global_before,
            second_pool.fee_growth_global_b
        );
        assert_eq!(leg_one.sqrt_price_before, first_pool.sqrt_price);
        assert_eq!(leg_two.sqrt_price_before, second_pool.sqrt_price);
        assert_eq!(leg_one.transfer_fee_in, 3);
        assert_eq!(leg_one.transfer_fee_out, 4);
        // Second leg of v2 two-hop does not transfer its input again.
        assert_eq!(leg_two.transfer_fee_in, 0);
        assert_eq!(leg_two.transfer_fee_out, 6);
        assert_eq!(leg_one.lp_fee + leg_one.protocol_fee, leg_one.total_fee);
        assert_eq!(leg_two.lp_fee + leg_two.protocol_fee, leg_two.total_fee);
        assert!(leg_one.total_fee > 0 && leg_two.total_fee > 0);
        emit!(leg_one);
        emit!(leg_two);
    }
}
