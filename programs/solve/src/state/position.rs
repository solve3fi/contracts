use anchor_lang::prelude::*;

use crate::{errors::ErrorCode, math::FULL_RANGE_ONLY_TICK_SPACING_THRESHOLD, state::NUM_REWARDS};

use super::{Solve, Tick};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Copy)]
pub struct OpenPositionBumps {
    pub position_bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Copy)]
pub struct OpenPositionWithMetadataBumps {
    pub position_bump: u8,
    pub metadata_bump: u8,
}

#[account]
#[derive(Default)]
pub struct Position {
    pub solve: Pubkey,         // 32
    pub position_mint: Pubkey, // 32
    pub liquidity: u128,       // 16
    pub tick_lower_index: i32, // 4
    pub tick_upper_index: i32, // 4

    // Q64.64
    pub fee_growth_checkpoint_a: u128, // 16
    pub fee_owed_a: u64,               // 8
    // Q64.64
    pub fee_growth_checkpoint_b: u128, // 16
    pub fee_owed_b: u64,               // 8

    pub reward_infos: [PositionRewardInfo; NUM_REWARDS], // 72
}

impl Position {
    pub const LEN: usize = 8 + 136 + 72;

    pub fn is_position_empty(position: &Position) -> bool {
        let fees_not_owed = position.fee_owed_a == 0 && position.fee_owed_b == 0;
        let mut rewards_not_owed = true;
        for i in 0..NUM_REWARDS {
            rewards_not_owed = rewards_not_owed && position.reward_infos[i].amount_owed == 0
        }
        position.liquidity == 0 && fees_not_owed && rewards_not_owed
    }

    pub fn update(&mut self, update: &PositionUpdate) {
        self.liquidity = update.liquidity;
        self.fee_growth_checkpoint_a = update.fee_growth_checkpoint_a;
        self.fee_growth_checkpoint_b = update.fee_growth_checkpoint_b;
        self.fee_owed_a = update.fee_owed_a;
        self.fee_owed_b = update.fee_owed_b;
        self.reward_infos = update.reward_infos;
    }

    pub fn open_position(
        &mut self,
        solve: &Account<Solve>,
        position_mint: Pubkey,
        tick_lower_index: i32,
        tick_upper_index: i32,
    ) -> Result<()> {
        validate_tick_range_for_solve(solve, tick_lower_index, tick_upper_index)?;
        self.solve = solve.key();
        self.position_mint = position_mint;

        self.tick_lower_index = tick_lower_index;
        self.tick_upper_index = tick_upper_index;
        Ok(())
    }

    pub fn reset_fees_owed(&mut self) {
        self.fee_owed_a = 0;
        self.fee_owed_b = 0;
    }

    pub fn update_reward_owed(&mut self, index: usize, amount_owed: u64) {
        self.reward_infos[index].amount_owed = amount_owed;
    }

    pub fn reset_position_range(
        &mut self,
        solve: &Account<Solve>,
        new_tick_lower_index: i32,
        new_tick_upper_index: i32,
    ) -> Result<()> {
        // Because locked liquidity rejects positions with 0 liquidity, this check will also reject locked positions
        if !Position::is_position_empty(self) {
            return Err(ErrorCode::ClosePositionNotEmpty.into());
        }

        if new_tick_lower_index == self.tick_lower_index
            && new_tick_upper_index == self.tick_upper_index
        {
            return Err(ErrorCode::SameTickRangeNotAllowed.into());
        }

        validate_tick_range_for_solve(solve, new_tick_lower_index, new_tick_upper_index)?;

        // Wihle we could theoretically update the solve here
        // For Token Extensions positions, the NFT metadata contains the solve address
        // so it could introduce some inconsistency

        // Set new tick ranges
        self.tick_lower_index = new_tick_lower_index;
        self.tick_upper_index = new_tick_upper_index;

        // Reset the growth checkpoints
        self.fee_growth_checkpoint_a = 0;
        self.fee_growth_checkpoint_b = 0;

        // fee_owed and rewards.amount_owed should be zero due to the check above
        for i in 0..NUM_REWARDS {
            self.reward_infos[i].growth_inside_checkpoint = 0;
        }

        Ok(())
    }
}

fn validate_tick_range_for_solve(
    solve: &Account<Solve>,
    tick_lower_index: i32,
    tick_upper_index: i32,
) -> Result<()> {
    if !Tick::check_is_usable_tick(tick_lower_index, solve.tick_spacing)
        || !Tick::check_is_usable_tick(tick_upper_index, solve.tick_spacing)
        || tick_lower_index >= tick_upper_index
    {
        return Err(ErrorCode::InvalidTickIndex.into());
    }

    // On tick spacing >= 2^15, should only be able to open full range positions
    if solve.tick_spacing >= FULL_RANGE_ONLY_TICK_SPACING_THRESHOLD {
        let (full_range_lower_index, full_range_upper_index) =
            Tick::full_range_indexes(solve.tick_spacing);
        if tick_lower_index != full_range_lower_index || tick_upper_index != full_range_upper_index
        {
            return Err(ErrorCode::FullRangeOnlyPool.into());
        }
    }

    Ok(())
}

#[derive(Copy, Clone, AnchorSerialize, AnchorDeserialize, Default, Debug, PartialEq)]
pub struct PositionRewardInfo {
    // Q64.64
    pub growth_inside_checkpoint: u128,
    pub amount_owed: u64,
}

#[derive(Default, Debug, PartialEq)]
pub struct PositionUpdate {
    pub liquidity: u128,
    pub fee_growth_checkpoint_a: u128,
    pub fee_owed_a: u64,
    pub fee_growth_checkpoint_b: u128,
    pub fee_owed_b: u64,
    pub reward_infos: [PositionRewardInfo; NUM_REWARDS],
}