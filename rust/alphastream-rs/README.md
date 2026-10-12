# alphastream

An open implementation of `libalphastream`: reads encrypted ASVR files and plain ASVP files,
rasterizes polystream frames to alpha masks, and exposes both a Rust API and a C ABI
(`CV_*` functions, header in [`include/alphastream.h`](include/alphastream.h)).

See the [repository](https://github.com/daniel5gh/open-alphastream) for the file format
documentation and the Python reference implementation.

## Rust

```rust
use alphastream::api::{AlphaStreamProcessorBuilder, ProcessingMode};
```

See `src/bin/demo.rs` in the repository for a complete example.

## Features

| feature    | default | description |
|------------|---------|-------------|
| `http`     | yes     | Fetch ASVR/ASVP over HTTP(S) (rustls, no OpenSSL). |
| `testlib`  | no      | `alphastream::testlib` helpers that generate synthetic ASVR/ASVP files. |
| `keysearch`| no      | `alphastream::keysearch`: recover scene id / version for an encrypted file. |
| `cli`      | no      | Builds the `demo` and `keysearch` binaries (implies `http`, `keysearch`). |

## C library

The crate builds a `cdylib` and a `staticlib` named `alphastream`
(`libalphastream.so`, `alphastream.dll`, `libalphastream.dylib`, `libalphastream.a`).

```sh
cargo build --release
```

The header is generated with [cbindgen](https://github.com/mozilla/cbindgen):

```sh
cbindgen --config cbindgen.toml --crate alphastream --output include/alphastream.h
```

## License

MIT, see [LICENSE](LICENSE).
