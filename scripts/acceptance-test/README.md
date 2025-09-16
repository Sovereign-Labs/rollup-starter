## Acceptance Test

This crate runs a test which syncs the rollup against a known set of block and asserts that all
of the *ledger* state responses are as expected. This guarantees the correct state roots are being
calculated - which transitively guarantees that the state is correct. After resyncing, we run a 
soak test for a fixed length of time and ensure that (1) there are no errors and (2) the throughput
is within the expected range.

To run the test simply `cargo run --bin acceptance-test`. All data should have been prepopulated.

The test is meant to be idempotent. It deletes any possible leftover files at the beginning of each run.
However, in case of errors it can sometimes be the case that docker containers haven't been shut down 
from the previous run. To fix, simply `docker rm -f postgres-acceptance-test`.


### Resetting the Test

If you need to generate a new test, simply run `rm -r acceptance-test-data && cargo run --bin setup`. This will generate all of the 
needed files, including a fresh mockDA. Note that setup may take an hour or more to run, since we have to generate a full history
for the rollup.
