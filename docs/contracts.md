# Contracts

lodwright emits JSON documents that other tools read. Each document is a contract (ADR-0006 in the nv-linux umbrella
repository) and says so:

* `schema_version`: an integer. It rises when a key is removed, renamed or retyped, or when a value is added to an enum
  that is not open. A new optional key does not raise it, so a reader ignores keys it does not know.
* `contract`: the name of the contract, `<tool>-<noun>`. A reader refuses a document whose name is not the one it asked
  for. Today: `lodwright-plan` (`lodwright plan --format json`).

## Handshake

`lodwright contracts --json` prints every contract this build emits and the schema versions it can emit, and exits 0:

```json
{"contracts":[{"name":"lodwright-plan","versions":[1]}]}
```

A consumer picks the highest version both sides list that is not above the version it was tested with, and refuses a
tool whose list lacks that version. This build emits only the newest version of each contract.

## Open enums

An enum in a published schema whose values may grow carries `"x-open-enum": true`, and a reader treats an unknown value
as "other". An enum without the mark is closed: adding a value to it is a version change.
