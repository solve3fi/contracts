use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::{
    errors::ErrorCode,
    events::*,
    state::*,
    util::{initialize_vault_token_account, to_timestamp_u64, verify_supported_token_mint},
};

#[derive(Accounts)]
pub struct InitializePoolWithAdaptiveFee<'info> {
    pub solves_config: Box<Account<'info, SolvesConfig>>,

    pub token_mint_a: Box<InterfaceAccount<'info, Mint>>,
    pub token_mint_b: Box<InterfaceAccount<'info, Mint>>,

    #[account(seeds = [b"token_badge", solves_config.key().as_ref(), token_mint_a.key().as_ref()], bump)]
    /// CHECK: checked in the handler
    pub token_badge_a: UncheckedAccount<'info>,
    #[account(seeds = [b"token_badge", solves_config.key().as_ref(), token_mint_b.key().as_ref()], bump)]
    /// CHECK: checked in the handler
    pub token_badge_b: UncheckedAccount<'info>,

    #[account(mut)]
    pub funder: Signer<'info>,

    #[account(constraint = adaptive_fee_tier.is_valid_initialize_pool_authority(initialize_pool_authority.key()))]
    pub initialize_pool_authority: Signer<'info>,

    #[account(init,
      seeds = [
        b"solve".as_ref(),
        solves_config.key().as_ref(),
        token_mint_a.key().as_ref(),
        token_mint_b.key().as_ref(),
        adaptive_fee_tier.fee_tier_index.to_le_bytes().as_ref()
      ],
      bump,
      payer = funder,
      space = Solve::LEN)]
    pub solve: Box<Account<'info, Solve>>,

    #[account(
        init,
        payer = funder,
        seeds = [b"oracle", solve.key().as_ref()],
        bump,
        space = Oracle::LEN)]
    pub oracle: AccountLoader<'info, Oracle>,

    /// CHECK: initialized in the handler
    #[account(mut)]
    pub token_vault_a: Signer<'info>,

    /// CHECK: initialized in the handler
    #[account(mut)]
    pub token_vault_b: Signer<'info>,

    #[account(has_one = solves_config)]
    pub adaptive_fee_tier: Box<Account<'info, AdaptiveFeeTier>>,

    #[account(address = *token_mint_a.to_account_info().owner)]
    pub token_program_a: Interface<'info, TokenInterface>,
    #[account(address = *token_mint_b.to_account_info().owner)]
    pub token_program_b: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<InitializePoolWithAdaptiveFee>,
    initial_sqrt_price: u128,
    trade_enable_timestamp: Option<u64>,
) -> Result<()> {
    let token_mint_a = ctx.accounts.token_mint_a.key();
    let token_mint_b = ctx.accounts.token_mint_b.key();

    let solve = &mut ctx.accounts.solve;
    let solves_config = &ctx.accounts.solves_config;

    let fee_tier_index = ctx.accounts.adaptive_fee_tier.fee_tier_index;

    let tick_spacing = ctx.accounts.adaptive_fee_tier.tick_spacing;

    let default_fee_rate = ctx.accounts.adaptive_fee_tier.default_base_fee_rate;

    // ignore the bump passed and use one Anchor derived
    let bump = ctx.bumps.solve;

    // Don't allow creating a pool with unsupported token mints
    verify_supported_token_mint(
        &ctx.accounts.token_mint_a,
        solves_config.key(),
        &ctx.accounts.token_badge_a,
    )?;
    verify_supported_token_mint(
        &ctx.accounts.token_mint_b,
        solves_config.key(),
        &ctx.accounts.token_badge_b,
    )?;

    // Don't allow setting trade_enable_timestamp for permission-less adaptive fee tier
    let clock = Clock::get()?;
    let timestamp = to_timestamp_u64(clock.unix_timestamp)?;
    if !is_valid_trade_enable_timestamp(
        trade_enable_timestamp,
        timestamp,
        ctx.accounts.adaptive_fee_tier.is_permissioned(),
    ) {
        return Err(ErrorCode::InvalidTradeEnableTimestamp.into());
    }

    initialize_vault_token_account(
        solve,
        &ctx.accounts.token_vault_a,
        &ctx.accounts.token_mint_a,
        &ctx.accounts.funder,
        &ctx.accounts.token_program_a,
        &ctx.accounts.system_program,
    )?;
    initialize_vault_token_account(
        solve,
        &ctx.accounts.token_vault_b,
        &ctx.accounts.token_mint_b,
        &ctx.accounts.funder,
        &ctx.accounts.token_program_b,
        &ctx.accounts.system_program,
    )?;

    solve.initialize(
        solves_config,
        fee_tier_index,
        bump,
        tick_spacing,
        initial_sqrt_price,
        default_fee_rate,
        token_mint_a,
        ctx.accounts.token_vault_a.key(),
        token_mint_b,
        ctx.accounts.token_vault_b.key(),
    )?;

    let mut oracle = ctx.accounts.oracle.load_init()?;
    oracle.initialize(
        ctx.accounts.solve.key(),
        trade_enable_timestamp,
        tick_spacing,
        ctx.accounts.adaptive_fee_tier.filter_period,
        ctx.accounts.adaptive_fee_tier.decay_period,
        ctx.accounts.adaptive_fee_tier.reduction_factor,
        ctx.accounts.adaptive_fee_tier.adaptive_fee_control_factor,
        ctx.accounts.adaptive_fee_tier.max_volatility_accumulator,
        ctx.accounts.adaptive_fee_tier.tick_group_size,
        ctx.accounts.adaptive_fee_tier.major_swap_threshold_ticks,
    )?;

    emit!(PoolInitialized {
        solve: ctx.accounts.solve.key(),
        solves_config: ctx.accounts.solves_config.key(),
        token_mint_a: ctx.accounts.token_mint_a.key(),
        token_mint_b: ctx.accounts.token_mint_b.key(),
        tick_spacing,
        token_program_a: ctx.accounts.token_program_a.key(),
        token_program_b: ctx.accounts.token_program_b.key(),
        decimals_a: ctx.accounts.token_mint_a.decimals,
        decimals_b: ctx.accounts.token_mint_b.decimals,
        initial_sqrt_price,
    });

    Ok(())
}

fn is_valid_trade_enable_timestamp(
    trade_enable_timestamp: Option<u64>,
    current_timestamp: u64,
    is_permissioned_adaptive_fee_tier: bool,
) -> bool {
    match trade_enable_timestamp {
        None => true,
        Some(trade_enable_timestamp) => {
            if !is_permissioned_adaptive_fee_tier {
                // If the adaptive fee tier is permission-less, trade_enable_timestamp is not allowed
                false
            } else if trade_enable_timestamp > current_timestamp {
                // reject far future timestamp
                trade_enable_timestamp - current_timestamp <= MAX_TRADE_ENABLE_TIMESTAMP_DELTA
            } else {
                // reject too old timestamp (> 30 seconds)
                // if pool initialize authority want to enable trading immediately, trade_enable_timestamp should be set to None
                current_timestamp - trade_enable_timestamp <= 30
            }
        }
    }
}
