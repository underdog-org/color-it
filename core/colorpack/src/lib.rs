//! 資產包（`.colorpack`）格式定義與讀寫——runtime 與 baker 共用。
//!
//! 這是 baker 與 runtime 的唯一共用面（`architecture.md §6` Boundary 4）。
//! 它只懂容器格式，**不依賴 `png`**——PNG／JPEG bytes 對它是不透明 blob。
//!
//! 格式規格見 `docs/specs/baker-core-design.md §3`。

pub mod container;
pub mod hash;
pub mod manifest;
pub mod region;
pub mod rle;

use std::fmt;
use std::io::{Read, Seek, Write};

pub use manifest::{Aspect, Category, Difficulty, Manifest};
pub use region::RegionEntry;

#[derive(Debug)]
pub enum Error {
    Zip(zip::result::ZipError),
    Io(std::io::Error),
    Json(serde_json::Error),
    SchemaVersion {
        found: String,
        expected: &'static str,
    },
    Malformed(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zip(e) => write!(f, "zip 容器錯誤：{e}"),
            Self::Io(e) => write!(f, "IO 錯誤：{e}"),
            Self::Json(e) => write!(f, "JSON 錯誤：{e}"),
            Self::SchemaVersion { found, expected } => write!(
                f,
                "不認得的 schema_version {found}（本版支援 {expected} 的 major）"
            ),
            Self::Malformed(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// 一個 `.colorpack` 的完整內容。
///
/// `region_ids` 已解 RLE；PNG／JPEG 保持原始 bytes（本 crate 不解碼影像）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorPack {
    pub manifest: Manifest,
    pub regions: Vec<RegionEntry>,
    /// 長度 = `canvas_size[0] * canvas_size[1]`，raster order。
    pub region_ids: Vec<u16>,
    pub lineart_png: Vec<u8>,
    pub shade_png: Option<Vec<u8>>,
    pub thumb_jpg: Vec<u8>,
}

impl ColorPack {
    /// 寫出容器，並就地把 `manifest.content_hash` 更新成正規化 hash（§3.3）。
    ///
    /// `manifest.has_shade` 以 `shade_png` 是否存在為準——兩者不一致時以資料為準並回寫，
    /// 避免產出一份自己說謊的 manifest。
    pub fn write_to<W: Write + Seek>(&mut self, sink: W) -> Result<(), Error> {
        self.manifest.has_shade = self.shade_png.is_some();
        self.manifest.schema_version = manifest::SCHEMA_VERSION.to_owned();

        let regions_json = serde_json::to_vec(&self.regions)?;
        let regions_bin = rle::encode(&self.region_ids);

        // hash 排除 manifest.json 自己（§3.3）
        let mut hashed: Vec<(&str, &[u8])> = vec![
            (container::REGIONS_JSON, &regions_json),
            (container::REGIONS_BIN, &regions_bin),
            (container::LINEART, &self.lineart_png),
        ];
        if let Some(shade) = &self.shade_png {
            hashed.push((container::SHADE, shade));
        }
        hashed.push((container::THUMB, &self.thumb_jpg));
        self.manifest.content_hash = hash::content_hash(&hashed);

        let manifest_json = serde_json::to_vec(&self.manifest)?;
        let mut entries: Vec<(&str, &[u8])> = vec![(container::MANIFEST, &manifest_json)];
        entries.extend_from_slice(&hashed);

        container::write(sink, &entries)
    }

    pub fn open<R: Read + Seek>(source: R) -> Result<Self, Error> {
        let mut zip = zip::ZipArchive::new(source)?;

        let manifest: Manifest =
            serde_json::from_slice(&container::read_entry(&mut zip, container::MANIFEST)?)?;
        manifest::check_schema_version(&manifest.schema_version)?;

        let regions_json = container::read_entry(&mut zip, container::REGIONS_JSON)?;
        let regions_bin = container::read_entry(&mut zip, container::REGIONS_BIN)?;
        let lineart_png = container::read_entry(&mut zip, container::LINEART)?;
        let shade_png = manifest
            .has_shade
            .then(|| container::read_entry(&mut zip, container::SHADE))
            .transpose()?;
        let thumb_jpg = container::read_entry(&mut zip, container::THUMB)?;

        let mut hashed: Vec<(&str, &[u8])> = vec![
            (container::REGIONS_JSON, &regions_json),
            (container::REGIONS_BIN, &regions_bin),
            (container::LINEART, &lineart_png),
        ];
        if let Some(shade) = &shade_png {
            hashed.push((container::SHADE, shade));
        }
        hashed.push((container::THUMB, &thumb_jpg));
        if hash::content_hash(&hashed) != manifest.content_hash {
            return Err(Error::Malformed("content_hash 與實際內容不符"));
        }

        let regions: Vec<RegionEntry> = serde_json::from_slice(&regions_json)?;
        let region_ids = rle::decode(&regions_bin)?;

        let expected = manifest.canvas_size[0] as usize * manifest.canvas_size[1] as usize;
        if region_ids.len() != expected {
            return Err(Error::Malformed(
                "regions.bin 的像素數與 manifest.canvas_size 不符",
            ));
        }
        if regions.len() != manifest.region_count as usize {
            return Err(Error::Malformed(
                "regions.json 的筆數與 manifest.region_count 不符",
            ));
        }
        // 超界 ID 會變成 runtime 永遠點不到的幽靈區——Mode A 的遮罩是
        // `id == active_region_id`，而 active id 只可能來自 regions.json。
        if region_ids
            .iter()
            .any(|&id| id as u32 >= manifest.region_count)
        {
            return Err(Error::Malformed(
                "regions.bin 含超出 manifest.region_count 的 ID",
            ));
        }

        Ok(Self {
            manifest,
            regions,
            region_ids,
            lineart_png,
            shade_png,
            thumb_jpg,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    pub(crate) fn sample() -> ColorPack {
        ColorPack {
            manifest: Manifest {
                schema_version: manifest::SCHEMA_VERSION.to_owned(),
                id: "kirby-demo-1".to_owned(),
                content_hash: String::new(),
                canvas_size: [4, 2],
                aspect: Aspect::Square,
                region_count: 2,
                difficulty: Difficulty::Easy,
                category: Category::Cartoon,
                has_shade: true,
                palette: vec!["#FF0000".to_owned(), "#00FF00".to_owned()],
            },
            regions: vec![
                RegionEntry {
                    id: 0,
                    centroid: [0, 0],
                    area: 4,
                    bbox: [0, 0, 2, 2],
                    suggested_color: "#FF0000".to_owned(),
                },
                RegionEntry {
                    id: 1,
                    centroid: [2, 0],
                    area: 4,
                    bbox: [2, 0, 2, 2],
                    suggested_color: "#00FF00".to_owned(),
                },
            ],
            region_ids: vec![0, 0, 1, 1, 0, 0, 1, 1],
            lineart_png: b"\x89PNG-lineart".to_vec(),
            shade_png: Some(b"\x89PNG-shade".to_vec()),
            thumb_jpg: b"\xff\xd8-thumb".to_vec(),
        }
    }

    fn write(pack: &mut ColorPack) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        pack.write_to(&mut buf).unwrap();
        buf.into_inner()
    }

    #[test]
    fn round_trip() {
        let mut pack = sample();
        let bytes = write(&mut pack);
        assert_eq!(ColorPack::open(Cursor::new(bytes)).unwrap(), pack);
    }

    #[test]
    fn round_trip_without_shade() {
        let mut pack = sample();
        pack.shade_png = None;
        let bytes = write(&mut pack);
        let reopened = ColorPack::open(Cursor::new(bytes)).unwrap();
        assert!(!reopened.manifest.has_shade);
        assert_eq!(reopened, pack);
    }

    #[test]
    fn writing_twice_is_bit_identical() {
        assert_eq!(write(&mut sample()), write(&mut sample()));
    }

    /// hash 追蹤的是內容而不是容器：改一個 entry 的 bytes 必須換 hash。
    #[test]
    fn content_hash_tracks_content() {
        let mut a = sample();
        write(&mut a);
        let mut b = sample();
        b.thumb_jpg = b"\xff\xd8-other".to_vec();
        write(&mut b);
        assert_ne!(a.manifest.content_hash, b.manifest.content_hash);
        assert!(a.manifest.content_hash.starts_with(hash::PREFIX));
    }

    #[test]
    fn sample_pack_content_hash_is_frozen() {
        let mut pack = sample();
        write(&mut pack);
        assert_eq!(
            pack.manifest.content_hash,
            "sha256:e471d23c43a526dd4aaa888eed212132fec3e0f593712d5ebcc88a691f416312"
        );
    }

    #[test]
    fn zip_entries_follow_the_declared_order() {
        let names = |pack: &mut ColorPack| -> Vec<String> {
            let bytes = write(pack);
            let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
            (0..zip.len())
                .map(|i| zip.by_index(i).unwrap().name().to_owned())
                .collect()
        };
        assert_eq!(names(&mut sample()), container::ENTRY_ORDER);

        let mut no_shade = sample();
        no_shade.shade_png = None;
        let expected: Vec<&str> = container::ENTRY_ORDER
            .iter()
            .copied()
            .filter(|n| *n != container::SHADE)
            .collect();
        assert_eq!(names(&mut no_shade), expected);
    }

    /// 拆開一個寫好的 pack、換掉指定的 entry，再照原順序打包回去。
    fn repack(bytes: Vec<u8>, replace: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_owned())
            .collect();
        let data: Vec<(String, Vec<u8>)> = names
            .into_iter()
            .map(|name| {
                let swapped = replace
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| v.clone());
                let bytes =
                    swapped.unwrap_or_else(|| container::read_entry(&mut zip, &name).unwrap());
                (name, bytes)
            })
            .collect();
        let entries: Vec<(&str, &[u8])> = data
            .iter()
            .map(|(n, d)| (n.as_str(), d.as_slice()))
            .collect();
        let mut out = Cursor::new(Vec::new());
        container::write(&mut out, &entries).unwrap();
        out.into_inner()
    }

    fn manifest_with(bytes: &[u8], key: &str, value: serde_json::Value) -> Vec<u8> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&container::read_entry(&mut zip, container::MANIFEST).unwrap())
                .unwrap();
        manifest[key] = value;
        serde_json::to_vec(&manifest).unwrap()
    }

    #[test]
    fn unknown_major_is_rejected_on_open() {
        let bytes = write(&mut sample());
        let manifest = manifest_with(&bytes, "schema_version", serde_json::json!("2.0"));
        let tampered = repack(bytes, &[(container::MANIFEST, manifest)]);
        assert!(matches!(
            ColorPack::open(Cursor::new(tampered)),
            Err(Error::SchemaVersion { .. })
        ));
    }

    #[test]
    fn a_tampered_entry_is_rejected_on_open() {
        let bytes = write(&mut sample());
        let tampered = repack(bytes, &[(container::THUMB, b"\xff\xd8-tampered".to_vec())]);
        assert!(matches!(
            ColorPack::open(Cursor::new(tampered)),
            Err(Error::Malformed(_))
        ));
    }

    /// 超界 ID 會變成 runtime 永遠點不到的幽靈區（Mode A 的遮罩是 `id == active`）。
    /// 這份 pack 的 hash 是自洽的——擋下它的必須是 ID 範圍檢查本身。
    #[test]
    fn an_out_of_range_region_id_is_rejected_on_open() {
        let mut pack = sample();
        assert_eq!(pack.manifest.region_count, 2);
        pack.region_ids = vec![0, 0, 1, 9, 0, 0, 1, 1];
        let bytes = write(&mut pack);
        assert!(matches!(
            ColorPack::open(Cursor::new(bytes)),
            Err(Error::Malformed(_))
        ));
    }
}
