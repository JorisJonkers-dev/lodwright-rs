// SPDX-License-Identifier: MPL-2.0
//! Plugin (`.esm`/`.esp`) reader contract. Not implemented.
//!
//! Will read the TES4 header, the master list, the GRUP tree and records (zlib-compressed ones included), and
//! resolve `FormID`s through the master list and load order. Only the records the LOD generator needs are decoded
//! (`WRLD`, `CELL`, `LAND`, `LTEX`, `STAT`, `TREE`, `REFR`); the rest stay opaque bytes.
//!
//! Derived from: UESP and `Tes4Mod` format pages, and the record layouts in xEdit's `wbDefinitionsFNV.pas`
//! (MPL-2.0). Never from decompiled or unlicensed code.
