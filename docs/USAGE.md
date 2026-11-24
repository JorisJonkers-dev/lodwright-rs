# Using lodwright

The [README](../README.md) is the short version. This page has how raul uses lodwright, its own commands, and how
to download and check a release.

## Used from raul

raul's LOD page runs lodwright for the active profile: it reads the profile's active plugins and its staged data
folder, generates terrain, objects and trees, and keeps the output as one Mod, `Generated LOD (<profile>)`, in the
mod list. A new run replaces it and keeps its place. The LOD settings page sets the options of the request raul
sends.

<p align="center">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/crates/raul-gui/tests/snapshots/lod/lod-generated.png" alt="raul's LOD page after a run: what was read and written, and every block generated" width="49%">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/crates/raul-gui/tests/snapshots/lod-settings/lod-settings-defaults.png" alt="raul's LOD settings page: worlds, terrain, land textures, water, objects and trees" width="49%">
</p>

The same from raul's command line: `raul lod plan`, `raul lod generate` and `raul lod settings`.

## On its own

```sh
lodwright plan --data <Data folder> --load-order plugins.txt              # what a run would build
lodwright generate --data <Data folder> --load-order plugins.txt --out <folder>
lodwright verify <folder> [--reference <another folder>]                  # check the output's formats
lodwright landmarks --request FILE --landmarks FILE                       # are named landmarks drawn?
lodwright --help                                                          # every command
```

`generate` reads archives and loose files, writes terrain blocks with water, terrain textures, the world settings
file, object blocks with their atlases, tree LOD and a manifest. A second run keeps the terrain blocks whose inputs did
not change. A request document (`lodwright schema request`) is how a program drives a run; the contracts are in
[docs/contracts.md](contracts.md). Programs can also link the `lodwright-api` crate, as raul does.

Each [release](https://github.com/JorisJonkers-dev/lodwright-rs/releases/latest) attaches the binary for Linux
x86-64 and for macOS (one universal binary), with `SHA256SUMS` and the contract schemas, and publishes an OCI package
for Linux: `ghcr.io/jorisjonkers-dev/lodwright-rs:<version>-x86_64-unknown-linux-gnu`. Check it with
`gh attestation verify oci://ghcr.io/jorisjonkers-dev/lodwright-rs:<version>-x86_64-unknown-linux-gnu --repo JorisJonkers-dev/lodwright-rs`
(GitHub CLI 2.49 or newer and a login: run `gh auth login` or set `GH_TOKEN`, even though the package is public).

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
