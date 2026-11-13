// SPDX-License-Identifier: MPL-2.0
//! Synthetic plugin bytes: a TES4 header record and flat records, built and parsed back.
//!
//! Record layout used: 4-byte signature, `u32` data size, `u32` flags, `u32` `FormID`, 8 bytes of version
//! control, then the data. Data is a run of subrecords: 4-byte signature, `u16` size, bytes. Records can be nested
//! in groups (`GRUP`) and can be zlib-compressed, as in the real format.

// source: plugin-record-header
const RECORD_HEADER: usize = 24;
// source: plugin-group-layout
const GROUP_HEADER: usize = 24;
// source: plugin-record-compression
const COMPRESSED_FLAG: u32 = 0x0004_0000;

/// One synthetic record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Record signature, for example `*b"LAND"`.
    pub signature: [u8; 4],
    /// Record flags.
    pub flags: u32,
    /// `FormID`.
    pub form_id: u32,
    /// Subrecords in order: signature and bytes.
    pub subrecords: Vec<([u8; 4], Vec<u8>)>,
    /// Whether the data is written zlib-compressed (the flag is set when building).
    pub compressed: bool,
}

impl Record {
    /// A record with no flags and no subrecords.
    #[must_use]
    pub fn new(signature: [u8; 4], form_id: u32) -> Self {
        Self {
            signature,
            flags: 0,
            form_id,
            subrecords: Vec::new(),
            compressed: false,
        }
    }

    /// Write this record's data compressed.
    #[must_use]
    pub fn compressed(mut self) -> Self {
        self.compressed = true;
        self
    }

    /// Append a subrecord.
    ///
    /// # Panics
    /// If `data` is longer than 65535 bytes (a fixture never needs that).
    #[must_use]
    pub fn with_subrecord(mut self, signature: [u8; 4], data: Vec<u8>) -> Self {
        assert!(u16::try_from(data.len()).is_ok(), "subrecord too long");
        self.subrecords.push((signature, data));
        self
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::new();
        for (sig, bytes) in &self.subrecords {
            data.extend_from_slice(sig);
            data.extend_from_slice(&u16::try_from(bytes.len()).unwrap_or(u16::MAX).to_le_bytes());
            data.extend_from_slice(bytes);
        }
        let mut flags = self.flags;
        if self.compressed {
            let mut packed = u32::try_from(data.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes()
                .to_vec();
            packed.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&data, 6));
            data = packed;
            flags |= COMPRESSED_FLAG;
        }
        let mut out = Vec::with_capacity(RECORD_HEADER + data.len());
        out.extend_from_slice(&self.signature);
        out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&self.form_id.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&data);
        out
    }
}

/// A group of records and further groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Group type (0 top level, 1 world children, 4 exterior block, 5 exterior sub-block, 6 cell children, ...).
    pub group_type: u32,
    /// The 4 label bytes (their meaning depends on the type).
    pub label: [u8; 4],
    /// Children in order.
    pub children: Vec<Node>,
}

impl Group {
    /// An empty group.
    #[must_use]
    pub fn new(group_type: u32, label: [u8; 4]) -> Self {
        Self {
            group_type,
            label,
            children: Vec::new(),
        }
    }

    /// Append a child record or group.
    #[must_use]
    pub fn with(mut self, child: impl Into<Node>) -> Self {
        self.children.push(child.into());
        self
    }

    fn to_bytes(&self) -> Vec<u8> {
        let body: Vec<u8> = self.children.iter().flat_map(Node::to_bytes).collect();
        let mut out = Vec::with_capacity(GROUP_HEADER + body.len());
        out.extend_from_slice(b"GRUP");
        out.extend_from_slice(
            &u32::try_from(GROUP_HEADER + body.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        out.extend_from_slice(&self.label);
        out.extend_from_slice(&self.group_type.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&body);
        out
    }
}

/// A record or a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A record.
    Record(Record),
    /// A group.
    Group(Group),
}

impl Node {
    fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Record(r) => r.to_bytes(),
            Self::Group(g) => g.to_bytes(),
        }
    }
}

impl From<Record> for Node {
    fn from(r: Record) -> Self {
        Self::Record(r)
    }
}

impl From<Group> for Node {
    fn from(g: Group) -> Self {
        Self::Group(g)
    }
}

/// Builds plugin bytes: a TES4 header (with master names) followed by records and groups.
#[derive(Debug, Default)]
pub struct PluginBuilder {
    masters: Vec<String>,
    nodes: Vec<Node>,
}

impl PluginBuilder {
    /// An empty plugin.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a master file name.
    #[must_use]
    pub fn master(mut self, name: &str) -> Self {
        self.masters.push(name.to_owned());
        self
    }

    /// Add a record after the header.
    #[must_use]
    pub fn record(mut self, record: Record) -> Self {
        self.nodes.push(Node::Record(record));
        self
    }

    /// Add a top-level group after the header.
    #[must_use]
    pub fn group(mut self, group: Group) -> Self {
        self.nodes.push(Node::Group(group));
        self
    }

    // source: plugin-subrecord-layout
    /// The finished bytes.
    #[must_use]
    pub fn build(&self) -> Vec<u8> {
        let mut hedr = Vec::new();
        hedr.extend_from_slice(&1.0_f32.to_le_bytes());
        hedr.extend_from_slice(
            &u32::try_from(self.nodes.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        hedr.extend_from_slice(&0x800_u32.to_le_bytes());
        let mut header = Record::new(*b"TES4", 0).with_subrecord(*b"HEDR", hedr);
        for master in &self.masters {
            let mut name = master.as_bytes().to_vec();
            name.push(0);
            header = header
                .with_subrecord(*b"MAST", name)
                .with_subrecord(*b"DATA", vec![0; 8]);
        }
        let mut out = header.to_bytes();
        for node in &self.nodes {
            out.extend_from_slice(&node.to_bytes());
        }
        out
    }
}

/// Parse bytes made by [`PluginBuilder::build`] for a plugin of flat, uncompressed records (no groups) into
/// records (the TES4 header first). `None` if truncated.
#[must_use]
pub fn parse_records(bytes: &[u8]) -> Option<Vec<Record>> {
    let mut records = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let head = bytes.get(at..at + RECORD_HEADER)?;
        let size = usize::try_from(u32::from_le_bytes(head[4..8].try_into().ok()?)).ok()?;
        let mut record = Record::new(
            head[0..4].try_into().ok()?,
            u32::from_le_bytes(head[12..16].try_into().ok()?),
        );
        record.flags = u32::from_le_bytes(head[8..12].try_into().ok()?);
        let data = bytes.get(at + RECORD_HEADER..at + RECORD_HEADER + size)?;
        let mut sub = 0;
        while sub < data.len() {
            let sig: [u8; 4] = data.get(sub..sub + 4)?.try_into().ok()?;
            let len = usize::from(u16::from_le_bytes(
                data.get(sub + 4..sub + 6)?.try_into().ok()?,
            ));
            record
                .subrecords
                .push((sig, data.get(sub + 6..sub + 6 + len)?.to_vec()));
            sub += 6 + len;
        }
        records.push(record);
        at += RECORD_HEADER + size;
    }
    Some(records)
}

#[cfg(test)]
mod tests {
    use super::{Group, Node, PluginBuilder, Record, parse_records};

    #[test]
    fn header_and_records_round_trip() {
        let land = Record::new(*b"LAND", 0x0100_0001).with_subrecord(*b"VHGT", vec![1, 2, 3]);
        let bytes = PluginBuilder::new()
            .master("FalloutNV.esm")
            .record(land.clone())
            .build();
        let records = parse_records(&bytes).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(&records[0].signature, b"TES4");
        assert_eq!(
            records[0].subrecords[1],
            (*b"MAST", b"FalloutNV.esm\0".to_vec())
        );
        assert_eq!(records[1], land);
    }

    #[test]
    fn a_plugin_without_masters_has_only_the_header() {
        let records = parse_records(&PluginBuilder::new().build()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].subrecords.len(), 1);
    }

    #[test]
    fn truncated_bytes_are_rejected() {
        let bytes = PluginBuilder::new().build();
        assert_eq!(parse_records(&bytes[..bytes.len() - 1]), None);
        assert_eq!(parse_records(&bytes[..10]), None);
    }

    #[test]
    fn empty_input_has_no_records() {
        assert_eq!(parse_records(&[]), Some(Vec::new()));
    }

    #[test]
    fn a_group_header_is_24_bytes_and_its_size_covers_the_children() {
        let child = Record::new(*b"STAT", 1).with_subrecord(*b"EDID", b"x\0".to_vec());
        let group = Group::new(0, *b"STAT").with(child.clone());
        let bytes = group.to_bytes();
        assert_eq!(&bytes[0..4], b"GRUP");
        assert_eq!(
            usize::try_from(u32::from_le_bytes(bytes[4..8].try_into().unwrap())).unwrap(),
            bytes.len()
        );
        assert_eq!(&bytes[8..12], b"STAT");
        assert_eq!(bytes[24..], child.to_bytes()[..]);
    }

    #[test]
    fn a_compressed_record_sets_the_flag_and_prefixes_the_plain_size() {
        let plain = Record::new(*b"LAND", 5).with_subrecord(*b"DATA", vec![7; 100]);
        let bytes = plain.clone().compressed().to_bytes();
        let flags = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        assert_eq!(flags & 0x4_0000, 0x4_0000);
        let plain_len = plain.to_bytes().len() - 24;
        assert_eq!(
            usize::try_from(u32::from_le_bytes(bytes[24..28].try_into().unwrap())).unwrap(),
            plain_len
        );
        assert!(bytes.len() < plain.to_bytes().len());
    }

    #[test]
    fn nodes_convert_from_records_and_groups() {
        assert!(matches!(
            Node::from(Record::new(*b"STAT", 1)),
            Node::Record(_)
        ));
        assert!(matches!(
            Node::from(Group::new(0, *b"STAT")),
            Node::Group(_)
        ));
    }
}
