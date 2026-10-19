use anchor_lang::prelude::*;

// Position, liquidity, fee and swap events. The Apache base emits none. PositionOpened,
// LiquidityIncreased and LiquidityDecreased keep the layouts the Quest decoder already reads.
// FeesCollected and PoolSwap are Solve3-original (not Orca's post-cutoff Traded event).
// All of them use `emit!` (a `Program data:` log). Discriminators are sha256("event:<Name>")[..8].
// Field order is part of the format and is frozen once deployed.

/// `PoolSwap.hop` for a single-pool swap.
pub const POOL_SWAP_HOP_SINGLE: u8 = 0;
/// `PoolSwap.hop` for the first leg of a two-hop swap.
pub const POOL_SWAP_HOP_FIRST: u8 = 1;
/// `PoolSwap.hop` for the second leg of a two-hop swap.
pub const POOL_SWAP_HOP_SECOND: u8 = 2;

#[event]
pub struct PositionOpened {
    pub solve: Pubkey,
    pub position: Pubkey,
    pub tick_lower_index: i32,
    pub tick_upper_index: i32,
}

#[event]
pub struct LiquidityIncreased {
    pub solve: Pubkey,
    pub position: Pubkey,
    pub tick_lower_index: i32,
    pub tick_upper_index: i32,
    pub liquidity: u128,
    pub token_a_amount: u64,
    pub token_b_amount: u64,
    pub token_a_transfer_fee: u64,
    pub token_b_transfer_fee: u64,
}

#[event]
pub struct LiquidityDecreased {
    pub solve: Pubkey,
    pub position: Pubkey,
    pub tick_lower_index: i32,
    pub tick_upper_index: i32,
    pub liquidity: u128,
    pub token_a_amount: u64,
    pub token_b_amount: u64,
    pub token_a_transfer_fee: u64,
    pub token_b_transfer_fee: u64,
}

/// Fees moved from a position's `fee_owed` to recipient token accounts.
///
/// Emitted after both transfers in `collect_fees` and `v2::collect_fees`, including
/// a claim of zero. `fee_a` / `fee_b` are the gross amounts debited from the position,
/// before any Token-2022 transfer fee. v1 sets the transfer-fee fields to 0.
#[event]
pub struct FeesCollected {
    pub solve: Pubkey,
    pub position: Pubkey,
    pub position_mint: Pubkey,
    pub position_authority: Pubkey,
    pub position_token_owner: Pubkey,
    pub token_owner_account_a: Pubkey,
    pub token_owner_account_b: Pubkey,
    pub fee_a: u64,
    pub fee_b: u64,
    pub token_a_transfer_fee: u64,
    pub token_b_transfer_fee: u64,
    pub liquidity: u128,
    pub fee_growth_checkpoint_a: u128,
    pub fee_growth_checkpoint_b: u128,
}

/// One swap leg. `hop` is 0 for `swap`, or 1 then 2 for the two legs of a two-hop swap.
///
/// `total_fee` is the sum of per-step swap fees in the input token. `protocol_fee` is
/// this leg's protocol share and `lp_fee` is the remainder. `fee_growth_global_*` is the
/// input token's global fee growth. v1 sets both transfer-fee fields to 0.
#[event]
pub struct PoolSwap {
    pub solve: Pubkey,
    pub token_authority: Pubkey,
    pub input_account_owner: Pubkey,
    pub output_account_owner: Pubkey,
    pub a_to_b: bool,
    pub hop: u8,
    pub amount_a: u64,
    pub amount_b: u64,
    pub transfer_fee_in: u64,
    pub transfer_fee_out: u64,
    pub total_fee: u64,
    pub protocol_fee: u64,
    pub lp_fee: u64,
    pub fee_rate: u16,
    pub protocol_fee_rate: u16,
    pub sqrt_price_before: u128,
    pub sqrt_price_after: u128,
    pub tick_before: i32,
    pub tick_after: i32,
    pub liquidity_before: u128,
    pub liquidity_after: u128,
    pub fee_growth_global_before: u128,
    pub fee_growth_global_after: u128,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::{AnchorDeserialize, Event};

    /// sha256("event:<Name>")[..8], the discriminator `emit!` writes ahead of the borsh body.
    fn event_discriminator(name: &str) -> [u8; 8] {
        let label = format!("event:{name}");
        let hash = solana_program::hash::hash(label.as_bytes());
        let mut out = [0u8; 8];
        out.copy_from_slice(&hash.to_bytes()[..8]);
        out
    }

    fn assert_disc<T: anchor_lang::Discriminator>(name: &str, expected: [u8; 8]) {
        let computed = event_discriminator(name);
        assert_eq!(
            computed, expected,
            "{name} discriminator drifted from the published bytes"
        );
        assert_eq!(
            T::DISCRIMINATOR,
            &computed[..],
            "{name} IDL discriminator does not match sha256(\"event:{name}\")"
        );
    }

    fn pk(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn push_pk(buf: &mut Vec<u8>, key: Pubkey) {
        buf.extend_from_slice(key.as_ref());
    }

    #[test]
    fn quest_position_events_keep_their_discriminators_and_layout() {
        // These three are what the Quest decoder already reads. The bytes are the
        // historical IDL values (PositionOpened was checked against idl/solve.json).
        assert_disc::<PositionOpened>("PositionOpened", [237, 175, 243, 230, 147, 117, 101, 121]);
        assert_disc::<LiquidityIncreased>(
            "LiquidityIncreased",
            event_discriminator("LiquidityIncreased"),
        );
        assert_disc::<LiquidityDecreased>(
            "LiquidityDecreased",
            event_discriminator("LiquidityDecreased"),
        );

        let opened = PositionOpened {
            solve: pk(1),
            position: pk(2),
            tick_lower_index: -128,
            tick_upper_index: 64,
        };
        let mut body = Vec::new();
        push_pk(&mut body, pk(1));
        push_pk(&mut body, pk(2));
        body.extend_from_slice(&(-128i32).to_le_bytes());
        body.extend_from_slice(&64i32.to_le_bytes());
        let data = opened.data();
        assert_eq!(&data[..8], &event_discriminator("PositionOpened")[..]);
        assert_eq!(&data[8..], &body[..]);
        assert_eq!(data.len(), 8 + 32 + 32 + 4 + 4);

        let increased = LiquidityIncreased {
            solve: pk(1),
            position: pk(2),
            tick_lower_index: 0,
            tick_upper_index: 128,
            liquidity: 9,
            token_a_amount: 3,
            token_b_amount: 4,
            token_a_transfer_fee: 0,
            token_b_transfer_fee: 0,
        };
        assert_eq!(
            increased.data().len(),
            8 + 32 + 32 + 4 + 4 + 16 + 8 + 8 + 8 + 8
        );
        let mut increased_body = Vec::new();
        push_pk(&mut increased_body, pk(1));
        push_pk(&mut increased_body, pk(2));
        increased_body.extend_from_slice(&0i32.to_le_bytes());
        increased_body.extend_from_slice(&128i32.to_le_bytes());
        increased_body.extend_from_slice(&9u128.to_le_bytes());
        increased_body.extend_from_slice(&3u64.to_le_bytes());
        increased_body.extend_from_slice(&4u64.to_le_bytes());
        increased_body.extend_from_slice(&0u64.to_le_bytes());
        increased_body.extend_from_slice(&0u64.to_le_bytes());
        assert_eq!(&increased.data()[8..], &increased_body[..]);
        let decreased = LiquidityDecreased {
            solve: pk(1),
            position: pk(2),
            tick_lower_index: 0,
            tick_upper_index: 128,
            liquidity: 9,
            token_a_amount: 3,
            token_b_amount: 4,
            token_a_transfer_fee: 1,
            token_b_transfer_fee: 2,
        };
        assert_eq!(decreased.data().len(), increased.data().len());
        assert_ne!(increased.data()[..8], decreased.data()[..8]);
    }

    #[test]
    fn fees_collected_and_pool_swap_match_the_published_spec() {
        assert_disc::<FeesCollected>("FeesCollected", [233, 23, 117, 225, 107, 178, 254, 8]);
        assert_disc::<PoolSwap>("PoolSwap", [58, 96, 47, 75, 176, 73, 145, 4]);
        assert_eq!(POOL_SWAP_HOP_SINGLE, 0);
        assert_eq!(POOL_SWAP_HOP_FIRST, 1);
        assert_eq!(POOL_SWAP_HOP_SECOND, 2);

        // A zero claim is still an event. Indexers treat fee_a == fee_b == 0 as "no claim".
        let zero = FeesCollected {
            solve: pk(1),
            position: pk(2),
            position_mint: pk(3),
            position_authority: pk(4),
            position_token_owner: pk(5),
            token_owner_account_a: pk(6),
            token_owner_account_b: pk(7),
            fee_a: 0,
            fee_b: 0,
            token_a_transfer_fee: 0,
            token_b_transfer_fee: 0,
            liquidity: 0,
            fee_growth_checkpoint_a: 0,
            fee_growth_checkpoint_b: 0,
        };
        assert_eq!(zero.data().len(), 312);

        let collected = FeesCollected {
            solve: pk(1),
            position: pk(2),
            position_mint: pk(3),
            position_authority: pk(4),
            position_token_owner: pk(5),
            token_owner_account_a: pk(6),
            token_owner_account_b: pk(7),
            fee_a: 100,
            fee_b: 250,
            token_a_transfer_fee: 1,
            token_b_transfer_fee: 2,
            liquidity: 1_000,
            fee_growth_checkpoint_a: 7,
            fee_growth_checkpoint_b: 8,
        };
        let mut body = Vec::new();
        for byte in 1..=7 {
            push_pk(&mut body, pk(byte));
        }
        body.extend_from_slice(&100u64.to_le_bytes());
        body.extend_from_slice(&250u64.to_le_bytes());
        body.extend_from_slice(&1u64.to_le_bytes());
        body.extend_from_slice(&2u64.to_le_bytes());
        body.extend_from_slice(&1_000u128.to_le_bytes());
        body.extend_from_slice(&7u128.to_le_bytes());
        body.extend_from_slice(&8u128.to_le_bytes());
        let data = collected.data();
        assert_eq!(data.len(), 312, "8 + 7*32 + 4*8 + 3*16");
        assert_eq!(&data[8..], &body[..]);
        let decoded = FeesCollected::try_from_slice(&data[8..]).unwrap();
        assert_eq!(decoded.fee_a, 100);
        assert_eq!(decoded.position_token_owner, pk(5));
        assert_eq!(decoded.position_mint, pk(3));
        assert_eq!(decoded.fee_growth_checkpoint_b, 8);
        // The log line the Quest decoder and the fees indexer both see.
        emit!(collected);

        let swapped = PoolSwap {
            solve: pk(1),
            token_authority: pk(2),
            input_account_owner: pk(3),
            output_account_owner: pk(4),
            a_to_b: false,
            hop: POOL_SWAP_HOP_SECOND,
            amount_a: 10,
            amount_b: 20,
            transfer_fee_in: 1,
            transfer_fee_out: 2,
            total_fee: 5,
            protocol_fee: 1,
            lp_fee: 4,
            fee_rate: 3_000,
            protocol_fee_rate: 2_000,
            sqrt_price_before: 100,
            sqrt_price_after: 90,
            tick_before: 15,
            tick_after: -4,
            liquidity_before: 50,
            liquidity_after: 40,
            fee_growth_global_before: 1,
            fee_growth_global_after: 9,
        };
        let mut swap_body = Vec::new();
        for byte in 1..=4 {
            push_pk(&mut swap_body, pk(byte));
        }
        swap_body.push(0); // a_to_b == false
        swap_body.push(POOL_SWAP_HOP_SECOND);
        for n in [10u64, 20, 1, 2, 5, 1, 4] {
            swap_body.extend_from_slice(&n.to_le_bytes());
        }
        swap_body.extend_from_slice(&3_000u16.to_le_bytes());
        swap_body.extend_from_slice(&2_000u16.to_le_bytes());
        swap_body.extend_from_slice(&100u128.to_le_bytes());
        swap_body.extend_from_slice(&90u128.to_le_bytes());
        swap_body.extend_from_slice(&15i32.to_le_bytes());
        swap_body.extend_from_slice(&(-4i32).to_le_bytes());
        swap_body.extend_from_slice(&50u128.to_le_bytes());
        swap_body.extend_from_slice(&40u128.to_le_bytes());
        swap_body.extend_from_slice(&1u128.to_le_bytes());
        swap_body.extend_from_slice(&9u128.to_le_bytes());
        let swap_data = swapped.data();
        assert_eq!(
            swap_data.len(),
            302,
            "8 + 4*32 + 1 + 1 + 7*8 + 2*2 + 6*16 + 2*4"
        );
        assert_eq!(&swap_data[8..], &swap_body[..]);
        let decoded = PoolSwap::try_from_slice(&swap_data[8..]).unwrap();
        assert!(!decoded.a_to_b);
        assert_eq!(decoded.hop, 2);
        assert_eq!(decoded.lp_fee + decoded.protocol_fee, decoded.total_fee);
        assert_eq!(decoded.tick_after, -4);
        assert_eq!(decoded.input_account_owner, pk(3));
        emit!(swapped);
    }
}
