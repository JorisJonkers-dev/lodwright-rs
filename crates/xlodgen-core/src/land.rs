// SPDX-License-Identifier: MPL-2.0
//! LAND heightfield contract (first slice). Not implemented.
//!
//! Will decode the VHGT subrecord of a LAND record (a 33x33 vertex grid per cell, stored as an offset and signed
//! byte deltas) into heights, and assemble the cells of a worldspace into one height raster, with neighbour
//! lookup for borders. The encoder used in tests lives in `xlodgen-testkit`.
//!
//! Derived from: UESP LAND record page and `wbDefinitionsFNV.pas`.
