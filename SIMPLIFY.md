# Retired complete-engine contract

The former transition-processor/full-loop fatal-error contract is retired.
Its source and rationale remain in Git history at `247c4974`. Current component
faults and reset behavior remain independently verified; there is no complete
FPGA application backend or public ARM processing protocol in this branch.

See [current architecture](FPGA_ARCHITECTURE.md) and the
[retirement ledger](docs/fpga/retirement-ledger.md). Future integration follows the
RT communication direction in PR #997.
