use serde::Deserialize;
use std::{convert::TryFrom, fmt::Display};

/// Unique identifier for a client.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize)]
pub(crate) struct Client(u16);

impl Display for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Client {
    #[cfg(test)]
    pub fn new(id: u16) -> Self {
        Client(id)
    }
}

/// Unique identifier for a transaction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
pub(crate) struct Tx(u32);

impl Tx {
    #[cfg(test)]
    pub fn new(id: u32) -> Self {
        Tx(id)
    }
}

/// Description of the action a transaction would like to perform.
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    /// Amounts for Deposits are `u64`s representing the number of 1/10_000's.
    Deposit(u64),
    /// Amounts for Withdrawals are `u64`s representing the number of 1/10_000's.
    Withdrawal(u64),
    Dispute,
    Resolve,
    ChargeBack,
}

impl Action {
    fn from_type_and_amount(type_: &str, amount: Option<f64>) -> Result<Action, String> {
        fn convert_amount(amount: f64) -> u64 {
            (amount * 10_000.0).round() as u64
        }
        match (type_, amount) {
            ("deposit", Some(amount)) => Ok(Action::Deposit(convert_amount(amount))),
            ("withdrawal", Some(amount)) => Ok(Action::Withdrawal(convert_amount(amount))),
            ("dispute", None) => Ok(Action::Dispute),
            ("resolve", None) => Ok(Action::Resolve),
            ("chargeback", None) => Ok(Action::ChargeBack),
            other => Err(format!("Invalid transaction type: {:?}", other)),
        }
    }
}

/// A single client transaction.
#[derive(Debug, PartialEq)]
pub(crate) struct Transaction {
    pub client: Client,
    pub tx: Tx,
    pub detail: Action,
}

/// A row parsed from the CSV. This needs to be converted to
/// a `Transaction` for use and it may be invalid (e.g. if the type
/// is `"withdrawal"`, but there is no amount).
#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct TransactionRow {
    #[serde(rename = "type")]
    type_: String,
    client: Client,
    tx: Tx,
    amount: Option<f64>,
}

impl TryFrom<TransactionRow> for Transaction {
    type Error = String;

    fn try_from(value: TransactionRow) -> Result<Self, Self::Error> {
        let detail = Action::from_type_and_amount(&value.type_, value.amount)?;
        Ok(Transaction {
            client: value.client,
            tx: value.tx,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use csv::ReaderBuilder;
    use std::convert::TryInto;

    use super::*;

    #[test]
    fn read_with_headers() {
        let data = r#"type, client, tx, amount
            deposit, 0, 1, 2"#;
        let mut rdr = ReaderBuilder::new()
            .trim(csv::Trim::All)
            .from_reader(data.as_bytes());
        let row: Option<csv::Result<TransactionRow>> = rdr.deserialize().next();
        match row {
            None => panic!("No row"),
            Some(Err(csv_err)) => panic!("{}", csv_err),
            Some(Ok(parsed)) => assert_eq!(
                parsed,
                TransactionRow {
                    type_: "deposit".to_string(),
                    client: Client::new(0),
                    tx: Tx::new(1),
                    amount: Some(2.0),
                }
            ),
        }
    }

    fn read_line(s: &str) -> Result<Transaction, String> {
        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .from_reader(s.as_bytes());
        let transaction_row: TransactionRow = match rdr.deserialize().next() {
            None => Err("No record".to_string()),
            Some(Err(csv_err)) => Err(csv_err.to_string()),
            Some(Ok(row)) => Ok(row),
        }?;
        transaction_row.try_into()
    }

    #[test]
    fn read_deposit() {
        assert_eq!(
            read_line("deposit,4,5,6"),
            Ok(Transaction {
                client: Client::new(4),
                tx: Tx::new(5),
                detail: Action::Deposit(6_0000)
            })
        )
    }

    #[test]
    fn read_withdrawal() {
        assert_eq!(
            read_line("withdrawal,0,0,0"),
            Ok(Transaction {
                client: Client::new(0),
                tx: Tx::new(0),
                detail: Action::Withdrawal(0)
            })
        )
    }

    #[test]
    fn read_dispute() {
        assert_eq!(
            read_line("dispute,0,0,"),
            Ok(Transaction {
                client: Client::new(0),
                tx: Tx::new(0),
                detail: Action::Dispute
            })
        )
    }

    /// For transactions that don't have an amount, we expect that the last
    /// field is empty, but it must still exist.
    #[test]
    fn no_amount_field_is_error() {
        assert!(read_line("dispute,0,0").is_err())
    }

    #[test]
    fn read_resolve() {
        assert_eq!(
            read_line("resolve,0,0,"),
            Ok(Transaction {
                client: Client::new(0),
                tx: Tx::new(0),
                detail: Action::Resolve
            })
        )
    }

    #[test]
    fn read_charge_back() {
        assert_eq!(
            read_line("chargeback,0,0,"),
            Ok(Transaction {
                client: Client::new(0),
                tx: Tx::new(0),
                detail: Action::ChargeBack
            })
        )
    }
}
