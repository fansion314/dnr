//! Independent v3 wire fixture writer for malformed-archive and CRC tests.
use dnr_package::{MARKER, Region};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn string(out: &mut Vec<u8>, value: &str) {
    out.extend((value.len() as u32).to_le_bytes());
    out.extend(value.as_bytes());
}
pub fn archive(path: &Path, entries: &[(&str, &str, bool)]) {
    let mut body = Vec::new();
    string(&mut body, "main.js");
    string(&mut body, "test.contents");
    body.extend(0u32.to_le_bytes()); // targets
    body.extend(0u32.to_le_bytes()); // groups
    body.extend((entries.len() as u32).to_le_bytes());
    for &(name, value, link) in entries {
        string(&mut body, name);
        string(&mut body, name);
        body.push(if link { 2 } else { 0 });
        body.extend((value.len() as u64).to_le_bytes());
        body.extend((if link { 0o777u32 } else { 0o644u32 }).to_le_bytes());
        body.push(1);
        body.extend(Sha256::digest(value.as_bytes()));
        body.push(u8::from(link));
        if link {
            string(&mut body, value);
        }
        body.extend([0, 0, 0]); // group, target, native
        body.extend(0u32.to_le_bytes()); // napi
    }
    let mut metadata = b"DNRMETA3".to_vec();
    metadata.extend((body.len() as u64).to_le_bytes());
    metadata.extend(Sha256::digest(&body));
    metadata.extend(body);
    let mut file = fs::File::create(path).unwrap();
    let header = format!("#!/bin/sh\nexit 127\n{MARKER}");
    file.write_all(header.as_bytes()).unwrap();
    let mut zip = ZipWriter::new(Region::new(file, header.len() as u64).unwrap());
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file(dnr_package::v3::META, options).unwrap();
    zip.write_all(&metadata).unwrap();
    for &(name, value, link) in entries {
        if link {
            zip.add_symlink(name, value, options.unix_permissions(0o777))
                .unwrap();
        } else {
            zip.start_file(name, options).unwrap();
            zip.write_all(value.as_bytes()).unwrap();
        }
    }
    zip.finish().unwrap();
}
