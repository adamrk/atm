use csv::{ReaderBuilder, Trim, Writer};
use state::State;
use std::{convert::TryFrom, env, io, path::PathBuf};
use transaction::{Transaction, TransactionRow};

mod state;
mod transaction;

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() != 2 {
        panic!("Usage: cargo run -- <atm-transactions-file>");
    }
    let arg: PathBuf = args[1].parse().unwrap();
    let mut csv_reader = ReaderBuilder::new()
        .trim(Trim::All) // Input file might have extra spaces.
        .has_headers(true) // Input file must have headers.
        .from_path(arg)
        .unwrap();

    let mut state = State::new();
    for row in csv_reader.deserialize::<TransactionRow>() {
        let transaction = Transaction::try_from(row.unwrap()).unwrap();
        let _possible_client_error = state.handle_transaction(transaction);
    }

    let mut writer = Writer::from_writer(io::stdout());
    state.write_csv(&mut writer).unwrap();
}
