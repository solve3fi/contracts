use crate::errors::ErrorCode;
use crate::math::MAX_FEE_RATE;
use crate::state::SolvesConfig;
use anchor_lang::prelude::*;

use super::AdaptiveFeeConstants;

#[account]
pub struct AdaptiveFeeTier {
    pub solves_config: Pubkey,
    pub fee_tier_index: u16,

    pub tick_spacing: u16,

    // authority who can use this adaptive fee tier
    pub initialize_pool_authority: Pubkey,

    // delegation
    pub delegated_fee_authority: Pubkey,

    // base fee
    pub default_base_fee_rate: u16,

    // adaptive fee constants
    pub filter_period: u16,
    pub decay_period: u16,
    pub reduction_factor: u16,
    pub adaptive_fee_control_factor: u32,
    pub max_volatility_accumulator: u32,
    pub tick_group_size: u16,
    pub major_swap_threshold_ticks: u16,
    // 128 RESERVE
}

impl AdaptiveFeeTier {
    pub const LEN: usize = 8 + 32 + 2 + 2 + 32 + 32 + 2 + 2 + 2 + 2 + 4 + 4 + 2 + 2 + 128;

    #[allow(clippy::too_many_arguments)]
    pub fn initialize(
        &mut self,
        solves_config: &Account<SolvesConfig>,
        fee_tier_index: u16,
        tick_spacing: u16,
        initialize_pool_authority: Pubkey,
        delegated_fee_authority: Pubkey,
        default_base_fee_rate: u16,
        filter_period: u16,
        decay_period: u16,
        reduction_factor: u16,
        adaptive_fee_control_factor: u32,
        max_volatility_accumulator: u32,
        tick_group_size: u16,
        major_swap_threshold_ticks: u16,
    ) -> Result<()> {
        if fee_tier_index == tick_spacing {
            // fee_tier_index == tick_spacing is reserved for FeeTier account
            return Err(ErrorCode::InvalidFeeTierIndex.into());
        }

        if tick_spacing == 0 {
            return Err(ErrorCode::InvalidTickSpacing.into());
        }

        self.solves_config = solves_config.key();
        self.fee_tier_index = fee_tier_index;

        self.tick_spacing = tick_spacing;

        self.update_default_base_fee_rate(default_base_fee_rate)?;

        self.update_initialize_pool_authority(initialize_pool_authority);
        self.update_delegated_fee_authority(delegated_fee_authority);

        self.update_adaptive_fee_constants(
            filter_period,
            decay_period,
            reduction_factor,
            adaptive_fee_control_factor,
            max_volatility_accumulator,
            tick_group_size,
            major_swap_threshold_ticks,
        )?;

        Ok(())
    }

    pub fn update_initialize_pool_authority(&mut self, initialize_pool_authority: Pubkey) {
        self.initialize_pool_authority = initialize_pool_authority;
    }

    pub fn update_delegated_fee_authority(&mut self, delegated_fee_authority: Pubkey) {
        self.delegated_fee_authority = delegated_fee_authority;
    }

    pub fn update_default_base_fee_rate(&mut self, default_base_fee_rate: u16) -> Result<()> {
        if default_base_fee_rate > MAX_FEE_RATE {
            return Err(ErrorCode::FeeRateMaxExceeded.into());
        }
        self.default_base_fee_rate = default_base_fee_rate;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_adaptive_fee_constants(
        &mut self,
        filter_period: u16,
        decay_period: u16,
        reduction_factor: u16,
        adaptive_fee_control_factor: u32,
        max_volatility_accumulator: u32,
        tick_group_size: u16,
        major_swap_threshold_ticks: u16,
    ) -> Result<()> {
        if !AdaptiveFeeConstants::validate_constants(
            self.tick_spacing,
            filter_period,
            decay_period,
            reduction_factor,
            adaptive_fee_control_factor,
            max_volatility_accumulator,
            tick_group_size,
            major_swap_threshold_ticks,
        ) {
            return Err(ErrorCode::InvalidAdaptiveFeeConstants.into());
        }

        self.filter_period = filter_period;
        self.decay_period = decay_period;
        self.reduction_factor = reduction_factor;
        self.adaptive_fee_control_factor = adaptive_fee_control_factor;
        self.max_volatility_accumulator = max_volatility_accumulator;
        self.tick_group_size = tick_group_size;
        self.major_swap_threshold_ticks = major_swap_threshold_ticks;

        Ok(())
    }

    pub fn is_valid_initialize_pool_authority(&self, initialize_pool_authority: Pubkey) -> bool {
        // no authority is set (permission-less)
        if self.initialize_pool_authority == Pubkey::default() {
            return true;
        }
        self.initialize_pool_authority == initialize_pool_authority
    }

    pub fn is_permissioned(&self) -> bool {
        self.initialize_pool_authority != Pubkey::default()
    }
}

