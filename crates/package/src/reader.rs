//! Independent logical cursors over the same open package (no shared seek).
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    os::unix::fs::FileExt,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub(crate) struct PackageReader {
    file: Arc<File>,
    base: u64,
    len: u64,
    pos: u64,
}

impl PackageReader {
    pub(crate) fn new(file: File, base: u64) -> io::Result<Self> {
        let len = file
            .metadata()?
            .len()
            .checked_sub(base)
            .ok_or_else(|| io::Error::other("ZIP starts beyond file"))?;
        Ok(Self {
            file: Arc::new(file),
            base,
            len,
            pos: 0,
        })
    }
}

impl Read for PackageReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.len.saturating_sub(self.pos);
        let count = buf
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        if count == 0 {
            return Ok(0);
        }
        let offset = self
            .base
            .checked_add(self.pos)
            .ok_or_else(|| io::Error::other("ZIP offset overflow"))?;
        let count = self.file.read_at(&mut buf[..count], offset)?;
        self.pos += count as u64;
        Ok(count)
    }
}

impl Seek for PackageReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let pos = match from {
            SeekFrom::Start(pos) => i128::from(pos),
            SeekFrom::Current(delta) => i128::from(self.pos) + i128::from(delta),
            SeekFrom::End(delta) => i128::from(self.len) + i128::from(delta),
        };
        self.pos = u64::try_from(pos)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid ZIP seek"))?;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn cloned_cursors_are_independent_and_region_relative() {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"headerabcdef").unwrap();
        let mut a = PackageReader::new(file, 6).unwrap();
        let mut b = a.clone();
        a.seek(SeekFrom::Start(2)).unwrap();
        let mut bytes = [0; 2];
        b.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ab");
        a.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"cd");
        b.seek(SeekFrom::End(-2)).unwrap();
        b.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ef");
        assert!(b.seek(SeekFrom::Current(-7)).is_err());
        b.seek(SeekFrom::Start(u64::MAX)).unwrap();
        assert_eq!(b.read(&mut bytes).unwrap(), 0);
    }
}
