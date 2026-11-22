# Request option notes

One file per request option (or group of options that share a mechanism), named after the topic:
`docs/contracts/options/<topic>.md`, starting with a `### <Title>` heading. It says the dotted key, the default, the
tier and the release it was introduced in, what it does, what it does to the request digest and the block digests, and
the source or "own design" with the row id or spec section.

A new option adds a new file here. `docs/contracts.md` (Request and manifest) does not list the options and is not
edited for one, so two pull requests that each add an option do not touch the same file for their notes. The folder
listing is the index.
