//! Recover unknown ASVR key inputs (scene ID and/or version string) by exhaustive search.
//!
//! The key is `scrypt(passphrase, u32le(scene_id) || version || base_name)`. After decryption with
//! key_id `0xFFFFFFFF` every ASVR header starts with a fixed u64 format version (`4`) and the sizes
//! table starts with a zlib stream, so the first [`HEADER_PROBE_LEN`] bytes of a file are enough to
//! test a guess with ~80 bits of known plaintext. Each guess costs one scrypt (N=16384, r=8, p=1),
//! so the search is CPU-bound and runs on all available cores.

use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

use crate::serializers::{decrypt_frame_data, derive_key};

/// Number of leading file bytes needed to verify a key guess.
pub const HEADER_PROBE_LEN: usize = 18;

/// Decrypted header bytes 0..8: u64 LE format version 4 (`CryptoV4` / `HeaderV4` in libalphastream).
const HEADER_MAGIC: [u8; 8] = [4, 0, 0, 0, 0, 0, 0, 0];

/// Number of candidates a worker claims per atomic fetch.
const CHUNK: u64 = 16;

/// Header fields recovered by a successful guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderInfo {
    pub frame_count: u32,
    pub compressed_sizes_size: u32,
}

/// A key-input combination that decrypts the probed header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMatch {
    pub scene_id: u32,
    pub version: String,
    pub header: HeaderInfo,
}

/// Base name libalphastream uses as salt: the part after the last `/`, up to the first `?`.
pub fn base_name_from_url(url: &str) -> &str {
    let start = url.rfind('/').map_or(0, |i| i + 1);
    let end = url.find('?').unwrap_or(url.len());
    // A `?` before the last `/` (e.g. inside a query value) would give an empty slice, as in the original.
    url.get(start..end).unwrap_or("")
}

/// All `major.minor.patch` strings with each component in `0..=max` of the given bound.
pub fn version_sweep(max_major: u32, max_minor: u32, max_patch: u32) -> Vec<String> {
    let mut out = Vec::new();
    for a in 0..=max_major {
        for b in 0..=max_minor {
            for c in 0..=max_patch {
                out.push(format!("{a}.{b}.{c}"));
            }
        }
    }
    out
}

/// Decrypt the probe with `key` and check the fixed header prefix and zlib magic.
pub fn check_key(probe: &[u8], key: &[u8; 32]) -> Option<HeaderInfo> {
    if probe.len() < HEADER_PROBE_LEN {
        return None;
    }
    let plain = decrypt_frame_data(&probe[..HEADER_PROBE_LEN], key, 0xFFFF_FFFF).ok()?;
    // zlib header: CMF 0x78 (deflate, 32K window) and (CMF << 8 | FLG) divisible by 31
    let zlib_ok = plain[16] == 0x78 && (u16::from_be_bytes([plain[16], plain[17]]) % 31 == 0);
    if plain[..8] != HEADER_MAGIC || !zlib_ok {
        return None;
    }
    Some(HeaderInfo {
        frame_count: u32::from_le_bytes(plain[8..12].try_into().unwrap()),
        compressed_sizes_size: u32::from_le_bytes(plain[12..16].try_into().unwrap()),
    })
}

/// Test a single `(scene_id, version, base_name)` guess against the probe.
pub fn try_key(probe: &[u8], scene_id: u32, version: &str, base_name: &str) -> Option<HeaderInfo> {
    let key = derive_key(scene_id, version.as_bytes(), base_name.as_bytes()).ok()?;
    check_key(probe, &key)
}

/// Exhaustive search over `scene_ids × versions` on `threads` worker threads.
///
/// All versions are tried for a scene ID before moving to the next ID. Returns the first match
/// found (any match is conclusive given the known-plaintext check), or `None` if the space is
/// exhausted. `progress`, if given, is incremented by the number of guesses tried so the caller
/// can report progress from another thread.
pub fn find_key_inputs(
    probe: &[u8],
    scene_ids: RangeInclusive<u32>,
    versions: &[String],
    base_name: &str,
    threads: usize,
    progress: Option<&AtomicU64>,
) -> Option<KeyMatch> {
    if versions.is_empty() || scene_ids.is_empty() || probe.len() < HEADER_PROBE_LEN {
        return None;
    }
    let first_id = *scene_ids.start() as u64;
    let n_versions = versions.len() as u64;
    let total = (*scene_ids.end() as u64 - first_id + 1) * n_versions;
    let next = AtomicU64::new(0);
    let found = AtomicBool::new(false);

    let worker = || -> Option<KeyMatch> {
        while !found.load(Ordering::Relaxed) {
            let start = next.fetch_add(CHUNK, Ordering::Relaxed);
            if start >= total {
                break;
            }
            let end = (start + CHUNK).min(total);
            for i in start..end {
                let scene_id = (first_id + i / n_versions) as u32;
                let version = &versions[(i % n_versions) as usize];
                if let Some(header) = try_key(probe, scene_id, version, base_name) {
                    found.store(true, Ordering::Relaxed);
                    return Some(KeyMatch { scene_id, version: version.clone(), header });
                }
            }
            if let Some(p) = progress {
                p.fetch_add(end - start, Ordering::Relaxed);
            }
        }
        None
    };

    thread::scope(|s| {
        let handles: Vec<_> = (0..threads.max(1)).map(|_| s.spawn(worker)).collect();
        handles.into_iter().filter_map(|h| h.join().unwrap()).next()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serializers::{ASVRSerializer, FrameData};

    #[test]
    fn base_name_rules() {
        assert_eq!(base_name_from_url("https://cdn/x/12345/pov_mask.asvr?Expires=1&Signature=a%2Fb"), "pov_mask.asvr");
        assert_eq!(base_name_from_url("https://cdn/x/no_pov_mask.asvr"), "no_pov_mask.asvr");
        assert_eq!(base_name_from_url("pov_mask.asvr"), "pov_mask.asvr");
    }

    #[test]
    fn version_sweep_counts() {
        let v = version_sweep(1, 2, 3);
        assert_eq!(v.len(), 2 * 3 * 4);
        assert_eq!(v.first().unwrap(), "0.0.0");
        assert_eq!(v.last().unwrap(), "1.2.3");
    }

    #[test]
    fn short_or_garbage_probe_never_matches() {
        assert!(check_key(&[0u8; 10], &[0u8; 32]).is_none());
        assert!(check_key(&[0u8; HEADER_PROBE_LEN], &[0u8; 32]).is_none());
    }

    #[test]
    fn finds_known_scene_id() {
        let mut writer = ASVRSerializer::new(Vec::new(), 12345, b"1.5.0", b"pov_mask.asvr").unwrap();
        for i in 0..3u8 {
            writer.add_frame(FrameData { polystream: vec![i; 16], bitmap: None, triangle_vertices: None });
        }
        let data = writer.write_all().unwrap();
        let probe = &data[..HEADER_PROBE_LEN];
        let versions = vec!["1.0.0".to_string(), "1.5.0".to_string()];
        let hit = find_key_inputs(probe, 12340..=12345, &versions, "pov_mask.asvr", 4, None).expect("match");
        assert_eq!(hit.scene_id, 12345);
        assert_eq!(hit.version, "1.5.0");
        assert_eq!(hit.header.frame_count, 3);
        assert!(try_key(probe, 12344, "1.5.0", "pov_mask.asvr").is_none());
    }
}
