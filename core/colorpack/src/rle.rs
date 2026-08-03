//! `regions.bin`：R16 ID map 的 RLE 無損編碼（`specs/baker-core-design.md §3.6`）。
//!
//! ```text
//! magic    "CLR1"          4 bytes
//! pixels   u32 LE          像素總數，解碼時據此檢查完整性
//! runs     [u16 LE len][u16 LE id]  ...   len ∈ 1..=65535
//! ```
//!
//! header 取 8 bytes，讓 run 陣列從 4-byte 對齊處開始——`regions.bin` 在 zip 內是
//! Stored，runtime 要能 mmap 後零拷貝取 slice（§3.1）。

use crate::Error;

pub const MAGIC: [u8; 4] = *b"CLR1";
pub const HEADER_LEN: usize = 8;
const MAX_RUN: usize = u16::MAX as usize;

/// ID map → RLE。ID 必須 ≤ 65535（呼叫端以 `region-count-overflow` 先擋）。
pub fn encode(ids: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + ids.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&(ids.len() as u32).to_le_bytes());

    let mut i = 0;
    while i < ids.len() {
        let value = ids[i];
        let mut len = 1;
        while i + len < ids.len() && ids[i + len] == value && len < MAX_RUN {
            len += 1;
        }
        out.extend_from_slice(&(len as u16).to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
        i += len;
    }
    out
}

pub fn decode(bytes: &[u8]) -> Result<Vec<u16>, Error> {
    if bytes.len() < HEADER_LEN || bytes[..4] != MAGIC {
        return Err(Error::Malformed("regions.bin 的 magic 不正確"));
    }
    let pixels = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let body = &bytes[HEADER_LEN..];
    if !body.len().is_multiple_of(4) {
        return Err(Error::Malformed("regions.bin 的 run 陣列長度非 4 的倍數"));
    }

    let mut out = Vec::with_capacity(pixels);
    for run in body.chunks_exact(4) {
        let len = u16::from_le_bytes([run[0], run[1]]) as usize;
        let value = u16::from_le_bytes([run[2], run[3]]);
        if len == 0 {
            return Err(Error::Malformed("regions.bin 含長度 0 的 run"));
        }
        if out.len() + len > pixels {
            return Err(Error::Malformed("regions.bin 的 run 總長超出宣告的像素數"));
        }
        out.resize(out.len() + len, value);
    }
    if out.len() != pixels {
        return Err(Error::Malformed("regions.bin 的 run 總長不足宣告的像素數"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn empty_round_trips() {
        assert_eq!(decode(&encode(&[])).unwrap(), Vec::<u16>::new());
    }

    #[test]
    fn runs_longer_than_u16_are_split() {
        let ids = vec![7u16; 70000];
        let bytes = encode(&ids);
        // 70000 = 65535 + 4465 → 兩個 run
        assert_eq!(bytes.len(), HEADER_LEN + 8);
        assert_eq!(decode(&bytes).unwrap(), ids);
    }

    #[test]
    fn truncated_input_is_rejected_not_panicking() {
        assert!(decode(&[]).is_err());
        assert!(decode(b"CLR1\x04\x00\x00\x00").is_err());
        assert!(decode(b"XXXX\x00\x00\x00\x00").is_err());
    }

    proptest! {
        #[test]
        fn round_trip(ids in prop::collection::vec(0u16..8, 0..4096)) {
            prop_assert_eq!(decode(&encode(&ids)).unwrap(), ids);
        }

        #[test]
        fn round_trip_sparse_ids(ids in prop::collection::vec(any::<u16>(), 0..1024)) {
            prop_assert_eq!(decode(&encode(&ids)).unwrap(), ids);
        }
    }
}
