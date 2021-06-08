# Assumptions
1. We're assuming that only deposits can be disputed. This wasn't explicitly
stated in the problem, but it does say that the resolution of a dispute is to
withdraw funds which doesn't make sense if the disputed transaction was a
withdrawal.

# Overall Design
There are three main types:

1. `Transaction`: Represents a transaction and uses ADTs so only deposits and
withdrawals have associated amounts.  

1. `Account`: Holds the state of an account for a single client.  Most of the
core logic for solving the exercise is in the internal fn `handle_valid_update`
which updates the account states based on a single `Transaction`. Caller's can
safely manipulate this type through it's public methods (e.g. an `Account` knows
which client it is tied to and will reject any `Transaction`s for other
clients).

1. `State`: The collection of `Account`s for each client. The main program logic
is to just fold over a list of `Transaction`s and update `State` by distributing
each `Transaction` to the `Account` for the relevent client.


# Notes on Correctness
Some properties are guaranteed by the types:

- amounts are stored as `u64` (representing the numer of 1/10000's) so they
are guaranteed to always be non-negative. 

- `Account`s report the total funds as the sum of funds held and funds available.
This means there is no need to keep a third variable in sync with the other two.

- The `Action` type used in transactions guarantees that deposits and
withdrawals always have amounts, but the other transaction types don't.

Properties of how the state transitions are checked by unit tests. We run all
unit tests on the public interface of `State` so as not to have our tests depend
on implementation details. Each test applies a simple sequence of transactions
to an initial empty state and makes a single assertion about the final state.

We check that all the "happy path" sequences work as expected:

- deposit
- deposit, withdrawal
- deposit, dispute
- deposit, dispute, resolve
- deposit, dispute, chargeback

In addition, we check some sequences that result in errors:

- withdrawing more funds than are in the account
- disputing a transaction that doesn't exist
- resolving a dispute which hasn't been disputed
- ... etc.

Some cases are probably missing, but the general test setup should make it easy
to add them as they are encountered. There are also test cases for the example
in the problem spec.

We also have a simple end-to-end that runs the actual compiled binary to check
that it accepts input and produces output in the expected format.


# Safety
There is no unsafe code used. We only use `unwrap` in the `main` function where
we want to crash if we can't properly read the input or generate the output. We
also do some casting when reading the `amount` fields to `u64`s. In production
code it would be better to make that a custom type and add some tests for the
conversions, but I didn't have time.

The `Account` type does have an invariant that
needs to be maintained: the field `held` must be equal to the sum of the amounts
in all disputed transactions. This is done for efficiency so that we don't need
to iterate through all transactions to see how much is held. The invariant is 
documented and maintained by the type public interface, so callers do not need
to be aware of it.


# Efficiency
The main problem with this solution is that it stores the details of each
deposit forever. This would cause a memory leak if the process were to be part
of a long-running server. In that case, we would modify the code to only store
the most recent deposits and the full history would be stored in a separate
database. Tuning how transactions are stored in the DB and how the cache is
maintained would depend on the particular situation (e.g. if transactions cannot
be disputed after two days, we could definitely remove anything more than two
days old from the cache).

This solution basically folds over a sequence of transactions, so could easily
be modified to handle a stream coming over a network instead of reading from a
CSV (in fact, the way we are currently using the `csv` library, each transaction
is only read as it is needed). In addition, the state can be partition by
clients so it would be possible to scale this out by having multiple workers
with each responsible for a different subset of clients.
