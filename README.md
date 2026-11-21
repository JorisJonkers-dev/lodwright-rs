# lodwright-rs

A Linux-native, clean-room generator of terrain and object LOD (level of detail) data for Fallout: New Vegas and
Tale of Two Wastelands, written in Rust. The existing generators are Windows-only; this one is built from public
file-format documentation, our own measurements and synthetic tests.

**Systems.** Built and tested on Linux and macOS (Apple silicon) on every pull request, and linked into the Toolbox's
`raul` on Linux, Windows and macOS, which builds it from the pinned source. Each release attaches the binary for Linux
x86-64 and for macOS (one universal binary for Apple silicon and Intel); the OCI package is Linux only.

**Licence: GPL-3.0-only**, with two files translated from xEdit kept under MPL-2.0 ([docs/LICENSING.md](docs/LICENSING.md)).
The clean-room rule is in [docs/clean-room.md](docs/clean-room.md).

## Clean-room rule

Format facts come from public format documents, the MPL-2.0 xEdit source, public readmes, Microsoft's DDS
documentation, the game's own data formats on the owner's machine, and black-box output of an existing generator
run on the owner's own data. Decompiled or leaked code and unlicensed source are never read. Every fact the code
relies on is recorded in `provenance/sources.toml` and `provenance/LOG.md` (CI checks the record). Game data and
reference output never go in this repository; test fixtures are synthetic.

## Status

Working end to end for terrain and object LOD of a world: `lodwright generate` reads a load order and the data folder
(archives and loose files), and writes terrain block meshes with water planes, terrain diffuse and normal maps (land
texture layers, vertex colours, heights), the world settings file, object LOD blocks with their atlas pair, tree LOD
(billboard atlas, type list and block files, from billboard textures that mods ship), and a manifest. Output matches
the game's own tree in file names, counts and structure; against the game's own blocks of the main world, all 295
object blocks have the same vertex and triangle counts (99.98 percent of vertices in place) and the water planes
cover the same ground to 0.91 to 0.99 (see `docs/gap-list.md` for the measurements). A second run keeps the terrain
blocks whose inputs did not change. `lodwright verify` checks a folder against the formats and can compare it with
another tree. A request document (`lodwright schema request`) is how a caller drives a run.

Not done: loading in the game (owner only, docs/owner-runbook.md section 6), the game's own water outline (ours is
our own design), and reuse of unchanged object and tree blocks. The gap list has each item with its state.

## Packages

The `lodwright` program is published on each release as an OCI package on ghcr.io:

`ghcr.io/jorisjonkers-dev/lodwright-rs:<version>-x86_64-unknown-linux-gnu`

where `<version>` is the release tag without the leading `v`. The package is a `tar.gz` archive with the binary and
`LICENSE`. The package is an OCI artifact with one layer; the layer is that archive, and its digest equals the
"Archive SHA-256" in the release notes.

Verify it with:

```sh
gh attestation verify oci://ghcr.io/jorisjonkers-dev/lodwright-rs:<version>-x86_64-unknown-linux-gnu --repo JorisJonkers-dev/lodwright-rs
```

This needs `gh` 2.49 or newer (older versions, such as the 2.46 of Debian 13, have no `attestation` command) and
a GitHub token: run `gh auth login` or set `GH_TOKEN`, even though the package and its attestation are public.

### Pulling and verifying without a login or docker

The package is public, so an anonymous token is enough. With `curl`, `python3` and `sha256sum` (replace `<version>`):

```sh
TAG=<version>-x86_64-unknown-linux-gnu
TOKEN=$(curl -s "https://ghcr.io/token?scope=repository:jorisjonkers-dev/lodwright-rs:pull" | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
# the manifest (keep the digest header: it is the manifest digest) and the layer it names
curl -s -D headers.txt -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.oci.image.manifest.v1+json" \
  -o manifest.json "https://ghcr.io/v2/jorisjonkers-dev/lodwright-rs/manifests/$TAG"
LAYER=$(python3 -c 'import json;print(json.load(open("manifest.json"))["layers"][0]["digest"])')
curl -sL -H "Authorization: Bearer $TOKEN" -o lodwright.tar.gz "https://ghcr.io/v2/jorisjonkers-dev/lodwright-rs/blobs/$LAYER"
sha256sum lodwright.tar.gz    # equals the Archive SHA-256 of the release notes and the layer digest
```

To verify the attestation without a login, fetch its bundle from the public API (the digest is the manifest digest
from `headers.txt`, `docker-content-digest`) and give it to `gh`, which then needs no token:

```sh
DIGEST=$(grep -i '^docker-content-digest:' headers.txt | tr -d '\r' | cut -d' ' -f2)
curl -s "https://api.github.com/repos/JorisJonkers-dev/lodwright-rs/attestations/$DIGEST" \
  | python3 -c 'import sys,json;[print(json.dumps(a["bundle"])) for a in json.load(sys.stdin)["attestations"]]' > bundle.jsonl
gh attestation verify oci://ghcr.io/jorisjonkers-dev/lodwright-rs:$TAG --bundle bundle.jsonl --repo JorisJonkers-dev/lodwright-rs
```

`gh` exits 0 when the bundle verifies. The unauthenticated GitHub API allows 60 requests an hour per address.

To republish an existing tag, for example after the package was renamed, run the release workflow by hand on `main` with the tag and `publish` set: `gh workflow run release.yml -R JorisJonkers-dev/lodwright-rs --ref main -f tag=vX.Y.Z -f publish=true`. The run builds the assets of that tag, uploads them to its GitHub release and pushes the package again. Without `publish=true` it is a dry run that pushes nothing.

## Place in the stack

lodwright-rs is a Component of [Raul's Trusty Toolbox](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox).
The Toolbox (`raul`) pins, fetches and verifies the lodwright release, writes the request document and runs
`lodwright` as a process (ADR-0007 there). lodwright owns its Contracts (plan, request, result, manifest, verify and
events, see [docs/contracts.md](docs/contracts.md)); the Toolbox keeps the registry of which versions each Known-good
set speaks (ADR-0009 there). The CLI works alone and never fetches or starts another tool.

## Scope

Heightfield reader, block-aligned raster and terrain planning (done), then terrain meshes on disk, terrain
textures, object and tree LOD. Out of scope: running the game, and shipping or reading anyone's game data from this
repository.

## Where to read next

- [docs/owner-runbook.md](docs/owner-runbook.md): what the owner runs on real data, and what to report back.
- [docs/gap-list.md](docs/gap-list.md): what full generation needs and what exists.
- [docs/RELEASE-TEST.md](docs/RELEASE-TEST.md): the checklist a release passes on real data before it is cut.
- [docs/LICENSING.md](docs/LICENSING.md): which crate carries which licence.
- [docs/design.md](docs/design.md): pipeline, crates, first-slice interfaces, open questions.
- [docs/clean-room.md](docs/clean-room.md): who may read what; provenance rules; what CI enforces.
- [docs/TESTING.md](docs/TESTING.md): synthetic fixtures only, properties, oracle plan, `verify` design.
- [docs/PRACTICES.md](docs/PRACTICES.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md).
