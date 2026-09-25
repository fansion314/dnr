//! Portable, bounded v3 metadata. All integers are little endian; no native struct layout.
use crate::{Manifest, config::Target, index::Record};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const META: &str = ".dnr/meta.bin";
pub const INSTALL: &str = ".dnr/install.bin";
pub const INSTALL_LOCK: &str = ".dnr/install.lock";
pub const LIMIT: u64 = 64 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"DNRMETA3";

pub(crate) fn read_prefix(file: &mut std::fs::File, offset: u64) -> Result<Option<Vec<u8>>> {
    use std::io::{Read, Seek, SeekFrom};
    file.seek(SeekFrom::Start(offset))?;
    let mut header = [0u8; 30];
    file.read_exact(&mut header)?;
    ensure!(&header[..4] == b"PK\x03\x04", "invalid ZIP local header");
    let name_len = u16::from_le_bytes(header[26..28].try_into()?) as usize;
    let extra_len = u16::from_le_bytes(header[28..30].try_into()?) as i64;
    let mut name = vec![0; name_len];
    file.read_exact(&mut name)?;
    if name != META.as_bytes() {
        return Ok(None);
    }
    ensure!(
        u16::from_le_bytes(header[6..8].try_into()?) & 9 == 0 && header[8..10] == [0, 0],
        "v3 metadata must be stored without encryption or data descriptor"
    );
    let size = u32::from_le_bytes(header[22..26].try_into()?);
    ensure!(
        u64::from(size) <= LIMIT && header[18..22] == header[22..26],
        "invalid metadata size"
    );
    file.seek(SeekFrom::Current(extra_len))?;
    let mut bytes = vec![0; size as usize];
    file.read_exact(&mut bytes)?;
    ensure!(
        crc32fast::hash(&bytes) == u32::from_le_bytes(header[14..18].try_into()?),
        "metadata CRC mismatch"
    );
    Ok(Some(bytes))
}

pub(crate) fn installation(bytes: &[u8], target: &str) -> Result<Vec<u8>> {
    let mut out = b"DNRINST3".to_vec();
    string(&mut out, target)?;
    out.extend(bytes);
    Ok(out)
}

pub(crate) struct Metadata {
    pub manifest: Manifest,
    pub records: Vec<Record>,
    pub content_hash: String,
}

fn string(out: &mut Vec<u8>, value: &str) -> Result<()> {
    out.extend(u32::try_from(value.len())?.to_le_bytes());
    out.extend(value.as_bytes());
    Ok(())
}
fn optional(out: &mut Vec<u8>, value: Option<&str>) -> Result<()> {
    out.push(u8::from(value.is_some()));
    if let Some(value) = value {
        string(out, value)?;
    }
    Ok(())
}
pub(crate) fn encode(manifest: &Manifest, records: &[Record]) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    string(&mut body, &manifest.entry)?;
    string(&mut body, &manifest.app_id)?;
    body.extend(u32::try_from(manifest.targets.len())?.to_le_bytes());
    for (id, target) in &manifest.targets {
        string(&mut body, id)?;
        string(&mut body, &target.os)?;
        string(&mut body, &target.arch)?;
        optional(&mut body, target.libc.as_deref())?;
    }
    let mut groups = manifest.groups.clone();
    groups.sort();
    body.extend(u32::try_from(groups.len())?.to_le_bytes());
    for group in &groups {
        string(&mut body, group)?;
    }
    let mut records: Vec<_> = records.iter().collect();
    records.sort_by(|a, b| (&a.path, &a.target, &a.source).cmp(&(&b.path, &b.target, &b.source)));
    body.extend(u32::try_from(records.len())?.to_le_bytes());
    for r in records {
        string(&mut body, &r.path)?;
        string(&mut body, &r.source)?;
        body.push(match r.kind.as_str() {
            "file" => 0,
            "directory" => 1,
            "symlink" => 2,
            _ => anyhow::bail!("invalid kind"),
        });
        body.extend(r.size.to_le_bytes());
        body.extend(r.mode.to_le_bytes());
        body.push(u8::from(r.sha256.is_some()));
        if let Some(hash) = &r.sha256 {
            ensure!(hash.len() == 64 && hash.is_ascii(), "invalid SHA-256");
            for i in (0..64).step_by(2) {
                body.push(u8::from_str_radix(&hash[i..i + 2], 16)?);
            }
        }
        for value in [&r.link, &r.group, &r.target, &r.native] {
            optional(&mut body, value.as_deref())?;
        }
        body.extend(r.napi.unwrap_or(0).to_le_bytes());
    }
    ensure!(body.len() as u64 <= LIMIT - 48, "oversized metadata");
    let mut out = Vec::with_capacity(body.len() + 48);
    out.extend(MAGIC);
    out.extend((body.len() as u64).to_le_bytes());
    out.extend(Sha256::digest(&body));
    out.extend(body);
    Ok(out)
}

struct Reader<'a>(&'a [u8]);
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize]);
        output.push(DIGITS[(byte & 15) as usize]);
    }
    String::from_utf8(output).expect("hex is ASCII")
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        ensure!(n <= self.0.len(), "truncated v3 metadata");
        let (bytes, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into()?))
    }
    fn string(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        Ok(std::str::from_utf8(self.take(n)?)?.into())
    }
    fn optional(&mut self) -> Result<Option<String>> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => anyhow::bail!("invalid optional field"),
        }
    }
    fn count(&mut self) -> Result<usize> {
        let n = self.u32()? as usize;
        ensure!(n <= self.0.len(), "invalid metadata count");
        Ok(n)
    }
}
pub(crate) fn decode(bytes: &[u8]) -> Result<Metadata> {
    ensure!(bytes.len() as u64 <= LIMIT, "oversized metadata");
    let mut r = Reader(bytes);
    ensure!(r.take(8)? == MAGIC, "unsupported metadata format");
    let len = r.u64()?;
    let hash = r.take(32)?;
    ensure!(
        r.0.len() as u64 == len && Sha256::digest(r.0).as_slice() == hash,
        "metadata checksum/length mismatch"
    );
    let content_hash = hex(hash);
    let entry = r.string()?;
    let app_id = r.string()?;
    let mut targets = BTreeMap::new();
    for _ in 0..r.count()? {
        let id = r.string()?;
        let t = Target {
            os: r.string()?,
            arch: r.string()?,
            libc: r.optional()?,
        };
        t.validate()?;
        ensure!(targets.insert(id, t).is_none(), "duplicate target");
    }
    let mut groups = Vec::new();
    for _ in 0..r.count()? {
        groups.push(r.string()?);
    }
    let mut records = Vec::new();
    for _ in 0..r.count()? {
        let path = r.string()?;
        let source = r.string()?;
        let kind = match r.byte()? {
            0 => "file",
            1 => "directory",
            2 => "symlink",
            _ => anyhow::bail!("invalid kind"),
        }
        .into();
        let size = r.u64()?;
        let mode = r.u32()?;
        let sha256 = match r.byte()? {
            0 => None,
            1 => Some(hex(r.take(32)?)),
            _ => anyhow::bail!("invalid checksum field"),
        };
        let link = r.optional()?;
        let group = r.optional()?;
        let target = r.optional()?;
        let native = r.optional()?;
        let napi = match r.u32()? {
            0 => None,
            n => Some(n),
        };
        records.push(Record {
            path,
            source,
            kind,
            size,
            mode,
            sha256,
            link,
            group,
            target,
            native,
            napi,
        });
    }
    ensure!(r.0.is_empty(), "trailing metadata bytes");
    Ok(Metadata {
        manifest: Manifest {
            format_version: 3,
            entry,
            app_id,
            targets,
            groups,
        },
        records,
        content_hash,
    })
}

/// A full installation has mutable source files; this descriptor supplies identity, never source hashes.
pub fn installed_lease(directory: &std::path::Path) -> Result<std::fs::File> {
    let path = directory.join(INSTALL_LOCK);
    let meta = std::fs::symlink_metadata(&path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "invalid install lock"
    );
    let lease = std::fs::File::open(path)?;
    lease.lock_shared()?;
    Ok(lease)
}

pub fn installed_manifest(directory: &std::path::Path) -> Result<(Manifest, String)> {
    let path = directory.join(INSTALL);
    let meta = std::fs::symlink_metadata(&path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= LIMIT,
        "invalid install descriptor"
    );
    let bytes = std::fs::read(path)?;
    let mut reader = Reader(&bytes);
    ensure!(reader.take(8)? == b"DNRINST3", "invalid full installation");
    let target = reader.string()?;
    let decoded = decode(reader.0).context("reading full installation")?;
    let host = Target::host();
    ensure!(
        target == host.id() || decoded.manifest.targets.get(&target) == Some(&host),
        "installation targets {target}, not {}",
        host.id()
    );
    ensure!(
        crate::is_normalized_name(&decoded.manifest.entry)
            && !decoded.manifest.entry.starts_with(".dnr/"),
        "invalid installed entry"
    );
    ensure!(
        !decoded.manifest.app_id.trim().is_empty()
            && !decoded.manifest.app_id.chars().any(char::is_control),
        "invalid installed appId"
    );
    Ok((decoded.manifest, decoded.content_hash))
}
