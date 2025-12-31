# Open AlphaStream

An open implementation of "libalphastream" as used in not to be named closed source VR player: Python reference (educational) and high‑performance Rust for production. Intended for integration into my not yet released high performance and ultra stable VR media player.

The not to be named closed source VR player uses AlphaStream for alpha-mask rendering in AI passthrough mode, which is a key feature for immersive mixed reality experiences. This project aims to provide an open-source alternative implementation of the AlphaStream functionality, enabling developers and enthusiasts to understand, utilize, and potentially improve upon the original closed-source library.

Key motivation is because the not to be named closed source VR player is so unstable and buggy, getting worse with each release, I am forced to implement my own media player. Since AlphaStream is a key part of the experience, I need to reimplement it as well.

Also created as a reverse‑engineering exercise to maintain skills and as a showcase of capabilities. Targeted completion during Xmas 2025 leave, and I completed just before New Year 2026!

Key docs and code:
- [AGENTS.md](AGENTS.md)
- [FILE_FORMAT.md](docs/FILE_FORMAT.md)
- [alpha_stream_crypto.py](python/alpha_stream_crypto.py)
- [alpha_stream_draw.py](python/alpha_stream_draw.py)
