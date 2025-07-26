use anchor_lang::prelude::*;

#[account]
pub struct LockConfig {
    pub position: Pubkey,       // 32
    pub position_owner: Pubkey, // 32
    pub solve: Pubkey,          // 32
    pub locked_timestamp: u64,  // 8
    pub lock_type: LockTypeLabel, // 1
                                // 128 RESERVE
}

#[non_exhaustive]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq)]
pub enum LockType {
    Permanent,
}

// To avoid storing an enum that may be extended in the future to the account, separate the variant label and value. The value is added flatly to the account.
#[non_exhaustive]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq)]
pub enum LockTypeLabel {
    Permanent,
}

impl LockConfig {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 8 + 1 + 128;

    pub fn initialize(
        &mut self,
        position: Pubkey,
        position_owner: Pubkey,
        solve: Pubkey,
        locked_timestamp: u64,
        lock_type: LockType,
    ) -> Result<()> {
        self.position = position;
        self.position_owner = position_owner;
        self.solve = solve;
        self.locked_timestamp = locked_timestamp;
        match lock_type {
            LockType::Permanent => self.lock_type = LockTypeLabel::Permanent,
        }
        Ok(())
    }

    pub fn update_position_owner(&mut self, position_owner: Pubkey) {
        self.position_owner = position_owner;
    }
}

