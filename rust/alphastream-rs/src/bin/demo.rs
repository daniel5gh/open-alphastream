use std::fs::File;
use std::fs::metadata;
use libalphastream::formats::ASFormat;
use libalphastream::formats::{ASVRFormat};

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
    match File::open(&asvr_path) {
        Ok(mut file) => {
            // File size
            let file_size = match metadata(&asvr_path) {
                Ok(meta) => meta.len(),
                Err(e) => {
                    eprintln!("Failed to get file metadata: {}", e);
                    process::exit(1);
                }
            };

            // Parse as ASVR
            let scene_id_num = match scene_id.parse::<u32>() {
                Ok(num) => num,
                Err(_) => {
                    eprintln!("scene_id must be a valid u32");
                    process::exit(1);
                }
            };
            let version_bytes = version.as_bytes();
            // Only pass the filename, not the full path, as base_url
            let base_url = if let Some(ref override_name) = override_filename_for_decrypt {
                override_name.as_str()
            } else {
                std::path::Path::new(&asvr_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
            };
            let base_url_bytes = base_url.as_bytes();

            match ASVRFormat::new(&mut file, scene_id_num, version_bytes, base_url_bytes) {
                Ok(mut asvr) => {
                    println!("File size: {} bytes", file_size);
                    println!();
                    match asvr.metadata() {
                        Ok(meta) => {
                            println!("Frame count: {}", meta.frame_count);
                            println!("Compressed sizes table: {} bytes", meta.compressed_sizes_size);
                        }
                        Err(e) => {
                            println!("No metadata available: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Could not parse as ASVR: {}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open file '{}': {}", asvr_path, e);
            process::exit(1);
        }
    }
}

fn print_usage_and_exit() -> ! {
    eprintln!("Usage: demo <asvr_path> <version> <scene_id> [--override-filename-for-decrypt <filename>]");
    process::exit(1);
}
