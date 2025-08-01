use crate::errors::ErrorCode;
use crate::state::{Solve, TokenBadge};
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::extension::transfer_fee::{
    TransferFee, MAX_FEE_BASIS_POINTS,
};
use anchor_spl::token_interface::spl_token_2022::extension::BaseStateWithExtensions;

use anchor_spl::memo::{self, BuildMemo, Memo};
use anchor_spl::token::Token;
use anchor_spl::token_2022::spl_token_2022::{
    self,
    extension::{self, StateWithExtensions},
};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use spl_transfer_hook_interface;

#[allow(clippy::too_many_arguments)]
pub fn transfer_from_owner_to_vault_v2<'info>(
    authority: &Signer<'info>,
    token_mint: &InterfaceAccount<'info, Mint>,
    token_owner_account: &InterfaceAccount<'info, TokenAccount>,
    token_vault: &InterfaceAccount<'info, TokenAccount>,
    token_program: &Interface<'info, TokenInterface>,
    memo_program: &Program<'info, Memo>,
    transfer_hook_accounts: &Option<Vec<AccountInfo<'info>>>,
    amount: u64,
) -> Result<()> {
    // TransferFee extension
    if let Some(epoch_transfer_fee) = get_epoch_transfer_fee(token_mint)? {
        // log applied transfer fee
        // - Not must, but important for ease of investigation and replay when problems occur
        // - Use Memo because logs risk being truncated
        let transfer_fee_memo = format!(
            "TFe: {}, {}",
            u16::from(epoch_transfer_fee.transfer_fee_basis_points),
            u64::from(epoch_transfer_fee.maximum_fee),
        );
        memo::build_memo(
            CpiContext::new(memo_program.to_account_info(), BuildMemo {}),
            transfer_fee_memo.as_bytes(),
        )?;
    }

    // MemoTransfer extension
    // The vault doesn't have MemoTransfer extension, so we don't need to use memo_program here

    let mut instruction = spl_token_2022::instruction::transfer_checked(
        token_program.key,
        // owner to vault
        &token_owner_account.key(), // from (owner account)
        &token_mint.key(),          // mint
        &token_vault.key(),         // to (vault account)
        authority.key,              // authority (owner)
        &[],
        amount,
        token_mint.decimals,
    )?;

    let mut account_infos = vec![
        token_program.to_account_info(),
        // owner to vault
        token_owner_account.to_account_info(), // from (owner account)
        token_mint.to_account_info(),          // mint
        token_vault.to_account_info(),         // to (vault account)
        authority.to_account_info(),           // authority (owner)
    ];

    // TransferHook extension
    if let Some(hook_program_id) = get_transfer_hook_program_id(token_mint)? {
        if transfer_hook_accounts.is_none() {
            return Err(ErrorCode::NoExtraAccountsForTransferHook.into());
        }

        spl_transfer_hook_interface::onchain::add_extra_accounts_for_execute_cpi(
            &mut instruction,
            &mut account_infos,
            &hook_program_id,
            // owner to vault
            token_owner_account.to_account_info(), // from (owner account)
            token_mint.to_account_info(),          // mint
            token_vault.to_account_info(),         // to (vault account)
            authority.to_account_info(),           // authority (owner)
            amount,
            transfer_hook_accounts.as_ref().unwrap(),
        )?;
    }

    solana_program::program::invoke_signed(&instruction, &account_infos, &[])?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn transfer_from_vault_to_owner_v2<'info>(
    solve: &Account<'info, Solve>,
    token_mint: &InterfaceAccount<'info, Mint>,
    token_vault: &InterfaceAccount<'info, TokenAccount>,
    token_owner_account: &InterfaceAccount<'info, TokenAccount>,
    token_program: &Interface<'info, TokenInterface>,
    memo_program: &Program<'info, Memo>,
    transfer_hook_accounts: &Option<Vec<AccountInfo<'info>>>,
    amount: u64,
    memo: &[u8],
) -> Result<()> {
    // TransferFee extension
    if let Some(epoch_transfer_fee) = get_epoch_transfer_fee(token_mint)? {
        // log applied transfer fee
        // - Not must, but important for ease of investigation and replay when problems occur
        // - Use Memo because logs risk being truncated
        let transfer_fee_memo = format!(
            "TFe: {}, {}",
            u16::from(epoch_transfer_fee.transfer_fee_basis_points),
            u64::from(epoch_transfer_fee.maximum_fee),
        );
        memo::build_memo(
            CpiContext::new(memo_program.to_account_info(), BuildMemo {}),
            transfer_fee_memo.as_bytes(),
        )?;
    }

    // MemoTransfer extension
    if is_transfer_memo_required(token_owner_account)? {
        memo::build_memo(
            CpiContext::new(memo_program.to_account_info(), BuildMemo {}),
            memo,
        )?;
    }

    let mut instruction = spl_token_2022::instruction::transfer_checked(
        token_program.key,
        // vault to owner
        &token_vault.key(),         // from (vault account)
        &token_mint.key(),          // mint
        &token_owner_account.key(), // to (owner account)
        &solve.key(),               // authority (pool)
        &[],
        amount,
        token_mint.decimals,
    )?;

    let mut account_infos = vec![
        token_program.to_account_info(),
        // vault to owner
        token_vault.to_account_info(),         // from (vault account)
        token_mint.to_account_info(),          // mint
        token_owner_account.to_account_info(), // to (owner account)
        solve.to_account_info(),               // authority (pool)
    ];

    // TransferHook extension
    if let Some(hook_program_id) = get_transfer_hook_program_id(token_mint)? {
        if transfer_hook_accounts.is_none() {
            return Err(ErrorCode::NoExtraAccountsForTransferHook.into());
        }

        spl_transfer_hook_interface::onchain::add_extra_accounts_for_execute_cpi(
            &mut instruction,
            &mut account_infos,
            &hook_program_id,
            // vault to owner
            token_vault.to_account_info(), // from (vault account)
            token_mint.to_account_info(),  // mint
            token_owner_account.to_account_info(), // to (owner account)
            solve.to_account_info(),       // authority (pool)
            amount,
            transfer_hook_accounts.as_ref().unwrap(),
        )?;
    }

    solana_program::program::invoke_signed(&instruction, &account_infos, &[&solve.seeds()])?;

    Ok(())
}

fn get_transfer_hook_program_id(token_mint: &InterfaceAccount<'_, Mint>) -> Result<Option<Pubkey>> {
    let token_mint_info = token_mint.to_account_info();
    if *token_mint_info.owner == Token::id() {
        return Ok(None);
    }

    let token_mint_data = token_mint_info.try_borrow_data()?;
    let token_mint_unpacked =
        StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&token_mint_data)?;
    Ok(extension::transfer_hook::get_program_id(
        &token_mint_unpacked,
    ))
}

fn is_transfer_memo_required(token_account: &InterfaceAccount<'_, TokenAccount>) -> Result<bool> {
    let token_account_info = token_account.to_account_info();
    if *token_account_info.owner == Token::id() {
        return Ok(false);
    }

    let token_account_data = token_account_info.try_borrow_data()?;
    let token_account_unpacked =
        StateWithExtensions::<spl_token_2022::state::Account>::unpack(&token_account_data)?;
    let extension =
        token_account_unpacked.get_extension::<extension::memo_transfer::MemoTransfer>();

    if let Ok(memo_transfer) = extension {
        Ok(memo_transfer.require_incoming_transfer_memos.into())
    } else {
        Ok(false)
    }
}

pub fn is_supported_token_mint(
    token_mint: &InterfaceAccount<'_, Mint>,
    is_token_badge_initialized: bool,
) -> Result<bool> {
    let token_mint_info = token_mint.to_account_info();

    // if mint is owned by Token Program, it is supported (compatible to initialize_pool / initialize_reward)
    if *token_mint_info.owner == Token::id() {
        return Ok(true);
    }

    // now mint is owned by Token-2022 Program

    // reject native mint of Token-2022 Program to avoid SOL liquidity fragmentation
    if spl_token_2022::native_mint::check_id(&token_mint.key()) {
        return Ok(false);
    }

    // reject if mint has freeze_authority
    if token_mint.freeze_authority.is_some() && !is_token_badge_initialized {
        return Ok(false);
    }

    let token_mint_data = token_mint_info.try_borrow_data()?;
    let token_mint_unpacked =
        StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&token_mint_data)?;

    let tlv_data = token_mint_unpacked.get_tlv_data();
    let extensions = get_token_extension_types(tlv_data)?;
    for extension in extensions {
        match extension {
            // supported
            TokenExtensionType::TransferFeeConfig => {}
            TokenExtensionType::InterestBearingConfig => {}
            TokenExtensionType::TokenMetadata => {}
            TokenExtensionType::MetadataPointer => {}
            TokenExtensionType::ScaledUiAmount => {}
            // partially supported
            TokenExtensionType::ConfidentialTransferMint => {
                // Supported, but non-confidential transfer only
                //
                // SolveProgram invokes TransferChecked instruction and it supports non-confidential transfer only.
                //
                // Because the vault accounts are not configured to support confidential transfer,
                // it is impossible to send tokens directly to the vault accounts confidentially.
                // Note: Only the owner (Solve account) can call ConfidentialTransferInstruction::ConfigureAccount.
            }
            TokenExtensionType::ConfidentialTransferFeeConfig => {
                // Supported, but non-confidential transfer only
                // When both TransferFeeConfig and ConfidentialTransferMint are initialized,
                // ConfidentialTransferFeeConfig is also initialized to store encrypted transfer fee amount.
            }
            // supported if token badge is initialized
            TokenExtensionType::PermanentDelegate => {
                if !is_token_badge_initialized {
                    return Ok(false);
                }
            }
            TokenExtensionType::TransferHook => {
                if !is_token_badge_initialized {
                    return Ok(false);
                }
            }
            TokenExtensionType::MintCloseAuthority => {
                if !is_token_badge_initialized {
                    return Ok(false);
                }
            }
            TokenExtensionType::DefaultAccountState => {
                if !is_token_badge_initialized {
                    return Ok(false);
                }
            }
            TokenExtensionType::Pausable => {
                if !is_token_badge_initialized {
                    return Ok(false);
                }
            }
            // No possibility to support the following extensions
            TokenExtensionType::NonTransferable => {
                return Ok(false);
            }
            // mint has unknown or unsupported extensions
            _ => {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

pub fn is_token_badge_initialized(
    solves_config_key: Pubkey,
    token_mint_key: Pubkey,
    token_badge: &UncheckedAccount<'_>,
) -> Result<bool> {
    if *token_badge.owner != crate::id() {
        return Ok(false);
    }

    let token_badge = TokenBadge::try_deserialize(&mut token_badge.data.borrow().as_ref())?;

    Ok(token_badge.solves_config == solves_config_key && token_badge.token_mint == token_mint_key)
}

pub fn verify_supported_token_mint(
    token_mint: &InterfaceAccount<'_, Mint>,
    solves_config_key: Pubkey,
    token_badge: &UncheckedAccount<'_>,
) -> Result<()> {
    let token_badge_initialized =
        is_token_badge_initialized(solves_config_key, token_mint.key(), token_badge)?;

    if !is_supported_token_mint(token_mint, token_badge_initialized)? {
        return Err(ErrorCode::UnsupportedTokenMint.into());
    }

    Ok(())
}

#[derive(Debug)]
pub struct TransferFeeIncludedAmount {
    pub amount: u64,
    pub transfer_fee: u64,
}

#[derive(Debug)]
pub struct TransferFeeExcludedAmount {
    pub amount: u64,
    pub transfer_fee: u64,
}

pub fn calculate_transfer_fee_excluded_amount(
    token_mint: &InterfaceAccount<'_, Mint>,
    transfer_fee_included_amount: u64,
) -> Result<TransferFeeExcludedAmount> {
    if let Some(epoch_transfer_fee) = get_epoch_transfer_fee(token_mint)? {
        let transfer_fee = epoch_transfer_fee
            .calculate_fee(transfer_fee_included_amount)
            .unwrap();
        let transfer_fee_excluded_amount = transfer_fee_included_amount
            .checked_sub(transfer_fee)
            .unwrap();
        return Ok(TransferFeeExcludedAmount {
            amount: transfer_fee_excluded_amount,
            transfer_fee,
        });
    }

    Ok(TransferFeeExcludedAmount {
        amount: transfer_fee_included_amount,
        transfer_fee: 0,
    })
}

pub fn calculate_transfer_fee_included_amount(
    token_mint: &InterfaceAccount<'_, Mint>,
    transfer_fee_excluded_amount: u64,
) -> Result<TransferFeeIncludedAmount> {
    if transfer_fee_excluded_amount == 0 {
        return Ok(TransferFeeIncludedAmount {
            amount: 0,
            transfer_fee: 0,
        });
    }

    // now transfer_fee_excluded_amount > 0

    if let Some(epoch_transfer_fee) = get_epoch_transfer_fee(token_mint)? {
        let transfer_fee: u64 =
            if u16::from(epoch_transfer_fee.transfer_fee_basis_points) == MAX_FEE_BASIS_POINTS {
                // edge-case: if transfer fee rate is 100%, current SPL implementation returns 0 as inverse fee.
                // https://github.com/solana-labs/solana-program-library/blob/fe1ac9a2c4e5d85962b78c3fc6aaf028461e9026/token/program-2022/src/extension/transfer_fee/mod.rs#L95

                // But even if transfer fee is 100%, we can use maximum_fee as transfer fee.
                // if transfer_fee_excluded_amount + maximum_fee > u64 max, the following checked_add should fail.
                u64::from(epoch_transfer_fee.maximum_fee)
            } else {
                epoch_transfer_fee
                    .calculate_inverse_fee(transfer_fee_excluded_amount)
                    .ok_or(ErrorCode::TransferFeeCalculationError)?
            };

        let transfer_fee_included_amount = transfer_fee_excluded_amount
            .checked_add(transfer_fee)
            .ok_or(ErrorCode::TransferFeeCalculationError)?;

        // verify transfer fee calculation for safety
        let transfer_fee_verification = epoch_transfer_fee
            .calculate_fee(transfer_fee_included_amount)
            .unwrap();
        if transfer_fee != transfer_fee_verification {
            // We believe this should never happen
            return Err(ErrorCode::TransferFeeCalculationError.into());
        }

        return Ok(TransferFeeIncludedAmount {
            amount: transfer_fee_included_amount,
            transfer_fee,
        });
    }

    Ok(TransferFeeIncludedAmount {
        amount: transfer_fee_excluded_amount,
        transfer_fee: 0,
    })
}

pub fn get_epoch_transfer_fee(
    token_mint: &InterfaceAccount<'_, Mint>,
) -> Result<Option<TransferFee>> {
    let token_mint_info = token_mint.to_account_info();
    if *token_mint_info.owner == Token::id() {
        return Ok(None);
    }

    let token_mint_data = token_mint_info.try_borrow_data()?;
    let token_mint_unpacked =
        StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&token_mint_data)?;
    if let Ok(transfer_fee_config) =
        token_mint_unpacked.get_extension::<extension::transfer_fee::TransferFeeConfig>()
    {
        let epoch = Clock::get()?.epoch;
        return Ok(Some(*transfer_fee_config.get_epoch_fee(epoch)));
    }

    Ok(None)
}

// clone from spl-token-2022 (v9.0.0)
// https://github.com/solana-program/token-2022/blob/1c1a20cfa930058a853e15821112571b383c3e70/program/src/extension/mod.rs#L1059
// We still use Anchor 0.29.0 and old spl-token-2022 which doesn't support newer extensions.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, TryFromPrimitive, IntoPrimitive)]
enum TokenExtensionType {
    /// Used as padding if the account size would otherwise be 355, same as a
    /// multisig
    Uninitialized,
    /// Includes transfer fee rate info and accompanying authorities to withdraw
    /// and set the fee
    TransferFeeConfig,
    /// Includes withheld transfer fees
    TransferFeeAmount,
    /// Includes an optional mint close authority
    MintCloseAuthority,
    /// Auditor configuration for confidential transfers
    ConfidentialTransferMint,
    /// State for confidential transfers
    ConfidentialTransferAccount,
    /// Specifies the default Account::state for new Accounts
    DefaultAccountState,
    /// Indicates that the Account owner authority cannot be changed
    ImmutableOwner,
    /// Require inbound transfers to have memo
    MemoTransfer,
    /// Indicates that the tokens from this mint can't be transferred
    NonTransferable,
    /// Tokens accrue interest over time,
    InterestBearingConfig,
    /// Locks privileged token operations from happening via CPI
    CpiGuard,
    /// Includes an optional permanent delegate
    PermanentDelegate,
    /// Indicates that the tokens in this account belong to a non-transferable
    /// mint
    NonTransferableAccount,
    /// Mint requires a CPI to a program implementing the "transfer hook"
    /// interface
    TransferHook,
    /// Indicates that the tokens in this account belong to a mint with a
    /// transfer hook
    TransferHookAccount,
    /// Includes encrypted withheld fees and the encryption public that they are
    /// encrypted under
    ConfidentialTransferFeeConfig,
    /// Includes confidential withheld transfer fees
    ConfidentialTransferFeeAmount,
    /// Mint contains a pointer to another account (or the same account) that
    /// holds metadata
    MetadataPointer,
    /// Mint contains token-metadata
    TokenMetadata,
    /// Mint contains a pointer to another account (or the same account) that
    /// holds group configurations
    GroupPointer,
    /// Mint contains token group configurations
    TokenGroup,
    /// Mint contains a pointer to another account (or the same account) that
    /// holds group member configurations
    GroupMemberPointer,
    /// Mint contains token group member configurations
    TokenGroupMember,
    /// Mint allowing the minting and burning of confidential tokens
    ConfidentialMintBurn,
    /// Tokens whose UI amount is scaled by a given amount
    ScaledUiAmount,
    /// Tokens where minting / burning / transferring can be paused
    Pausable,
    /// Indicates that the account belongs to a pausable mint
    PausableAccount,
}

fn read_u16_le_from_slice(slice: &[u8]) -> Result<u16> {
    if slice.len() < 2 {
        return Err(ProgramError::InvalidAccountData.into());
    }
    Ok(u16::from_le_bytes(
        slice[0..2]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    ))
}

// reference implementation: get_tlv_data_info
// https://github.com/solana-program/token-2022/blob/1c1a20cfa930058a853e15821112571b383c3e70/program/src/extension/mod.rs#L203
fn get_token_extension_types(tlv_data: &[u8]) -> Result<Vec<TokenExtensionType>> {
    const TLV_TYPE_LENGTH: usize = 2;
    const TLV_LENGTH_LENGTH: usize = 2;

    let mut extension_types = Vec::new();
    let mut cursor = 0;

    while cursor < tlv_data.len() {
        let tlv_type_start = cursor;
        let tlv_length_start = tlv_type_start + TLV_TYPE_LENGTH;
        let tlv_value_start = tlv_length_start + TLV_LENGTH_LENGTH;

        if tlv_data.len() < tlv_length_start {
            // There aren't enough bytes to store the next type, which means we
            // got to the end. The last byte could be used during a realloc!
            return Ok(extension_types);
        }

        let extension_type_num =
            read_u16_le_from_slice(&tlv_data[tlv_type_start..tlv_length_start])?;
        let extension_type = TokenExtensionType::try_from(extension_type_num)
            .map_err(|_| ProgramError::InvalidAccountData)?;

        if extension_type == TokenExtensionType::Uninitialized {
            return Ok(extension_types);
        } else {
            if tlv_data.len() < tlv_value_start {
                // not enough bytes to store the length, malformed
                return Err(ProgramError::InvalidAccountData.into());
            }
            extension_types.push(extension_type);
            let length = read_u16_le_from_slice(&tlv_data[tlv_length_start..tlv_value_start])?;

            let value_end_index = tlv_value_start.saturating_add(usize::from(length));
            if value_end_index > tlv_data.len() {
                // value blows past the size of the slice, malformed
                return Err(ProgramError::InvalidAccountData.into());
            }
            cursor = value_end_index;
        }
    }

    Ok(extension_types)
}

