use std::fs::metadata;
use libalphastream::api::{AlphaStreamProcessorBuilder, ProcessingMode};

use std::process;

fn main() {
    // Use std::env for argument parsing
    let mut args = std::env::args().skip(1);
    let asvr_path = match args.next() {
        Some(val) => val,
        None => {
            eprintln!("Missing required argument: asvr_path");
            print_usage_and_exit();
        }
    };
    let version = match args.next() {
        Some(val) => val,
        None => {
            eprintln!("Missing required argument: version");
            print_usage_and_exit();
        }
    };
    let scene_id = match args.next() {
        Some(val) => val,
        None => {
            eprintln!("Missing required argument: scene_id");
            print_usage_and_exit();
        }
    };

    let mut override_filename_for_decrypt: Option<String> = None;
    while let Some(arg) = args.next() {
        if arg == "--override-filename-for-decrypt" {
            match args.next() {
                Some(val) => override_filename_for_decrypt = Some(val),
                None => {
                    eprintln!("Expected a filename after --override-filename-for-decrypt");
                    print_usage_and_exit();
                }
            }
        } else {
            eprintln!("Unknown argument: {}", arg);
            print_usage_and_exit();
        }
    }

    // Try to open the file at asvr_path
    let file_size = match metadata(&asvr_path) {
        Ok(meta) => meta.len(),
        Err(e) => {
            eprintln!("Failed to get file metadata: {}", e);
            process::exit(1);
        }
    };

    // Parse as ASVR using AlphaStreamProcessorBuilder
    let scene_id_num = match scene_id.parse::<u32>() {
        Ok(num) => num,
        Err(_) => {
            eprintln!("scene_id must be a valid u32");
            process::exit(1);
        }
    };
    let version_bytes = version.as_bytes();
    let base_url = if let Some(ref override_name) = override_filename_for_decrypt {
        override_name.as_str()
    } else {
        std::path::Path::new(&asvr_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
    };
    let base_url_bytes = base_url.as_bytes();

    let builder = AlphaStreamProcessorBuilder::new()
        .processing_mode(ProcessingMode::Bitmap)
        .prefetch_window(1000);
    let processor = match builder.build_asvr(&asvr_path, scene_id_num, version_bytes, base_url_bytes, 512, 512) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not create AlphaStreamProcessor: {}", e);
            process::exit(1);
        }
    };

    // Get metadata
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let meta = match rt.block_on(processor.metadata()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("No metadata available: {}", e);
            process::exit(1);
        }
    };

    println!("File size: {} bytes", file_size);
    println!();
    println!("Frame count: {}", meta.frame_count);
    println!("Compressed sizes table: {} bytes", meta.compressed_sizes_size);

    // Iterate over all frames with progress indicator
    use std::time::Instant;
    println!("Decoding all frames...");
    let total = meta.frame_count;
    let mut last_percent = 0;
    let start = Instant::now();
    for frame_idx in 0..total {
        // Request and wait until the frame is actually available, with timeout
        let frame_start = std::time::Instant::now();
        loop {
            let _ = rt.block_on(processor.request_frame(frame_idx));
            let got = rt.block_on(processor.get_frame(frame_idx as usize, 512, 512));
            if got.is_some() {
                if frame_start.elapsed().as_millis() > 0 {
                    println!("[debug] Frame {} decoded in {:.3} ms", frame_idx + 1, frame_start.elapsed().as_millis());
                }
                break;
            } else {
                if frame_start.elapsed().as_millis() > 500 {
                    eprintln!("Error: Timeout waiting for frame {} (>{} ms)", frame_idx, 500);
                    process::exit(1);
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        let percent = ((frame_idx + 1) * 100 / total).min(100);
        if percent != last_percent && (percent % 5 == 0 || percent == 100) {
            print!("\rProgress: {:3}% ({}/{} frames)", percent, frame_idx + 1, total);
            use std::io::Write;
            std::io::stdout().flush().unwrap();
            last_percent = percent;
        }
    }
    let elapsed = start.elapsed();
    println!("\nDone decoding all frames.");
    println!("Decoded {} frames in {:.3} seconds ({:.2} ms/frame)",
        total,
        elapsed.as_secs_f64(),
        if total > 0 { elapsed.as_secs_f64() * 1000.0 / total as f64 } else { 0.0 }
    );
}

fn print_usage_and_exit() -> ! {
    eprintln!("Usage: demo <asvr_path> <version> <scene_id> [--override-filename-for-decrypt <filename>]");
    process::exit(1);
}
