//! zip 讀寫（`specs/baker-core-design.md §3.1 §3.2`）。
//!
//! 規則：**副檔名決定壓縮方式，無例外**。二進位走 Stored 讓 runtime 可以 mmap 整個
//! pack 並零拷貝取 slice；JSON 走 Deflate，因為高區域數的 `regions.json` 會到 MB 級。

use std::io::{Read, Seek, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Error;

pub const MANIFEST: &str = "manifest.json";
pub const REGIONS_JSON: &str = "regions.json";
pub const REGIONS_BIN: &str = "regions.bin";
pub const LINEART: &str = "lineart.png";
pub const SHADE: &str = "shade.png";
pub const THUMB: &str = "thumb.jpg";

/// entry 順序固定。`shade.png` 在 `has_shade = false` 時整個不存在。
pub const ENTRY_ORDER: [&str; 6] = [MANIFEST, REGIONS_JSON, REGIONS_BIN, LINEART, SHADE, THUMB];

/// 固定 deflate level。換 level 會換掉檔案位元（但不會換掉 `content_hash`，見 `hash`）。
const DEFLATE_LEVEL: i64 = 6;

fn method_for(name: &str) -> CompressionMethod {
    match name.rsplit('.').next() {
        Some("json") => CompressionMethod::Deflated,
        _ => CompressionMethod::Stored,
    }
}

/// 決定性寫入：mtime 固定為 zip epoch（1980-01-01）、不寫 unix 權限／extra field／
/// comment、deflate level 固定。同輸入重跑 → 位元相同（§3.2）。
pub fn write<W: Write + Seek>(sink: W, entries: &[(&str, &[u8])]) -> Result<(), Error> {
    let mut zip = ZipWriter::new(sink);
    for (name, data) in entries {
        let method = method_for(name);
        let mut options = SimpleFileOptions::default()
            .compression_method(method)
            .last_modified_time(zip::DateTime::default())
            .large_file(false);
        if method == CompressionMethod::Deflated {
            options = options.compression_level(Some(DEFLATE_LEVEL));
        }
        zip.start_file(*name, options)?;
        zip.write_all(data)?;
    }
    zip.finish()?;
    Ok(())
}

pub fn read_entry<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Result<Vec<u8>, Error> {
    let mut entry = zip.by_name(name)?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn roundtrip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        write(&mut buf, entries).unwrap();
        buf.into_inner()
    }

    #[test]
    fn same_input_gives_identical_bytes() {
        let entries: [(&str, &[u8]); 2] = [("regions.json", b"[]"), ("regions.bin", b"CLR1")];
        assert_eq!(roundtrip(&entries), roundtrip(&entries));
    }

    #[test]
    fn extension_decides_compression_method() {
        let big = vec![b'a'; 4096];
        let entries: [(&str, &[u8]); 2] = [("a.json", &big), ("b.png", &big)];
        let bytes = roundtrip(&entries);
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(
            zip.by_name("a.json").unwrap().compression(),
            CompressionMethod::Deflated
        );
        assert_eq!(
            zip.by_name("b.png").unwrap().compression(),
            CompressionMethod::Stored
        );
    }

    #[test]
    fn mtime_is_pinned_to_zip_epoch() {
        let bytes = roundtrip(&[("a.json", b"[]")]);
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let entry = zip.by_name("a.json").unwrap();
        let t = entry.last_modified().unwrap();
        assert_eq!((t.year(), t.month(), t.day()), (1980, 1, 1));
    }
}
