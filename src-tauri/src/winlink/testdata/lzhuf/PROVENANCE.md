# lzhuf conformance vectors

These files anchor the native lzhuf codec (`src-tauri/src/winlink/lzhuf.rs`) to
the exact byte stream the Winlink network expects.

| File | What it is |
|---|---|
| `gettysburg.txt` | Lincoln's Gettysburg Address (public domain). Small, ordinary text. |
| `pi.txt` | The first 100,000 digits of pi (public domain). Large enough that decoding it rebuilds the adaptive Huffman tree at least once, exercising that path. |
| `*.lzh` | Each text compressed in the FBB B2 lzhuf format. |

The `.lzh` files were produced by the `la5nta/wl2k-go` lzhuf implementation
(MIT-licensed Go port of the JNOS 2 `lzhuf.c`), which is verified against the
real Winlink CMS. They are the reference our independent Rust implementation is
checked against: `decompress(x.lzh) == x.txt`, and `compress(x.txt) == x.lzh`
byte-for-byte. No Go code ships in tuxlink — these are data only.

Reference: https://github.com/la5nta/wl2k-go (`lzhuf/testdata/`).
