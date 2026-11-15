# Changelog

## [0.2.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.1.0...v0.2.0) (2026-11-14)


### Features

* **assets:** crate lodwright-assets with a BSA v104 reader and loose-over-archive resolver ([#45](https://github.com/JorisJonkers-dev/lodwright-rs/issues/45)) ([92a62c0](https://github.com/JorisJonkers-dev/lodwright-rs/commit/92a62c0b8487a2b542d066dd6ccdcdc2baeafb34))
* **cli:** name each JSON contract and add a contracts --json handshake ([#50](https://github.com/JorisJonkers-dev/lodwright-rs/issues/50)) ([f42a2ac](https://github.com/JorisJonkers-dev/lodwright-rs/commit/f42a2aca7b4af179a5995ce46b65248194af9c31))
* **cli:** probe command for numbers-only inspection of NIF, DDS, settings and dtl files ([#59](https://github.com/JorisJonkers-dev/lodwright-rs/issues/59)) ([b94e301](https://github.com/JorisJonkers-dev/lodwright-rs/commit/b94e30157f9246bd4f823ca9fd910c3089a9d0e8))
* **contract:** plan v1 and result v1 contract, schema command, generate stub, release assets ([#48](https://github.com/JorisJonkers-dev/lodwright-rs/issues/48)) ([a645fcf](https://github.com/JorisJonkers-dev/lodwright-rs/commit/a645fcfa1a194cdfa8d9d35decd1c089f348216c))
* **core:** LAND layers and texture resolution: VNML, VCLR, BTXT, ATXT, VTXT, LTEX, TXST ([#47](https://github.com/JorisJonkers-dev/lodwright-rs/issues/47)) ([62f495a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/62f495af7c329956e8f1251df42e3f53605fcf1e))
* **core:** worldspace data for TTW: parent worldspaces, water heights, many worlds ([#49](https://github.com/JorisJonkers-dev/lodwright-rs/issues/49)) ([5c0c95d](https://github.com/JorisJonkers-dev/lodwright-rs/commit/5c0c95dbdee012c198ce194b6b1653b354b81645))
* **dds:** crate lodwright-dds with DDS writing, resize, mips and BC1, BC3, BC5 ([#57](https://github.com/JorisJonkers-dev/lodwright-rs/issues/57)) ([cc509bf](https://github.com/JorisJonkers-dev/lodwright-rs/commit/cc509bf59be18477efdd159d915000b5b7d6ce49))


### Bug Fixes

* **release:** do not parse an empty pull request output in the lock refresh step ([#58](https://github.com/JorisJonkers-dev/lodwright-rs/issues/58)) ([35742be](https://github.com/JorisJonkers-dev/lodwright-rs/commit/35742be86a8c8e85ba1984ce5ad56cf121f6ac56))

## 0.1.0 (2026-11-14)


### ⚠ BREAKING CHANGES

* rename the project to lodwright-rs ([#11](https://github.com/JorisJonkers-dev/lodwright-rs/issues/11))

### Features

* assemble worldspaces across a load order and rasterise them on blocks ([#14](https://github.com/JorisJonkers-dev/lodwright-rs/issues/14)) ([80d3a51](https://github.com/JorisJonkers-dev/lodwright-rs/commit/80d3a5133fa6fc9e1abe7032c50506a4ce568936))
* Cargo workspace skeleton (core, cli, testkit) ([#4](https://github.com/JorisJonkers-dev/lodwright-rs/issues/4)) ([b6ffb73](https://github.com/JorisJonkers-dev/lodwright-rs/commit/b6ffb73b675953b61ef44c885ffd3c6265e70711))
* clean-room guardrails (process, provenance, SPDX and data guards) ([#7](https://github.com/JorisJonkers-dev/lodwright-rs/issues/7)) ([051aff8](https://github.com/JorisJonkers-dev/lodwright-rs/commit/051aff8e740251c2e33975c21dbc86186c3e7e8d))
* **cli:** emit an integer schema_version in the plan JSON ([#20](https://github.com/JorisJonkers-dev/lodwright-rs/issues/20)) ([0c914d1](https://github.com/JorisJonkers-dev/lodwright-rs/commit/0c914d1157a1dd303e1b25d947307f5f1ab969c0))
* decode LAND VHGT heightfields ([#12](https://github.com/JorisJonkers-dev/lodwright-rs/issues/12)) ([276f0ca](https://github.com/JorisJonkers-dev/lodwright-rs/commit/276f0ca1a18bb457c4ae02ec2d225bd148723777))
* plan and heightmap commands ([#16](https://github.com/JorisJonkers-dev/lodwright-rs/issues/16)) ([ede79aa](https://github.com/JorisJonkers-dev/lodwright-rs/commit/ede79aa93898ef3ca96fbfc3df117de151bbecbf))
* plan terrain LOD blocks, meshes and output layout ([#15](https://github.com/JorisJonkers-dev/lodwright-rs/issues/15)) ([1b4df70](https://github.com/JorisJonkers-dev/lodwright-rs/commit/1b4df70a64d7ba869512def140d03c2db361a026))
* read plugins down to worldspace LAND heights ([#13](https://github.com/JorisJonkers-dev/lodwright-rs/issues/13)) ([6d4a993](https://github.com/JorisJonkers-dev/lodwright-rs/commit/6d4a99322e36cec40d22b05ea0de184ceac57048))


### Bug Fixes

* **ci:** pin the first release to 0.1.0 ([#9](https://github.com/JorisJonkers-dev/lodwright-rs/issues/9)) ([cc5b949](https://github.com/JorisJonkers-dev/lodwright-rs/commit/cc5b9493a1f4a3f984d93890080d38b7af63805c))
* **release:** refresh the lock entries on the release branch ([#19](https://github.com/JorisJonkers-dev/lodwright-rs/issues/19)) ([5930d15](https://github.com/JorisJonkers-dev/lodwright-rs/commit/5930d150cc669fd7321cfb02eb8ffc3b0b78f27f))


### Code Refactoring

* rename the project to lodwright-rs ([#11](https://github.com/JorisJonkers-dev/lodwright-rs/issues/11)) ([b41a6bc](https://github.com/JorisJonkers-dev/lodwright-rs/commit/b41a6bcab2323d1a1f76799ae95e68af02e53952))
