use anchor_lang::prelude::*;

use crate::state::*;

#[derive(Accounts)]
#[instruction(start_tick_index: i32)]
pub struct InitializeTickArray<'info> {
    pub solve: Account<'info, Solve>,

    #[account(mut)]
    pub funder: Signer<'info>,

    #[account(
      init,
      payer = funder,
      seeds = [b"tick_array", solve.key().as_ref(), start_tick_index.to_string().as_bytes()],
      bump,
      space = FixedTickArray::LEN)]
    pub tick_array: AccountLoader<'info, FixedTickArray>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitializeTickArray>, start_tick_index: i32) -> Result<()> {
    let mut tick_array = ctx.accounts.tick_array.load_init()?;
    tick_array.initialize(&ctx.accounts.solve, start_tick_index)
}
