use std::ops::Deref;

use anchor_lang::prelude::*;

use crate::{
    manager::liquidity_manager::calculate_fee_and_reward_growths, state::*, util::to_timestamp_u64,
};

#[derive(Accounts)]
pub struct UpdateFeesAndRewards<'info> {
    #[account(mut)]
    pub solve: Account<'info, Solve>,

    #[account(mut, has_one = solve)]
    pub position: Account<'info, Position>,

    /// CHECK: Checked by the tick array loader
    pub tick_array_lower: UncheckedAccount<'info>,
    /// CHECK: Checked by the tick array loader
    pub tick_array_upper: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<UpdateFeesAndRewards>) -> Result<()> {
    let solve = &mut ctx.accounts.solve;
    let position = &mut ctx.accounts.position;
    let clock = Clock::get()?;
    let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

    let lower_tick_array = load_tick_array(&ctx.accounts.tick_array_lower, &solve.key())?;
    let upper_tick_array = load_tick_array(&ctx.accounts.tick_array_upper, &solve.key())?;

    let (position_update, reward_infos) = calculate_fee_and_reward_growths(
        solve,
        position,
        lower_tick_array.deref(),
        upper_tick_array.deref(),
        timestamp,
    )?;

    solve.update_rewards(reward_infos, timestamp);
    position.update(&position_update);

    Ok(())
}
