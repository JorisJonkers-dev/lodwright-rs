# Changelog

## [0.21.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.20.0...v0.21.0) (2026-11-27)


### Features

* **objects:** far light points for flickering and pulsing lights ([#441](https://github.com/JorisJonkers-dev/lodwright-rs/issues/441)) ([9c52d67](https://github.com/JorisJonkers-dev/lodwright-rs/commit/9c52d67bc1fc634c91d3368b698a7ea8c32498e8))

## [0.20.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.19.0...v0.20.0) (2026-11-27)


### Features

* **objects:** far blocks glow at night (objects.far_glow) ([#439](https://github.com/JorisJonkers-dev/lodwright-rs/issues/439)) ([a73ddca](https://github.com/JorisJonkers-dev/lodwright-rs/commit/a73ddcafff48d666eed731b26023d143fae6a7d9))

## [0.19.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.18.0...v0.19.0) (2026-11-25)


### Features

* **objects:** far light points on every copy of a lamp model when the load order lights only some copies ([#432](https://github.com/JorisJonkers-dev/lodwright-rs/issues/432)) ([27c6861](https://github.com/JorisJonkers-dev/lodwright-rs/commit/27c6861f341572a939431947035409edd3a33000))

## [0.18.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.17.0...v0.18.0) (2026-11-25)


### Features

* **objects:** far light points as one marked unlit shape per object block (objects.far_lights) ([#429](https://github.com/JorisJonkers-dev/lodwright-rs/issues/429)) ([6e8036a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/6e8036a5d11ebc45397e05cd2b0bf8a3ebb4ee68))
* **objects:** optional rule that takes unflagged references with a far model ([#428](https://github.com/JorisJonkers-dev/lodwright-rs/issues/428)) ([4b85a46](https://github.com/JorisJonkers-dev/lodwright-rs/commit/4b85a46eaeab3c5a4c4311b6b938a41c3b3e2989))


### Bug Fixes

* **generate:** an output written by another build of the same version is rebuilt ([#423](https://github.com/JorisJonkers-dev/lodwright-rs/issues/423)) ([4f63e45](https://github.com/JorisJonkers-dev/lodwright-rs/commit/4f63e45c49534f7eb352d3db537abc4bab8d8aca))
* **objects:** follow an enable parent chain to its end ([#413](https://github.com/JorisJonkers-dev/lodwright-rs/issues/413)) ([eecf4b4](https://github.com/JorisJonkers-dev/lodwright-rs/commit/eecf4b4d4fa551f715faf171861391da4f7618c5))
* **objects:** include rules follow the start state, and enable parents are found in every world ([#416](https://github.com/JorisJonkers-dev/lodwright-rs/issues/416)) ([0534f8f](https://github.com/JorisJonkers-dev/lodwright-rs/commit/0534f8fccc37c60040220da49d6f0df9ef29b8d3))
* **objects:** keep persistent initially disabled references in the far view ([#417](https://github.com/JorisJonkers-dev/lodwright-rs/issues/417)) ([af7c9c9](https://github.com/JorisJonkers-dev/lodwright-rs/commit/af7c9c9bdf691cf8b257bee01771b2d7bb227c23))
* **objects:** leave out references that are disabled when the game starts ([#412](https://github.com/JorisJonkers-dev/lodwright-rs/issues/412)) ([32dd4a7](https://github.com/JorisJonkers-dev/lodwright-rs/commit/32dd4a70dca0699eab0352c6d1f6c2d7607ba79f))
* **objects:** read High Priority LOD from the static as well as the reference ([#421](https://github.com/JorisJonkers-dev/lodwright-rs/issues/421)) ([dfb4885](https://github.com/JorisJonkers-dev/lodwright-rs/commit/dfb48857d1eb2b21ed18f9e7ed4bc0b6641bab6b))
* **pack:** progress counts the files read into the archive; a digest failure removes the written file ([#418](https://github.com/JorisJonkers-dev/lodwright-rs/issues/418)) ([8a01307](https://github.com/JorisJonkers-dev/lodwright-rs/commit/8a01307f55a2f470b5be17c1e1d0fe5cd71feaee))


### Performance Improvements

* **pack:** stream the archive to disk with one file in memory at a time ([#409](https://github.com/JorisJonkers-dev/lodwright-rs/issues/409)) ([8a6987a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/8a6987a4133f009bfb2d282a0016b9573bbb7d87))

## [0.17.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.16.3...v0.17.0) (2026-11-24)


### Features

* **gen:** add a simplifier for far shapes of models without a LOD model ([#404](https://github.com/JorisJonkers-dev/lodwright-rs/issues/404)) ([4957bde](https://github.com/JorisJonkers-dev/lodwright-rs/commit/4957bdeb992b1be8e8b9f349f232161a96f3b82d))
* **objects:** a generate option on each include rule draws only its objects from full models ([#408](https://github.com/JorisJonkers-dev/lodwright-rs/issues/408)) ([cddad5c](https://github.com/JorisJonkers-dev/lodwright-rs/commit/cddad5c038e383b712b323218f5d6ba4d9f225ff))
* **objects:** draw objects without a LOD model from simplified full models ([#405](https://github.com/JorisJonkers-dev/lodwright-rs/issues/405)) ([9468316](https://github.com/JorisJonkers-dev/lodwright-rs/commit/9468316a30fed1e2c87a5d0b59fe2f8c2bf61f4c))
* **objects:** include rules take chosen objects into the far view ([#407](https://github.com/JorisJonkers-dev/lodwright-rs/issues/407)) ([7a01f62](https://github.com/JorisJonkers-dev/lodwright-rs/commit/7a01f6270d9d68695f07f6430bd5829f2fa1da5d)), closes [#385](https://github.com/JorisJonkers-dev/lodwright-rs/issues/385)
* **verify:** report far-object error per level and atlas use ([#402](https://github.com/JorisJonkers-dev/lodwright-rs/issues/402)) ([ff03873](https://github.com/JorisJonkers-dev/lodwright-rs/commit/ff0387390e495492383cab4bcda99b4d73273f37))

## [0.16.3](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.16.2...v0.16.3) (2026-11-23)


### Bug Fixes

* **pack:** write an override marker beside the packed archive ([#400](https://github.com/JorisJonkers-dev/lodwright-rs/issues/400)) ([f0e334f](https://github.com/JorisJonkers-dev/lodwright-rs/commit/f0e334fd83a463cbbc0e74cd68ed387d3b6d7225))

## [0.16.2](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.16.1...v0.16.2) (2026-11-23)


### Bug Fixes

* **pack:** a file that several worlds list is packed once ([#396](https://github.com/JorisJonkers-dev/lodwright-rs/issues/396)) ([a11a25c](https://github.com/JorisJonkers-dev/lodwright-rs/commit/a11a25ca720a834654121c20655dae6fc3e99cdd))

## [0.16.1](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.16.0...v0.16.1) (2026-11-23)


### Bug Fixes

* **terrain:** the packed output of an earlier run is not the game's archive ([#393](https://github.com/JorisJonkers-dev/lodwright-rs/issues/393)) ([4bcbffe](https://github.com/JorisJonkers-dev/lodwright-rs/commit/4bcbffec30592ee4ccfda5b1b112abfdb348d233))

## [0.16.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.15.1...v0.16.0) (2026-11-23)


### Features

* **api:** the result profiles every step with time, CPU time, peak memory and files ([#377](https://github.com/JorisJonkers-dev/lodwright-rs/issues/377)) ([6f5c180](https://github.com/JorisJonkers-dev/lodwright-rs/commit/6f5c180924d370e88c657485cf9f67342be37c35))
* **cli:** lodwright landmarks reports whether named landmarks are written ([#376](https://github.com/JorisJonkers-dev/lodwright-rs/issues/376)) ([57d3b02](https://github.com/JorisJonkers-dev/lodwright-rs/commit/57d3b0259438c3a4ac8a535d25221013374e41ab)), closes [#375](https://github.com/JorisJonkers-dev/lodwright-rs/issues/375)
* **generate:** one warning per plugin for far models without a LOD model, a file with every warning ([#362](https://github.com/JorisJonkers-dev/lodwright-rs/issues/362)) ([96c34ed](https://github.com/JorisJonkers-dev/lodwright-rs/commit/96c34ed2985ed3f8cd7afa47e691c6312bb7dce7))
* **objects:** take far shapes from shipped blocks for the models a request names ([#387](https://github.com/JorisJonkers-dev/lodwright-rs/issues/387)) ([cc2d488](https://github.com/JorisJonkers-dev/lodwright-rs/commit/cc2d48891ad0fee31eb645d19dad81d79d956a53))
* **objects:** warn per LOD model without a diffuse texture; count the warnings left out ([#306](https://github.com/JorisJonkers-dev/lodwright-rs/issues/306)) ([7a9ff6c](https://github.com/JorisJonkers-dev/lodwright-rs/commit/7a9ff6cb452924fd8895e1ada470e1e08839a9ed))
* **terrain:** far land is lit as the near land is, and a game tile is used only where it already matches ([#351](https://github.com/JorisJonkers-dev/lodwright-rs/issues/351)) ([4edbe60](https://github.com/JorisJonkers-dev/lodwright-rs/commit/4edbe603bf2a8450a96507d44c1c3a72188ddf3d))
* **terrain:** leave the game's own far-land tiles where no mod changed the land ([#334](https://github.com/JorisJonkers-dev/lodwright-rs/issues/334)) ([458d7bf](https://github.com/JorisJonkers-dev/lodwright-rs/commit/458d7bf5204ae0de88348e9a85bad1c9dcf4afe4))


### Bug Fixes

* **cli:** --threads replaces the thread count of a request file ([#380](https://github.com/JorisJonkers-dev/lodwright-rs/issues/380)) ([67c5469](https://github.com/JorisJonkers-dev/lodwright-rs/commit/67c5469424ff5087d3a867795bd4eb29625c6688))
* **dds:** read BC7 far textures with lodwright's own decoder instead of a flat colour ([#360](https://github.com/JorisJonkers-dev/lodwright-rs/issues/360)) ([aebb809](https://github.com/JorisJonkers-dev/lodwright-rs/commit/aebb809b8c986ce1991fc1e8fd44cc77bcbe3d4a))
* **generate:** a land texture that no folder or archive holds is its own warning kind ([#345](https://github.com/JorisJonkers-dev/lodwright-rs/issues/345)) ([#363](https://github.com/JorisJonkers-dev/lodwright-rs/issues/363)) ([cf67f76](https://github.com/JorisJonkers-dev/lodwright-rs/commit/cf67f76245864e49acc79b8e3c0c433f66710e4f))
* **generate:** an output written by another version is rebuilt, not reported up to date ([#300](https://github.com/JorisJonkers-dev/lodwright-rs/issues/300)) ([660953b](https://github.com/JorisJonkers-dev/lodwright-rs/commit/660953b58d60956244f53a6f05ab7b4d28bd9719))
* **nif:** an object block carries its origin on the shape, as the game's blocks do ([#295](https://github.com/JorisJonkers-dev/lodwright-rs/issues/295)) ([0b43529](https://github.com/JorisJonkers-dev/lodwright-rs/commit/0b43529486ca531a1f3e2ce03c3316f97014f14a))
* **objects:** a block over the 16-bit triangle limit is written as several shapes, not a failed run ([#354](https://github.com/JorisJonkers-dev/lodwright-rs/issues/354)) ([bcdfb37](https://github.com/JorisJonkers-dev/lodwright-rs/commit/bcdfb375b1e4b254e11633980587579d52d44272))
* **objects:** a far-model shape with no texture takes the full model's texture instead of a flat grey ([#357](https://github.com/JorisJonkers-dev/lodwright-rs/issues/357)) ([e00acf0](https://github.com/JorisJonkers-dev/lodwright-rs/commit/e00acf02bfdc6ea5912287ed110bc2340f973cba))
* **objects:** a reference deleted by a later plugin drops out of object LOD ([#302](https://github.com/JorisJonkers-dev/lodwright-rs/issues/302)) ([faa9fda](https://github.com/JorisJonkers-dev/lodwright-rs/commit/faa9fdac7d3ac8d0c6ec6c2e29e4247ac9805575))
* **objects:** a texture coordinate outside the texture is held at its edge, as the game's blocks do ([#322](https://github.com/JorisJonkers-dev/lodwright-rs/issues/322)) ([157a10b](https://github.com/JorisJonkers-dev/lodwright-rs/commit/157a10b66e8a1bf21064141dbf0f6862bd170f54))
* **objects:** an object block's segment table has one segment per cell, as the game's blocks do ([#328](https://github.com/JorisJonkers-dev/lodwright-rs/issues/328)) ([8d9f24e](https://github.com/JorisJonkers-dev/lodwright-rs/commit/8d9f24ec61764cc168cf57897efb2ad91cbc84d0))
* **objects:** each shape of a block names its own texture set, so far objects on a second atlas page draw with that page ([#359](https://github.com/JorisJonkers-dev/lodwright-rs/issues/359)) ([47f1adc](https://github.com/JorisJonkers-dev/lodwright-rs/commit/47f1adcd6e19674581a96285d6a3a1fe7aa4e47e))
* **objects:** keep every far-object texture at its native size on several atlases instead of halving them all ([#355](https://github.com/JorisJonkers-dev/lodwright-rs/issues/355)) ([b0a2679](https://github.com/JorisJonkers-dev/lodwright-rs/commit/b0a2679727fe976768f7c381dbcf339714f63a2a))
* **objects:** keep persistent initially disabled references in object LOD, as the game's own blocks do ([#350](https://github.com/JorisJonkers-dev/lodwright-rs/issues/350)) ([cf20f3a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/cf20f3ac003ad6dba453c2496d7db16f874dd083))
* **objects:** leave initially disabled references out of object LOD, as xLODGen does ([#341](https://github.com/JorisJonkers-dev/lodwright-rs/issues/341)) ([61cca87](https://github.com/JorisJonkers-dev/lodwright-rs/commit/61cca87776c4846f3d3eb4c863688ea35e0bf3b4))
* **objects:** read the texture set of a BSLightingShaderProperty so Skyrim-stream LOD models are not drawn flat ([#304](https://github.com/JorisJonkers-dev/lodwright-rs/issues/304)) ([5311e02](https://github.com/JorisJonkers-dev/lodwright-rs/commit/5311e0237ea632c1b334f6626c369afdc928a709))
* **objects:** read uncompressed DDS textures for object LOD and land, and tell a missing texture from an unreadable one ([#336](https://github.com/JorisJonkers-dev/lodwright-rs/issues/336)) ([4a2ad4a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/4a2ad4a4b41f2ef161e8401931e665b68bb0a54b))
* **objects:** write a block that passes the 16-bit vertex limit as several shapes instead of leaving objects out ([#348](https://github.com/JorisJonkers-dev/lodwright-rs/issues/348)) ([b70a5f3](https://github.com/JorisJonkers-dev/lodwright-rs/commit/b70a5f31c0f2c31d1a8e94bb00f2f5ebc8fffdcd))
* **terrain:** a quadrant without a base texture shows the game's default land texture ([#329](https://github.com/JorisJonkers-dev/lodwright-rs/issues/329)) ([b53d067](https://github.com/JorisJonkers-dev/lodwright-rs/commit/b53d0673136e10d320477f06023b7395606527b5))
* **terrain:** a world with land beyond 128 cells gets the game's 128 cell window ([#308](https://github.com/JorisJonkers-dev/lodwright-rs/issues/308)) ([4fe7309](https://github.com/JorisJonkers-dev/lodwright-rs/commit/4fe7309e6aae094050b29494ec79005061f22d53))
* **terrain:** judge whether the game's far-land tiles match the near land once per world ([#353](https://github.com/JorisJonkers-dev/lodwright-rs/issues/353)) ([df9ce93](https://github.com/JorisJonkers-dev/lodwright-rs/commit/df9ce933f64544296bf54d5f3a5664f62e25eb37))
* **terrain:** skirts are on by default so blocks of different levels leave no cracks ([#330](https://github.com/JorisJonkers-dev/lodwright-rs/issues/330)) ([f0c8d1f](https://github.com/JorisJonkers-dev/lodwright-rs/commit/f0c8d1f2373d689233fc75586d9b23a46b23384d))
* **terrain:** the far land of a world is filled as the game's own tiles are: a flat default colour, the Strip's brown, grey outside the cells ([#333](https://github.com/JorisJonkers-dev/lodwright-rs/issues/333)) ([d581d15](https://github.com/JorisJonkers-dev/lodwright-rs/commit/d581d15fb365c4a118edb71e209dcef9ac1cb17d))
* **terrain:** water walls on the block border and a level 4 overlap, on by default ([#331](https://github.com/JorisJonkers-dev/lodwright-rs/issues/331)) ([bbb90b8](https://github.com/JorisJonkers-dev/lodwright-rs/commit/bbb90b8238aa9648a74b91fd606af833d8e33b02))
* **verify:** node translations are checked, so an object block with its origin on the root fails ([#298](https://github.com/JorisJonkers-dev/lodwright-rs/issues/298)) ([01f9059](https://github.com/JorisJonkers-dev/lodwright-rs/commit/01f9059f85193f32a4d839bce2c2c19ebac073d4))


### Performance Improvements

* **api:** assemble the base load order once and decode land textures on several threads ([#382](https://github.com/JorisJonkers-dev/lodwright-rs/issues/382)) ([c868043](https://github.com/JorisJonkers-dev/lodwright-rs/commit/c8680430a99586500cee654a92459db6ebbde7ba))
* **api:** every core works by default, held to three quarters of the machine's memory ([#378](https://github.com/JorisJonkers-dev/lodwright-rs/issues/378)) ([ae40fb4](https://github.com/JorisJonkers-dev/lodwright-rs/commit/ae40fb4af5a4a921ad1458fcbb69ee6b0827a6e8))
* **core:** do not score the triangles of the border vertices ([#384](https://github.com/JorisJonkers-dev/lodwright-rs/issues/384)) ([9d687c0](https://github.com/JorisJonkers-dev/lodwright-rs/commit/9d687c004cd58ab40be71e8d1aa55504463d089a))
* **dds:** encode the atlas pages on several threads with identical bytes ([#379](https://github.com/JorisJonkers-dev/lodwright-rs/issues/379)) ([e3675d8](https://github.com/JorisJonkers-dev/lodwright-rs/commit/e3675d8ad4735655296e20c018eb4984833eb4dc))
* **dds:** resample mip levels on several threads, find the tree bases once ([#383](https://github.com/JorisJonkers-dev/lodwright-rs/issues/383)) ([f85215a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/f85215ad8217e6c45f25c1db4f6350b4f2553976))
* **gen:** decode textures and make atlas textures and normal maps on several threads ([#381](https://github.com/JorisJonkers-dev/lodwright-rs/issues/381)) ([5f11541](https://github.com/JorisJonkers-dev/lodwright-rs/commit/5f1154196cde5aa1612130ab31db3164a5e0d4fd))

## [0.15.1](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.15.0...v0.15.1) (2026-11-21)


### Bug Fixes

* **nif:** the terrain block's multibound names its box block, so blocks with water draw their ground ([#285](https://github.com/JorisJonkers-dev/lodwright-rs/issues/285)) ([#287](https://github.com/JorisJonkers-dev/lodwright-rs/issues/287)) ([df38006](https://github.com/JorisJonkers-dev/lodwright-rs/commit/df38006bd37e976a6b3bfb5d05c2ac985e9ce253))

## [0.15.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.14.0...v0.15.0) (2026-11-21)


### Features

* **generate:** option output.pack packs the generated files into a BSA with a stub plugin, off by default ([#136](https://github.com/JorisJonkers-dev/lodwright-rs/issues/136)) ([#274](https://github.com/JorisJonkers-dev/lodwright-rs/issues/274)) ([0a04e38](https://github.com/JorisJonkers-dev/lodwright-rs/commit/0a04e3807202526e3bd1d868fe8fbb88f24a58f9))


### Bug Fixes

* **assets:** the stub plugin names a master, because the game opens a plugin's archives only when it has one ([#132](https://github.com/JorisJonkers-dev/lodwright-rs/issues/132)) ([#278](https://github.com/JorisJonkers-dev/lodwright-rs/issues/278)) ([fbc08e1](https://github.com/JorisJonkers-dev/lodwright-rs/commit/fbc08e1516a8f1419c4847b1699e744235a79152))

## [0.14.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.13.0...v0.14.0) (2026-11-21)


### Features

* **assets:** BSA v104 writer with name hashes, record order and refusals ([#130](https://github.com/JorisJonkers-dev/lodwright-rs/issues/130)) ([#270](https://github.com/JorisJonkers-dev/lodwright-rs/issues/270)) ([39a91bf](https://github.com/JorisJonkers-dev/lodwright-rs/commit/39a91bfda67c0d09200a515384a815b5a7533d73))
* **assets:** stub plugin that registers a packed archive by name ([#131](https://github.com/JorisJonkers-dev/lodwright-rs/issues/131)) ([#273](https://github.com/JorisJonkers-dev/lodwright-rs/issues/273)) ([78f7152](https://github.com/JorisJonkers-dev/lodwright-rs/commit/78f7152d90db0e4b7e2fa8068115dfbc0b94318a))
* **events:** run-wide progress that only moves forward, with phase index and phase totals ([#263](https://github.com/JorisJonkers-dev/lodwright-rs/issues/263)) ([#264](https://github.com/JorisJonkers-dev/lodwright-rs/issues/264)) ([11d800a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/11d800a7cb5b3d5d957694373fe7125abc619eee))
* **water:** option terrain.water_polygons writes the water planes of levels 8 to 32 as outline polygons, off by default ([#149](https://github.com/JorisJonkers-dev/lodwright-rs/issues/149)) ([#265](https://github.com/JorisJonkers-dev/lodwright-rs/issues/265)) ([5c8fddf](https://github.com/JorisJonkers-dev/lodwright-rs/commit/5c8fddfa8d9891f9506abafab4463bfd5c9bdb64))
* **water:** option terrain.water_shared_vertices writes shared plane corners once, off by default ([#149](https://github.com/JorisJonkers-dev/lodwright-rs/issues/149)) ([#261](https://github.com/JorisJonkers-dev/lodwright-rs/issues/261)) ([7a48072](https://github.com/JorisJonkers-dev/lodwright-rs/commit/7a48072b36705ae723793ab972de0238e6c4e33b))
* **water:** option terrain.water_walls writes walls under the water outline, off by default ([#149](https://github.com/JorisJonkers-dev/lodwright-rs/issues/149)) ([#259](https://github.com/JorisJonkers-dev/lodwright-rs/issues/259)) ([5b5d41e](https://github.com/JorisJonkers-dev/lodwright-rs/commit/5b5d41e7d0cc6ee5074b1ab5f46acb03f16fbef4))


### Bug Fixes

* **water:** terrain.water_polygons ships in 0.14.0 ([#149](https://github.com/JorisJonkers-dev/lodwright-rs/issues/149)) ([#266](https://github.com/JorisJonkers-dev/lodwright-rs/issues/266)) ([f944d49](https://github.com/JorisJonkers-dev/lodwright-rs/commit/f944d49db60b5fa52dc90402200a3092fa030e12))

## [0.13.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.12.0...v0.13.0) (2026-11-21)


### Features

* **verify:** water profile per level, to see walls, depth and height of the water outline ([#149](https://github.com/JorisJonkers-dev/lodwright-rs/issues/149)) ([#255](https://github.com/JorisJonkers-dev/lodwright-rs/issues/255)) ([85af450](https://github.com/JorisJonkers-dev/lodwright-rs/commit/85af450634c218fb4ad2aadd35de8b2a3c649435))


### Performance Improvements

* **terrain:** scan each triangle row by row when scoring greedy insertion ([#256](https://github.com/JorisJonkers-dev/lodwright-rs/issues/256)) ([#257](https://github.com/JorisJonkers-dev/lodwright-rs/issues/257)) ([2ea915f](https://github.com/JorisJonkers-dev/lodwright-rs/commit/2ea915f440d7a5c13d57144a6d34675a440d1780))

## [0.12.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.11.0...v0.12.0) (2026-11-21)


### Features

* **terrain:** choose block vertices by the true deviation inside each triangle ([#248](https://github.com/JorisJonkers-dev/lodwright-rs/issues/248)) ([#250](https://github.com/JorisJonkers-dev/lodwright-rs/issues/250)) ([bee1cb1](https://github.com/JorisJonkers-dev/lodwright-rs/commit/bee1cb157da0a5dab2896f1d0d689bdf4fa89354))
* **terrain:** greedy insertion with edge vertices chosen along each edge ([#248](https://github.com/JorisJonkers-dev/lodwright-rs/issues/248)) ([#253](https://github.com/JorisJonkers-dev/lodwright-rs/issues/253)) ([711c322](https://github.com/JorisJonkers-dev/lodwright-rs/commit/711c322018247651de4a5dcfe8f12306af2aea3d))

## [0.11.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.10.0...v0.11.0) (2026-11-21)


### Features

* **water:** write the game's empty segmented shape in blocks whose cells all have default water ([#149](https://github.com/JorisJonkers-dev/lodwright-rs/issues/149)) ([#244](https://github.com/JorisJonkers-dev/lodwright-rs/issues/244)) ([a262e12](https://github.com/JorisJonkers-dev/lodwright-rs/commit/a262e122ea1bf3f8e924e184d23a996f1ee489c3))


### Bug Fixes

* **verify:** do not check object blocks against a grid when the world has no settings file ([#245](https://github.com/JorisJonkers-dev/lodwright-rs/issues/245)) ([#247](https://github.com/JorisJonkers-dev/lodwright-rs/issues/247)) ([39bedb7](https://github.com/JorisJonkers-dev/lodwright-rs/commit/39bedb707ddcec12c63862517fac7ce1fa773b60))

## [0.10.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.9.0...v0.10.0) (2026-11-21)


### Features

* **water:** level 4 squares count as wet only 25 units below the water height ([#149](https://github.com/JorisJonkers-dev/lodwright-rs/issues/149)) ([#239](https://github.com/JorisJonkers-dev/lodwright-rs/issues/239)) ([9b6dd63](https://github.com/JorisJonkers-dev/lodwright-rs/commit/9b6dd63ed30cd318017d6575abd1e4f5648d4fea))

## [0.9.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.8.0...v0.9.0) (2026-11-21)


### Features

* **terrain:** complete the block set over the settings extent for worlds the game builds terrain LOD for ([#229](https://github.com/JorisJonkers-dev/lodwright-rs/issues/229)) ([#237](https://github.com/JorisJonkers-dev/lodwright-rs/issues/237)) ([c735903](https://github.com/JorisJonkers-dev/lodwright-rs/commit/c73590341f4c6757525f790327229ce7c549f43a))
* **terrain:** option to leave out terrain faces under the water, off by default ([#151](https://github.com/JorisJonkers-dev/lodwright-rs/issues/151)) ([#226](https://github.com/JorisJonkers-dev/lodwright-rs/issues/226)) ([9a79d23](https://github.com/JorisJonkers-dev/lodwright-rs/commit/9a79d23a9303d6d143645da1718598c6eea46183))
* **terrain:** skirts at block borders, off by default ([#90](https://github.com/JorisJonkers-dev/lodwright-rs/issues/90)) ([#223](https://github.com/JorisJonkers-dev/lodwright-rs/issues/223)) ([342fc5c](https://github.com/JorisJonkers-dev/lodwright-rs/commit/342fc5cb56531e20b2e0428694780e880f82ce7a))
* **water:** merge wet squares of one water height across cells above level 4 ([#149](https://github.com/JorisJonkers-dev/lodwright-rs/issues/149)) ([#228](https://github.com/JorisJonkers-dev/lodwright-rs/issues/228)) ([046b3b7](https://github.com/JorisJonkers-dev/lodwright-rs/commit/046b3b739b078a3bcc77c35a1c497e05a557df9e))


### Bug Fixes

* **assets:** count the embedded path in the stored size and refuse a path longer than its block ([#138](https://github.com/JorisJonkers-dev/lodwright-rs/issues/138)) ([#218](https://github.com/JorisJonkers-dev/lodwright-rs/issues/218)) ([1316038](https://github.com/JorisJonkers-dev/lodwright-rs/commit/1316038aea3895724865a5d7977acfea35de8eec))
* **terrain:** anchor the LOD block grid at the world's minimum corner cell, as the game's own tree does ([#219](https://github.com/JorisJonkers-dev/lodwright-rs/issues/219)) ([#220](https://github.com/JorisJonkers-dev/lodwright-rs/issues/220)) ([d6c25f8](https://github.com/JorisJonkers-dev/lodwright-rs/commit/d6c25f895c1b4dc797760c96fbb76144ca1e5067))
* **terrain:** start object blocks, tree blocks and the settings file at the same grid cell as the terrain blocks ([#219](https://github.com/JorisJonkers-dev/lodwright-rs/issues/219)) ([#225](https://github.com/JorisJonkers-dev/lodwright-rs/issues/225)) ([d268509](https://github.com/JorisJonkers-dev/lodwright-rs/commit/d26850979bcea1d195dc9c08104b61b49428b445))
* **terrain:** the block grid starts at the smallest CELL record of the world ([#229](https://github.com/JorisJonkers-dev/lodwright-rs/issues/229)) ([#236](https://github.com/JorisJonkers-dev/lodwright-rs/issues/236)) ([6d8926a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/6d8926a9dec8a325c94f721b38a9d56c05cdcce1))
* **terrain:** write only the levels the world's width allows and the game's settings extent ([#229](https://github.com/JorisJonkers-dev/lodwright-rs/issues/229)) ([#231](https://github.com/JorisJonkers-dev/lodwright-rs/issues/231)) ([e627fed](https://github.com/JorisJonkers-dev/lodwright-rs/commit/e627fed8b211d63af5100b4c0fb040c94a11b2d5))

## [0.8.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.7.0...v0.8.0) (2026-11-20)


### Features

* **verify:** report per-channel texture correlation against the reference ([#91](https://github.com/JorisJonkers-dev/lodwright-rs/issues/91)) ([#212](https://github.com/JorisJonkers-dev/lodwright-rs/issues/212)) ([3732eb0](https://github.com/JorisJonkers-dev/lodwright-rs/commit/3732eb0f7cf02d2c9c9aff3b86d78374514b878d))

## [0.7.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.6.0...v0.7.0) (2026-11-20)


### Features

* **generate:** build in a staging folder and swap it in on success, resume a cancelled or killed run ([#209](https://github.com/JorisJonkers-dev/lodwright-rs/issues/209)) ([e2844d9](https://github.com/JorisJonkers-dev/lodwright-rs/commit/e2844d9351486c44f958b4807131ed5cf643a22d)), closes [#134](https://github.com/JorisJonkers-dev/lodwright-rs/issues/134)
* **terrain:** specular in the alpha of the normal maps, off by default ([#210](https://github.com/JorisJonkers-dev/lodwright-rs/issues/210)) ([807fe55](https://github.com/JorisJonkers-dev/lodwright-rs/commit/807fe55ea84ba950e9d1594610b4f8b2ea886a1b))
* **verify:** compare terrain heights against the reference, with a tolerance and per-level numbers ([#206](https://github.com/JorisJonkers-dev/lodwright-rs/issues/206)) ([c639cf2](https://github.com/JorisJonkers-dev/lodwright-rs/commit/c639cf237f097eb416c347e2bed73e74bd83159d))

## [0.6.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.5.0...v0.6.0) (2026-11-20)


### Features

* **api:** expose the effective request defaults through lodwright-api ([#190](https://github.com/JorisJonkers-dev/lodwright-rs/issues/190)) ([bc9bd95](https://github.com/JorisJonkers-dev/lodwright-rs/commit/bc9bd95e23396dc643f1c2f74448f854557c0303))
* **generate:** hold the memory budget while the run goes ([#192](https://github.com/JorisJonkers-dev/lodwright-rs/issues/192)) ([7bf3194](https://github.com/JorisJonkers-dev/lodwright-rs/commit/7bf31940e69ab091a413b9bad63488b9b9daecc6))
* **objects:** optionally drop object faces under the terrain and the water ([#195](https://github.com/JorisJonkers-dev/lodwright-rs/issues/195)) ([7e04dd6](https://github.com/JorisJonkers-dev/lodwright-rs/commit/7e04dd6293df297456e8dccfe079d0f6f2f44d6a))
* **terrain:** per-texture tiling scale in the terrain diffuse maps ([#196](https://github.com/JorisJonkers-dev/lodwright-rs/issues/196)) ([11fe78a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/11fe78a0d5f7bcd05f8635b7700058f04b430a10))
* **verify:** a known-differences list with reasons, and the water numbers per level ([#197](https://github.com/JorisJonkers-dev/lodwright-rs/issues/197)) ([235dd83](https://github.com/JorisJonkers-dev/lodwright-rs/commit/235dd830bbda0955c8c07fab8ac19b1a3340eae6))
* **verify:** print the minimum and maximum of each statistic over several runs ([#194](https://github.com/JorisJonkers-dev/lodwright-rs/issues/194)) ([b1cb304](https://github.com/JorisJonkers-dev/lodwright-rs/commit/b1cb304a096e3bab195a18b610e916c56768919f))


### Bug Fixes

* **deps:** update minor and patch dependencies ([#199](https://github.com/JorisJonkers-dev/lodwright-rs/issues/199)) ([6a7ac8e](https://github.com/JorisJonkers-dev/lodwright-rs/commit/6a7ac8e08410b8f6193919e872fa4b8acee9279c))

## [0.5.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.4.2...v0.5.0) (2026-11-19)


### ⚠ BREAKING CHANGES

* **verify:** typed verify contract in the handshake, and documented exit codes ([#177](https://github.com/JorisJonkers-dev/lodwright-rs/issues/177))

### Features

* **generate:** progress events for object and tree LOD ([#178](https://github.com/JorisJonkers-dev/lodwright-rs/issues/178)) ([8f20913](https://github.com/JorisJonkers-dev/lodwright-rs/commit/8f20913ff067c4742c7aa39b6ee440fda77f3311))
* **release:** attach a universal macOS binary to each release ([#181](https://github.com/JorisJonkers-dev/lodwright-rs/issues/181)) ([10012bb](https://github.com/JorisJonkers-dev/lodwright-rs/commit/10012bbc52c04e8e2a6a8c47c600c2e0084fa4db))
* **verify:** report per-block water IoU and object block agreement against a reference ([#188](https://github.com/JorisJonkers-dev/lodwright-rs/issues/188)) ([30ec22f](https://github.com/JorisJonkers-dev/lodwright-rs/commit/30ec22f58ed90372c631d8b097199736fbf6fc7c))
* **verify:** typed verify contract in the handshake, and documented exit codes ([#177](https://github.com/JorisJonkers-dev/lodwright-rs/issues/177)) ([27f428a](https://github.com/JorisJonkers-dev/lodwright-rs/commit/27f428a24a8d90f2fb36ef907bf62c5821ac37d3))


### Bug Fixes

* **contract:** regenerate the events schema for the other block kind ([#185](https://github.com/JorisJonkers-dev/lodwright-rs/issues/185)) ([d7bdad2](https://github.com/JorisJonkers-dev/lodwright-rs/commit/d7bdad2a5453ec5ffb8a6acbdf5a6f0dc99ff929))
* **generate:** never write or remove outside the output folder through links or manifest paths ([#186](https://github.com/JorisJonkers-dev/lodwright-rs/issues/186)) ([a327d82](https://github.com/JorisJonkers-dev/lodwright-rs/commit/a327d822c56f278c00ffed839637bd0d6991a9f1))
* **generate:** read an unknown block kind as other and cover the request form with objects and trees ([#182](https://github.com/JorisJonkers-dev/lodwright-rs/issues/182)) ([1c6ebf3](https://github.com/JorisJonkers-dev/lodwright-rs/commit/1c6ebf3df7406a6988a912210f699a55ebb6a562))

## [0.4.2](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.4.1...v0.4.2) (2026-11-18)


### Bug Fixes

* **deps:** use caret requirements so the Components link together ([#116](https://github.com/JorisJonkers-dev/lodwright-rs/issues/116)) ([3af826c](https://github.com/JorisJonkers-dev/lodwright-rs/commit/3af826cbf5c70bbbde080f3693530ba1ff1f8932))

## [0.4.1](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.4.0...v0.4.1) (2026-11-18)


### Bug Fixes

* **deps:** pin dependencies ([#108](https://github.com/JorisJonkers-dev/lodwright-rs/issues/108)) ([4fe1142](https://github.com/JorisJonkers-dev/lodwright-rs/commit/4fe1142625bf1ace4bd2d90aef37d6db3111d3b7))

## [0.4.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.3.0...v0.4.0) (2026-11-17)


### Features

* **api:** library API for in-process LOD generation ([#102](https://github.com/JorisJonkers-dev/lodwright-rs/issues/102)) ([a686f8b](https://github.com/JorisJonkers-dev/lodwright-rs/commit/a686f8b809801900a1e4284fbfb1e39b517660bd))

## [0.3.0](https://github.com/JorisJonkers-dev/lodwright-rs/compare/v0.2.0...v0.3.0) (2026-11-16)


### Features

* generate terrain LOD for a world with adaptive meshes ([#67](https://github.com/JorisJonkers-dev/lodwright-rs/issues/67)) ([1406d43](https://github.com/JorisJonkers-dev/lodwright-rs/commit/1406d4348069d19243ed74cd08b2ea50417455d6))
* **generate:** keep unchanged terrain blocks across runs ([#84](https://github.com/JorisJonkers-dev/lodwright-rs/issues/84)) ([b66a5aa](https://github.com/JorisJonkers-dev/lodwright-rs/commit/b66a5aa9471a5025e417033179ca0b67c038cef0))
* land texture layers in terrain diffuse maps ([#72](https://github.com/JorisJonkers-dev/lodwright-rs/issues/72)) ([0d670cb](https://github.com/JorisJonkers-dev/lodwright-rs/commit/0d670cb32307d4fbc30c19ed847cc34123314c29))
* match the game's LOD tree, settings file codec and terrain block NIF writer ([#66](https://github.com/JorisJonkers-dev/lodwright-rs/issues/66)) ([aa40793](https://github.com/JorisJonkers-dev/lodwright-rs/commit/aa40793d0a6d7cdda1111f431456b5b315189a8b))
* object LOD generation ([#70](https://github.com/JorisJonkers-dev/lodwright-rs/issues/70)) ([fc6b341](https://github.com/JorisJonkers-dev/lodwright-rs/commit/fc6b3410524c19c6e031cbca84615e4847c6a067))
* progress events, README and gap list ([#74](https://github.com/JorisJonkers-dev/lodwright-rs/issues/74)) ([dabd010](https://github.com/JorisJonkers-dev/lodwright-rs/commit/dabd010e54035b4617ede9b82d88544b8a33c2e0))
* request document, inclusion rules and manifest for generate ([#73](https://github.com/JorisJonkers-dev/lodwright-rs/issues/73)) ([9d87bb5](https://github.com/JorisJonkers-dev/lodwright-rs/commit/9d87bb5f7f17adf8840bb3e7cc1ec410cfcc8b03))
* **terrain:** water planes in terrain blocks ([#83](https://github.com/JorisJonkers-dev/lodwright-rs/issues/83)) ([e2cf3fd](https://github.com/JorisJonkers-dev/lodwright-rs/commit/e2cf3fd240a2966e82007723cb417f77bf47179f))
* **trees:** tree LOD lists, block files and billboard atlas ([#79](https://github.com/JorisJonkers-dev/lodwright-rs/issues/79)) ([2edd83f](https://github.com/JorisJonkers-dev/lodwright-rs/commit/2edd83fe29e660e5c88a0672e1b28243818d3f41))
* use esplugin for plugin headers in a GPL-3.0 crate and add a licence map ([#64](https://github.com/JorisJonkers-dev/lodwright-rs/issues/64)) ([0001fb4](https://github.com/JorisJonkers-dev/lodwright-rs/commit/0001fb49add11e118b35ef81803ec728c9590492))
* verify command for generated LOD folders ([#71](https://github.com/JorisJonkers-dev/lodwright-rs/issues/71)) ([2dbabe8](https://github.com/JorisJonkers-dev/lodwright-rs/commit/2dbabe890d275009b3eae156f8d2a3c4269dc781))


### Bug Fixes

* **licence:** mark the crate linking esplugin as GPL-3.0-only ([#69](https://github.com/JorisJonkers-dev/lodwright-rs/issues/69)) ([9819e33](https://github.com/JorisJonkers-dev/lodwright-rs/commit/9819e33285b459fecf4eed2a338183a063f0cbfe))
* **objects:** keep model translations out of the reference scale and ignore node rotations ([#80](https://github.com/JorisJonkers-dev/lodwright-rs/issues/80)) ([a51835e](https://github.com/JorisJonkers-dev/lodwright-rs/commit/a51835ef9a1674796e0b07336c35898feca1fdfd))
* **objects:** place temporary references in the block of their cell ([#81](https://github.com/JorisJonkers-dev/lodwright-rs/issues/81)) ([3816fc2](https://github.com/JorisJonkers-dev/lodwright-rs/commit/3816fc2d56957979cb18d1ff7ee573be46aab038))

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
