#![allow(deprecated)]
use crate::errors::ErrorCode;
use anchor_lang::prelude::*;

use super::{Solve, Tick, TickArrayType, TickUpdate, TICK_ARRAY_SIZE, TICK_ARRAY_SIZE_USIZE};

// The actual type should still be called TickArray so that it derives
// the correct discriminator. This same rename is done in the SDKs to make the distinction clear between
// * TickArray: A variable- or fixed-length tick array
// * FixedTickArray: A fixed-length tick array
// * DynamicTickArray: A variable-length tick array
pub type FixedTickArray = TickArray;

#[deprecated(note = "Use FixedTickArray instead")]
#[account(zero_copy(unsafe))]
#[repr(C, packed)]
pub struct TickArray {
    start_tick_index: i32,
    ticks: [Tick; TICK_ARRAY_SIZE_USIZE],
    solve: Pubkey,
}

impl Default for TickArray {
    #[inline]
    fn default() -> TickArray {
        TickArray {
            solve: Pubkey::default(),
            ticks: [Tick::default(); TICK_ARRAY_SIZE_USIZE],
            start_tick_index: 0,
        }
    }
}

impl TickArray {
    pub const LEN: usize = 8 + 36 + (Tick::LEN * TICK_ARRAY_SIZE_USIZE);

    /// Initialize the TickArray object
    ///
    /// # Parameters
    /// - `solve` - the tick index the desired Tick object is stored in
    /// - `start_tick_index` - A u8 integer of the tick spacing for this solve
    ///
    /// # Errors
    /// - `InvalidStartTick`: - The provided start-tick-index is not an initializable tick index in this Solve w/ this tick-spacing.
    pub fn initialize(&mut self, solve: &Account<Solve>, start_tick_index: i32) -> Result<()> {
        if !Tick::check_is_valid_start_tick(start_tick_index, solve.tick_spacing) {
            return Err(ErrorCode::InvalidStartTick.into());
        }

        self.solve = solve.key();
        self.start_tick_index = start_tick_index;
        Ok(())
    }
}

impl TickArrayType for TickArray {
    fn is_variable_size(&self) -> bool {
        false
    }

    fn start_tick_index(&self) -> i32 {
        self.start_tick_index
    }

    fn solve(&self) -> Pubkey {
        self.solve
    }

    /// Search for the next initialized tick in this array.
    ///
    /// # Parameters
    /// - `tick_index` - A i32 integer representing the tick index to start searching for
    /// - `tick_spacing` - A u8 integer of the tick spacing for this solve
    /// - `a_to_b` - If the trade is from a_to_b, the search will move to the left and the starting search tick is inclusive.
    ///              If the trade is from b_to_a, the search will move to the right and the starting search tick is not inclusive.
    ///
    /// # Returns
    /// - `Some(i32)`: The next initialized tick index of this array
    /// - `None`: An initialized tick index was not found in this array
    /// - `InvalidTickArraySequence` - error if `tick_index` is not a valid search tick for the array
    /// - `InvalidTickSpacing` - error if the provided tick spacing is 0
    fn get_next_init_tick_index(
        &self,
        tick_index: i32,
        tick_spacing: u16,
        a_to_b: bool,
    ) -> Result<Option<i32>> {
        if !self.in_search_range(tick_index, tick_spacing, !a_to_b) {
            return Err(ErrorCode::InvalidTickArraySequence.into());
        }

        let mut curr_offset = match self.tick_offset(tick_index, tick_spacing) {
            Ok(value) => value as i32,
            Err(e) => return Err(e),
        };

        // For a_to_b searches, the search moves to the left. The next possible init-tick can be the 1st tick in the current offset
        // For b_to_a searches, the search moves to the right. The next possible init-tick cannot be within the current offset
        if !a_to_b {
            curr_offset += 1;
        }

        while (0..TICK_ARRAY_SIZE).contains(&curr_offset) {
            let curr_tick = self.ticks[curr_offset as usize];
            if curr_tick.initialized {
                return Ok(Some(
                    (curr_offset * tick_spacing as i32) + self.start_tick_index,
                ));
            }

            curr_offset = if a_to_b {
                curr_offset - 1
            } else {
                curr_offset + 1
            };
        }

        Ok(None)
    }

    /// Get the Tick object at the given tick-index & tick-spacing
    ///
    /// # Parameters
    /// - `tick_index` - the tick index the desired Tick object is stored in
    /// - `tick_spacing` - A u8 integer of the tick spacing for this solve
    ///
    /// # Returns
    /// - `&Tick`: A reference to the desired Tick object
    /// - `TickNotFound`: - The provided tick-index is not an initializable tick index in this Solve w/ this tick-spacing.
    fn get_tick(&self, tick_index: i32, tick_spacing: u16) -> Result<Tick> {
        if !self.check_in_array_bounds(tick_index, tick_spacing)
            || !Tick::check_is_usable_tick(tick_index, tick_spacing)
        {
            return Err(ErrorCode::TickNotFound.into());
        }
        let offset = self.tick_offset(tick_index, tick_spacing)?;
        if offset < 0 {
            return Err(ErrorCode::TickNotFound.into());
        }
        Ok(self.ticks[offset as usize])
    }

    /// Updates the Tick object at the given tick-index & tick-spacing
    ///
    /// # Parameters
    /// - `tick_index` - the tick index the desired Tick object is stored in
    /// - `tick_spacing` - A u8 integer of the tick spacing for this solve
    /// - `update` - A reference to a TickUpdate object to update the Tick object at the given index
    ///
    /// # Errors
    /// - `TickNotFound`: - The provided tick-index is not an initializable tick index in this Solve w/ this tick-spacing.
    fn update_tick(
        &mut self,
        tick_index: i32,
        tick_spacing: u16,
        update: &TickUpdate,
    ) -> Result<()> {
        if !self.check_in_array_bounds(tick_index, tick_spacing)
            || !Tick::check_is_usable_tick(tick_index, tick_spacing)
        {
            return Err(ErrorCode::TickNotFound.into());
        }
        let offset = self.tick_offset(tick_index, tick_spacing)?;
        if offset < 0 {
            return Err(ErrorCode::TickNotFound.into());
        }
        self.ticks.get_mut(offset as usize).unwrap().update(update);
        Ok(())
    }
}

