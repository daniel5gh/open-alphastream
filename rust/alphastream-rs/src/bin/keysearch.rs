//! Find the scene ID and/or version string of an ASVR file by exhaustive key search.
//!
//! Only the first 18 bytes of the file are needed, so a URL costs a single small Range request.

use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use libalphastream::keysearch::{base_name_from_url, find_key_inputs, version_sweep, HEADER_PROBE_LEN};

/// Version strings seen so far in the wild (0.2.3 on early-2024 full-length files).
const DEFAULT_VERSIONS: &[&str] = &["1.5.0", "1.0.0", "0.2.3"];

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage: keysearch <asvr_path_or_url> [options]\n\
         \n\
         Options:\n  \
           --ids A-B | N          scene IDs to try (default 1-200000)\n  \
           --versions v1,v2,...   version strings to try (default {})\n  \
           --sweep-versions X.Y.Z also try every a.b.c with a<=X, b<=Y, c<=Z\n  \
           --base-name NAME       salt file name (default: derived from the path/URL)\n  \
           --threads N            worker threads (default: all cores)",
        DEFAULT_VERSIONS.join(",")
    );
    process::exit(2);
}

fn parse_ids(s: &str) -> Option<(u32, u32)> {
    match s.split_once('-') {
        Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
        None => s.parse().ok().map(|n| (n, n)),
    }
}

fn fetch_probe(source: &str) -> Result<Vec<u8>, String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        rt.block_on(async {
            let resp = reqwest::Client::new()
                .get(source)
                .header("Range", format!("bytes=0-{}", HEADER_PROBE_LEN - 1))
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| e.to_string())?;
            let body = resp.bytes().await.map_err(|e| e.to_string())?;
            Ok(body[..body.len().min(HEADER_PROBE_LEN)].to_vec())
        })
    } else {
        use std::io::Read;
        let mut buf = vec![0u8; HEADER_PROBE_LEN];
        std::fs::File::open(source)
            .and_then(|mut f| f.read_exact(&mut buf))
            .map_err(|e| e.to_string())?;
        Ok(buf)
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let source = args.next().unwrap_or_else(|| print_usage_and_exit());
    let mut ids = (1u32, 200_000u32);
    let mut versions: Vec<String> = DEFAULT_VERSIONS.iter().map(|s| s.to_string()).collect();
    let mut base_name: Option<String> = None;
    let mut threads = thread_count();

    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| print_usage_and_exit());
        match arg.as_str() {
            "--ids" => ids = parse_ids(&value()).unwrap_or_else(|| print_usage_and_exit()),
            "--versions" => versions = value().split(',').map(str::to_string).collect(),
            "--sweep-versions" => {
                let v = value();
                let parts: Vec<u32> = v.split('.').filter_map(|p| p.parse().ok()).collect();
                let [a, b, c] = parts[..] else { print_usage_and_exit() };
                for s in version_sweep(a, b, c) {
                    if !versions.contains(&s) {
                        versions.push(s);
                    }
                }
            }
            "--base-name" => base_name = Some(value()),
            "--threads" => threads = value().parse().unwrap_or_else(|_| print_usage_and_exit()),
            _ => {
                eprintln!("Unknown argument: {arg}");
                print_usage_and_exit();
            }
        }
    }
    if ids.0 > ids.1 {
        print_usage_and_exit();
    }

    let base_name = base_name.unwrap_or_else(|| base_name_from_url(&source.replace('\\', "/")).to_string());
    let probe = fetch_probe(&source).unwrap_or_else(|e| {
        eprintln!("Failed to read first {HEADER_PROBE_LEN} bytes of {source}: {e}");
        process::exit(1);
    });
    if probe.len() < HEADER_PROBE_LEN {
        eprintln!("Source returned only {} bytes", probe.len());
        process::exit(1);
    }

    let total = (ids.1 - ids.0) as u64 + 1;
    let total = total * versions.len() as u64;
    eprintln!(
        "Searching scene IDs {}-{} x {} version(s), base name {base_name:?}, {threads} threads ({total} guesses)",
        ids.0,
        ids.1,
        versions.len()
    );

    let progress = AtomicU64::new(0);
    let started = Instant::now();
    let result = std::thread::scope(|s| {
        let search = s.spawn(|| find_key_inputs(&probe, ids.0..=ids.1, &versions, &base_name, threads, Some(&progress)));
        while !search.is_finished() {
            std::thread::sleep(Duration::from_millis(200));
            report(&progress, total, started, false);
        }
        search.join().unwrap()
    });
    report(&progress, total, started, true);

    match result {
        Some(m) => {
            println!(
                "scene_id={} version={} base_name={} frames={} sizes_len={}",
                m.scene_id, m.version, base_name, m.header.frame_count, m.header.compressed_sizes_size
            );
        }
        None => {
            println!("no match");
            process::exit(1);
        }
    }
}

fn thread_count() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

fn report(progress: &AtomicU64, total: u64, started: Instant, last: bool) {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let elapsed = started.elapsed().as_secs_f64();
    let secs = elapsed as u64;
    if !last && LAST.swap(secs, Ordering::Relaxed) == secs {
        return;
    }
    let done = progress.load(Ordering::Relaxed);
    let rate = done as f64 / elapsed.max(1e-9);
    let eta = (total.saturating_sub(done)) as f64 / rate.max(1e-9);
    eprint!("\r{done}/{total} guesses, {rate:.0}/s, elapsed {elapsed:.0}s, worst-case remaining {eta:.0}s   ");
    if last {
        eprintln!();
    }
}
