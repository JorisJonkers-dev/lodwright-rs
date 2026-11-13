// SPDX-License-Identifier: MPL-2.0
//! Synthetic plugin bytes: a TES4 header record and flat records, built and parsed back.
//!
//! Record layout used: 4-byte signature, `u32` data size, `u32` flags, `u32` `FormID`, 8 bytes of version
//! control, then the data. Data is a run of subrecords: 4-byte signature, `u16` size, bytes. There are no
//! groups and no compression; a reader for the real format is a separate, later piece of work.

const RECORD_HEADER: usize = 24;

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
        }
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
        let mut out = Vec::with_capacity(RECORD_HEADER + data.len());
        out.extend_from_slice(&self.signature);
        out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.form_id.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&data);
        out
    }
}

/// Builds plugin bytes: a TES4 header (with master names) followed by records.
#[derive(Debug, Default)]
pub struct PluginBuilder {
    masters: Vec<String>,
    records: Vec<Record>,
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
        self.records.push(record);
        self
    }

    /// The finished bytes.
    #[must_use]
    pub fn build(&self) -> Vec<u8> {
        let mut hedr = Vec::new();
        hedr.extend_from_slice(&1.0_f32.to_le_bytes());
        hedr.extend_from_slice(
            &u32::try_from(self.records.len())
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
        for record in &self.records {
            out.extend_from_slice(&record.to_bytes());
        }
        out
    }
}

/// Parse bytes made by [`PluginBuilder::build`] into records (the TES4 header first). `None` if truncated.
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
    use super::{PluginBuilder, Record, parse_records};

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
}
