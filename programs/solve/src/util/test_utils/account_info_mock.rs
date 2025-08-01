use crate::state::{AdaptiveFeeConstants, FixedTickArray, Oracle, Solve};
use anchor_lang::prelude::*;
use anchor_lang::Discriminator;
use std::cell::RefCell;

pub struct AccountInfoMock {
    pub key: Pubkey,
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: Pubkey,
    pub rent_epoch: u64,
    pub executable: bool,
}

impl AccountInfoMock {
    pub fn new(key: Pubkey, data: Vec<u8>, owner: Pubkey) -> Self {
        Self {
            key,
            lamports: 0,
            data,
            owner,
            rent_epoch: 0,
            executable: false,
        }
    }

    pub fn new_solve(
        key: Pubkey,
        tick_spacing: u16,
        tick_current_index: i32,
        owner: Option<Pubkey>,
    ) -> Self {
        let solve = Solve {
            tick_spacing,
            tick_current_index,
            ..Solve::default()
        };

        let mut data = vec![0u8; Solve::LEN];
        solve.try_serialize(&mut data.as_mut_slice()).unwrap();
        Self::new(key, data, owner.unwrap_or(Solve::owner()))
    }

    pub fn new_tick_array(
        key: Pubkey,
        solve: Pubkey,
        start_tick_index: i32,
        owner: Option<Pubkey>,
    ) -> Self {
        let mut data = vec![0u8; FixedTickArray::LEN];
        data[0..8].copy_from_slice(&FixedTickArray::discriminator());
        data[8..12].copy_from_slice(&start_tick_index.to_le_bytes());
        data[9956..9988].copy_from_slice(&solve.to_bytes());
        Self::new(key, data, owner.unwrap_or(FixedTickArray::owner()))
    }

    pub fn new_oracle(
        key: Pubkey,
        solve: Pubkey,
        trade_enable_timestamp: u64,
        adaptive_fee_constants: AdaptiveFeeConstants,
        owner: Option<Pubkey>,
    ) -> Self {
        let mut af_const_data = [0u8; AdaptiveFeeConstants::LEN];
        let mut offset = 0;
        af_const_data[offset..offset + 2]
            .copy_from_slice(&adaptive_fee_constants.filter_period.to_le_bytes());
        offset += 2;
        af_const_data[offset..offset + 2]
            .copy_from_slice(&adaptive_fee_constants.decay_period.to_le_bytes());
        offset += 2;
        af_const_data[offset..offset + 2]
            .copy_from_slice(&adaptive_fee_constants.reduction_factor.to_le_bytes());
        offset += 2;
        af_const_data[offset..offset + 4].copy_from_slice(
            &adaptive_fee_constants
                .adaptive_fee_control_factor
                .to_le_bytes(),
        );
        offset += 4;
        af_const_data[offset..offset + 4].copy_from_slice(
            &adaptive_fee_constants
                .max_volatility_accumulator
                .to_le_bytes(),
        );
        offset += 4;
        af_const_data[offset..offset + 2]
            .copy_from_slice(&adaptive_fee_constants.tick_group_size.to_le_bytes());
        offset += 2;
        af_const_data[offset..offset + 2].copy_from_slice(
            &adaptive_fee_constants
                .major_swap_threshold_ticks
                .to_le_bytes(),
        );
        offset += 2;
        // reserved
        offset += 16;

        assert_eq!(offset, AdaptiveFeeConstants::LEN);

        let mut data = vec![0u8; Oracle::LEN];
        data[0..8].copy_from_slice(&Oracle::discriminator());
        data[8..40].copy_from_slice(&solve.to_bytes());
        data[40..48].copy_from_slice(&trade_enable_timestamp.to_le_bytes());
        data[48..48 + af_const_data.len()].copy_from_slice(&af_const_data);

        Self::new(key, data, owner.unwrap_or(Oracle::owner()))
    }

    pub fn to_account_info(&mut self, is_writable: bool) -> AccountInfo<'_> {
        AccountInfo {
            key: &self.key,
            is_signer: false,
            is_writable,
            lamports: std::rc::Rc::new(RefCell::new(&mut self.lamports)),
            data: std::rc::Rc::new(RefCell::new(&mut self.data)),
            owner: &self.owner,
            rent_epoch: self.rent_epoch,
            executable: self.executable,
        }
    }
}
