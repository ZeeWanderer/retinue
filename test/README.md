# Shared Tests

- `fixtures/`: read-only inputs shared across applications and languages.
- `support/`: Erlang helpers compiled only in Rebar's test profile.
- `integration/`: cross-application scenarios, including the isolated staged PoE2 test.

Component tests stay with their owning app or crate. See
[testing](../docs/developer/testing.md) for commands, coverage and benchmark policy.
