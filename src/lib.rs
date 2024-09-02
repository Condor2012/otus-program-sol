use std::io::Write;

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{account_info::{next_account_info, AccountInfo}, entrypoint, entrypoint::ProgramResult, msg, program::{invoke, invoke_signed}, program_error::ProgramError, pubkey::Pubkey, rent::Rent, system_instruction, system_program, sysvar::Sysvar};

const ADMIN_ACCOUNT_ID: &str = "HWd8ZyEzy7exV7UGLBb6Hf1it54WNPXtK5sMivepDmP";

#[derive(BorshSerialize, BorshDeserialize, Debug)]
struct Invoice {
    id: u128,
    amount: u64,
    paid: bool,
    destination: [u8; 32],
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
enum InstructionData {
    PayInvoice,
    CreateInvoice(Invoice)
}

entrypoint!(process_instruction);
fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    match InstructionData::try_from_slice(instruction_data)? {
        InstructionData::PayInvoice => pay_invoice(accounts),
        InstructionData::CreateInvoice(invoice) => create_invoice(program_id, accounts, invoice),
    }
}

/// Accounts:
///
/// 0. `[signer, writable]` Debit lamports from this account
/// 1. `[writable]` PDA account with payment data
/// 2. `[writable]` Destination account
/// 3. `[]` System program
fn pay_invoice(
    accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();

    let sender = next_account_info(accounts_iter)?;
    let pda = next_account_info(accounts_iter)?;
    let destination = next_account_info(accounts_iter)?;
    let system_program = next_account_info(accounts_iter)?;

    if !sender.is_signer {
        msg!("sender isn't a transaction signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if pda.data_is_empty() {
        msg!("pda is empty");
        return Err(ProgramError::InvalidAccountData);
    }

    if !system_program::check_id(system_program.key) {
        msg!("unknown program was passed instead of system program");
        return Err(ProgramError::InvalidArgument);
    }

    let mut invoice = Invoice::try_from_slice(&pda.data.borrow())?;

    if destination.key.to_string() != Pubkey::new_from_array(invoice.destination).to_string() {
        msg!("destination wallet is invalid");
        return Err(ProgramError::InvalidArgument);
    }

    let instruction = system_instruction::transfer(
        sender.key, destination.key,
        invoice.amount,
    );
    invoke(&instruction, &[sender.clone(), destination.clone()])?;

    invoice.paid = true;

    let mut data = pda.data.borrow_mut();
    invoice.serialize(data.as_mut().by_ref())?;

    Ok(())
}

/// Accounts:
///
/// 0. `[signer, writable]` Admin account
/// 1. `[writable]` PDA account to write invoice data
/// 2. `[]` System program
/// 3. `[]` Sysvar rent program
fn create_invoice(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    invoice: Invoice,
) -> ProgramResult {
    let accounts = &mut accounts.iter();

    let admin = next_account_info(accounts)?;
    let pda = next_account_info(accounts)?;
    let system_program = next_account_info(accounts)?;
    let sysvar_rent_program = next_account_info(accounts)?;

    if admin.key.to_string() != ADMIN_ACCOUNT_ID.to_string() {
        msg!("access denied. Invalid admin account");
        return Err(ProgramError::InvalidArgument);
    }

    if !admin.is_signer {
        msg!("access denied. Admin isn't a transaction signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    let id = invoice.id.to_be_bytes();
    let (_, seed) = Pubkey::find_program_address(&[&id], &program_id);
    let signer_seeds: &[&[_]] = &[&id, &[seed]];

    let space = borsh::object_length(&invoice)?;
    let rent = Rent::from_account_info(sysvar_rent_program)?;
    let minimum_balance = rent.minimum_balance(space);

    invoke_signed(
        &system_instruction::create_account(
            admin.key,
            pda.key,
            minimum_balance,
            space as u64,
            program_id,
        ),
        &[admin.clone(), pda.clone(), system_program.clone()],
        &[&signer_seeds],
    )?;

    let mut data = pda.data.borrow_mut();

    invoice.serialize(data.as_mut().by_ref())?;

    Ok(())
}