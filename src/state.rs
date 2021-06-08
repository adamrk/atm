use crate::transaction::{Action, Client, Transaction, Tx};
use csv::Writer;
use std::{collections::HashMap, io::Write};

/// The information associated to a deposit that we need to save in case it
/// is disputed/resolved/charged back.
#[derive(Debug, PartialEq)]
struct DepositDetail {
    amount: u64,
    under_dispute: bool,
}

/// The state of a single client account.
///
/// # Invariant
///
/// The total amount of all transactions under dispute should be equal to the
/// the amount `held` _if_ the account isn't locked. If the accoun is locked,
/// then there is no guarantee about `held` relating to the disputed
/// transactions.
#[derive(Debug)]
struct Account {
    client: Client,
    held: u64,
    available: u64,
    locked: bool,
    transactions: HashMap<Tx, DepositDetail>,
}

impl Account {
    /// Create a new empty account.
    pub(crate) fn new(client: Client) -> Self {
        // INVARIANT: No transactions are under dispute and `held` is 0.
        Account {
            client,
            held: 0,
            available: 0,
            locked: false,
            transactions: HashMap::new(),
        }
    }

    fn lookup_transaction(
        &mut self,
        tx: Tx,
        expect_disputed: bool,
    ) -> Result<&mut DepositDetail, String> {
        let transaction = self
            .transactions
            .get_mut(&tx)
            .ok_or_else(|| format!("Transaction was not found: {:?}", tx))?;
        if expect_disputed && !transaction.under_dispute {
            return Err(format!(
                "Transaction is not under dispute: {:?}",
                transaction
            ));
        } else if !expect_disputed && transaction.under_dispute {
            return Err(format!(
                "Transaction is already under dispute: {:?}",
                transaction
            ));
        }
        Ok(transaction)
    }

    fn check_transaction_is_new(&self, tx: Tx) -> Result<(), String> {
        match self.transactions.get(&tx) {
            None => Ok(()),
            Some(_) => Err(format!("Transaction already exists: {:?}", tx)),
        }
    }

    /// Assumes that the transaction is actually for this account and the
    /// account is not locked.
    fn handle_valid_transaction(&mut self, transaction: Transaction) -> Result<(), String> {
        let tx = transaction.tx;
        match transaction.detail {
            Action::Deposit(amount) => {
                self.check_transaction_is_new(tx)?;
                // INVARIANT: The new transaction is not under dispute and
                // `held` is not modified.
                self.available += amount;
                self.transactions.insert(
                    tx,
                    DepositDetail {
                        amount,
                        under_dispute: false,
                    },
                );
                Ok(())
            }
            Action::Withdrawal(amount) => {
                self.check_transaction_is_new(tx)?;
                let new_available = self.available.checked_sub(amount).ok_or_else(|| {
                    format!("Insufficient funds for withdrawal {:?}", transaction)
                })?;
                // INVARIANT: Transactions are not changed and `held` is not
                // modified.
                self.available = new_available;
                Ok(())
            }
            Action::Dispute => {
                let available = self.available;
                let disputed_transaction = self.lookup_transaction(tx, false)?;
                let amount = disputed_transaction.amount;
                let new_available = available.checked_sub(amount).ok_or_else(|| {
                    format!(
                        "Insufficient funds to dispute transaction: {:?}",
                        transaction
                    )
                })?;
                // INVARIANT: The transaction is switched from not under dispute
                // to under dispute and `held` is incremented by the ammount of
                // the transaction.
                disputed_transaction.under_dispute = true;
                self.available = new_available;
                self.held += amount;
                Ok(())
            }
            Action::Resolve => {
                let resolved_transaction = self.lookup_transaction(tx, true)?;
                // INVARIANT: The transaction is switched from under dispute to
                // not under dispute and `held` is decremented by the ammount of
                // the transaction.
                resolved_transaction.under_dispute = false;
                let amount = resolved_transaction.amount;
                self.held -= amount;
                self.available += amount;
                Ok(())
            }
            Action::ChargeBack => {
                let charge_back_transaction = self.lookup_transaction(tx, true)?;
                // INVARIANT: The account is now locked, so we don't need to
                // keep `held` in line with the disputed transactions.
                self.held -= charge_back_transaction.amount;
                self.locked = true;
                Ok(())
            }
        }
    }

    /// Apply the effects of the given transaction.
    pub(crate) fn handle_transaction(&mut self, transaction: Transaction) -> Result<(), String> {
        if self.client != transaction.client {
            return Err(format!(
                "Transaction cannot be applied to client {:?}: {:?}",
                self.client, transaction
            ));
        }
        if self.locked {
            return Err(format!(
                "Cannot apply transaction because client account {:?} is locked: {:?}",
                self.client, transaction
            ));
        }
        self.handle_valid_transaction(transaction)
    }
}

/// State of all known accounts.
pub(crate) struct State {
    accounts: HashMap<Client, Account>,
}

impl State {
    /// Create an empty `State`.
    pub(crate) fn new() -> Self {
        State {
            accounts: HashMap::new(),
        }
    }

    /// Update `State` based on a `Transaction`.
    pub(crate) fn handle_transaction(&mut self, transaction: Transaction) -> Result<(), String> {
        let client = transaction.client;
        let account = self
            .accounts
            .entry(client)
            .or_insert_with(|| Account::new(client));
        account.handle_transaction(transaction)
    }

    /// Display the state of all accounts as a CSV.
    pub(crate) fn write_csv<W: Write>(&self, writer: &mut Writer<W>) -> csv::Result<()> {
        fn convert_from_thousandths(amount: u64) -> String {
            format!("{}", (amount as f64) / 10_000.0)
        }
        writer.write_record(&["client", "available", "held", "total", "locked"])?;
        let mut accounts: Vec<_> = self.accounts.values().collect();
        // Sort by client so the output doesn't depend on the order of iterating through
        // the map (which isn't stable).
        accounts.sort_by_key(|a| a.client);
        for account in accounts {
            writer.write_record(&[
                account.client.to_string(),
                convert_from_thousandths(account.available),
                convert_from_thousandths(account.held),
                convert_from_thousandths(account.available + account.held),
                account.locked.to_string(),
            ])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::TransactionRow;
    use csv::{ReaderBuilder, Trim, Writer};
    use std::convert::TryFrom;

    fn read_transactions(s: &str) -> Vec<Transaction> {
        let mut rdr = ReaderBuilder::new()
            .trim(Trim::All)
            .has_headers(false)
            .from_reader(s.as_bytes());
        rdr.deserialize()
            .map(|row| {
                let row: TransactionRow = row.unwrap();
                Transaction::try_from(row).unwrap()
            })
            .collect()
    }

    fn apply_transactions(account: &mut Account, transaction_data: &str) {
        for transaction in read_transactions(transaction_data) {
            account.handle_transaction(transaction).unwrap()
        }
    }

    fn apply_transactions_to_empty_state(transaction_data: &str) -> Result<String, String> {
        let mut state = State::new();
        for transaction in read_transactions(transaction_data) {
            match state.handle_transaction(transaction) {
                Ok(()) => (),
                Err(err) => println!("Error: {}", err),
            }
        }
        let mut vec = Vec::new();
        {
            let mut writer = Writer::from_writer(&mut vec);
            state.write_csv(&mut writer).map_err(|e| e.to_string())?;
        }
        String::from_utf8(vec).map_err(|e| e.to_string())
    }

    #[test]
    fn simple_deposit() {
        let mut account = Account::new(Client::new(1));
        let data = "deposit,1,3,5";
        apply_transactions(&mut account, data);
        assert_eq!(account.held, 0);
        assert_eq!(account.available, 50_000);
        assert!(!account.locked);
        assert_eq!(account.transactions.len(), 1);
        assert_eq!(
            account.transactions.get(&Tx::new(3)).unwrap(),
            &DepositDetail {
                amount: 50_000,
                under_dispute: false
            }
        );
    }

    #[test]
    fn simple_withrawal() {
        let mut account = Account::new(Client::new(1));
        let data = r#"deposit,1,3,5
        withdrawal,1,35,2"#;
        apply_transactions(&mut account, data);
        assert_eq!(account.available, 30_000);
    }

    #[test]
    fn simple_dispute() {
        let mut account = Account::new(Client::new(1));
        let data = r#"deposit,1,3,5
        dispute,1,3,"#;
        apply_transactions(&mut account, data);
        assert_eq!(account.held, 50_000);
        assert_eq!(account.available, 0);
        assert!(!account.locked);
        assert_eq!(
            account.transactions.get(&Tx::new(3)).unwrap(),
            &DepositDetail {
                amount: 50_000,
                under_dispute: true
            }
        );
    }

    #[test]
    fn simple_resolve() {
        let mut account = Account::new(Client::new(1));
        let data = r#"deposit,1,3,5
        dispute,1,3,
        resolve,1,3,"#;
        apply_transactions(&mut account, data);
        assert_eq!(account.held, 0);
        assert_eq!(account.available, 50_000);
        assert!(!account.locked);
        assert_eq!(
            account.transactions.get(&Tx::new(3)).unwrap(),
            &DepositDetail {
                amount: 50_000,
                under_dispute: false
            }
        );
    }

    #[test]
    fn simple_chargeback() {
        let mut account = Account::new(Client::new(1));
        let data = r#"deposit,1,3,5
        dispute,1,3,
        chargeback,1,3,"#;
        apply_transactions(&mut account, data);
        assert_eq!(account.held, 0);
        assert_eq!(account.available, 0);
        assert!(account.locked);
        assert_eq!(
            account.transactions.get(&Tx::new(3)).unwrap(),
            &DepositDetail {
                amount: 50_000,
                under_dispute: true
            }
        );
    }

    #[test]
    fn problem_example_with_integers() {
        let data = r#"deposit, 1, 1, 1
            deposit, 2, 2, 2
            deposit, 1, 3, 2
            withdrawal, 1, 4, 1
            withdrawal, 2, 5, 3"#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,2,0,2,false
2,2,0,2,false
"#
            .to_string())
        );
    }

    #[test]
    fn problem_example() {
        let data = r#"deposit, 1, 1, 1.0
            deposit, 2, 2, 2.0
            deposit, 1, 3, 2.0
            withdrawal, 1, 4, 1.5
            withdrawal, 2, 5, 3.0"#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,1.5,0,1.5,false
2,2,0,2,false
"#
            .to_string())
        );
    }

    #[test]
    fn cant_withdraw_without_funds() {
        let data = r#"deposit, 1, 1, 1.0
            withdrawal, 1, 5, 3.0"#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,1,0,1,false
"#
            .to_string())
        );
    }

    #[test]
    fn cant_chargeback_without_dispute() {
        let data = r#"deposit, 1, 1, 1.0
            chargeback, 1, 1, "#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,1,0,1,false
"#
            .to_string())
        );
    }

    #[test]
    fn cant_resolve_without_dispute() {
        let data = r#"deposit, 1, 1, 1.0
            resolve, 1, 1, "#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,1,0,1,false
"#
            .to_string())
        );
    }

    #[test]
    fn cant_dispute_missing_deposit() {
        let data = r#"deposit, 1, 122, 1.0
            dispute, 1, 123, "#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,1,0,1,false
"#
            .to_string())
        );
    }

    #[test]
    fn cant_dispute_without_funds() {
        let data = r#"deposit, 1, 122, 5.0
            withdrawal, 1, 123, 0.55
            dispute, 1, 122, "#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,4.45,0,4.45,false
"#
            .to_string())
        );
    }

    #[test]
    fn successfull_dispute_and_resolve() {
        let data = r#"deposit, 1, 122, 5.0
            dispute, 1, 122,
            resolve, 1, 122,
            withdrawal, 1, 123, .1234"#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,4.8766,0,4.8766,false
"#
            .to_string())
        );
    }

    #[test]
    fn successfull_chargeback() {
        let data = r#"deposit, 1, 122, 5.0
            deposit, 1, 123, 10.0
            dispute, 1, 122,
            chargeback, 1, 122,
            withdrawal, 1, 123, .1234"#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,10,0,10,true
"#
            .to_string())
        );
    }

    #[test]
    fn successfull_dispute() {
        let data = r#"deposit, 1, 122, 5.0
            deposit, 1, 123, 11.0
            dispute, 1, 123,"#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,5,11,16,false
"#
            .to_string())
        );
    }

    #[test]
    fn cant_transact_after_chargeback() {
        let data = r#"deposit, 1, 122, 5.0
            deposit, 1, 123, 11.0
            dispute, 1, 123,
            chargeback, 1, 123,
            deposit, 1, 124, 35.0
            withdrawal, 1, 125, .1111"#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,5,0,5,true
"#
            .to_string())
        );
    }

    #[test]
    fn duplicate_tx_ignored() {
        let data = r#"deposit, 1, 122, 5.0
            deposit, 1, 122, 11.0
            withdrawal, 1, 122, 1.0"#;
        assert_eq!(
            apply_transactions_to_empty_state(data),
            Ok(r#"client,available,held,total,locked
1,5,0,5,false
"#
            .to_string())
        );
    }
}
