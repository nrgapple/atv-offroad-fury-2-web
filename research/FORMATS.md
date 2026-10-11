# Recovered format notes — USA v2.00

These observations apply to the preserved `SCUS_972.11` disc build identified in [HANDOFF](../HANDOFF.md). Readers were written independently from this build's files. Unknown layouts fail explicitly. This document contains structural notes; original file contents and full inventories remain under ignored `private/`.

## Resource archives: RES and LSI

The native audit parsed all 368 RES and 119 LSI files: 487 archives containing 58,598 member entries. There are 486 compressed archives and one raw archive, `/AUDIO/AUDIO.RES`. All 91,522 compressed blocks inflated to the expected length. Repeated entries across archives are not deduplicated, so the member count is not a unique-asset count.

All integers below are little-endian. The observed header is:

| Byte offset | Field |
| --- | --- |
| 0 | `u32` version, `3` |
| 4 | `u32` relative directory offset; add `8` to obtain its file offset |
| 8 | `u32` flags: `0x340` compressed, `0x100` raw |
| 12 | `u32` zero |
| 16 | Compressed block-table byte count for compressed archives |
| 20 | Start of compressed `u16` block-size table, when present |

The compressed table ends immediately before the directory, includes a final zero sentinel, and its size equals `directory_offset - 20`. Each preceding entry gives the stored block extent minus one. The directory starts with a `u32` member count and a `u32` directory-record byte length. Each record contains a `u32` name byte length, exactly that many non-NUL name bytes, a `u32` logical payload offset, and a `u32` member byte length. Names use backslashes on disc; tools normalize separators and reject traversal paths.

The physical payload follows the directory records. For raw archives it is the member-addressable byte stream. Compressed archives contain independently compressed zlib blocks, each producing exactly `0x6000` (24,576) logical bytes. Member offsets address the concatenated logical blocks. The reader inflates only intersecting blocks for a selected member and applies a 128 MiB member extraction limit. It validates directory bounds, complete physical block coverage, logical member bounds, and per-block output length. The exact meaning of bytes trailing some zlib streams inside their stored extent remains unverified.

The audit establishes container layout and decompression coverage; it does not interpret every member's format or prove a correct game resource lifecycle.

## VAG audio variants

All 192 standalone VAG files decoded (162 mono and 30 stereo), as did 798 VAG entries inside resource archives. The accepted 48-byte header has either `VAGp` magic or twelve leading zero bytes. Bytes 12–15 hold a big-endian ADPCM byte count per channel; bytes 16–19 hold a big-endian sample rate.

Observed mono extent is `48 + byte_count`, with ADPCM following the header. Observed stereo extent is `2016 + 2 * byte_count`, with zero padding through offset 2016 (`0x7e0`) and alternating 16-byte left/right blocks thereafter. The first pair contains silent initialization blocks; the decoder preserves them and maintains independent channel histories. This stereo interpretation is supported by the exact file extents, channel structure, and decoded data, but has not been compared with a recording of original playback.

Each 16-byte PS ADPCM block produces 28 signed samples. The first byte selects predictor and shift; the second carries end/loop flags. Sample nibbles are low-first. Implemented predictors are `(0,0)`, `(60,0)`, `(115,-52)`, `(98,-55)`, and `(122,-60)`, with rounded predictor accumulation and clipped 16-bit history. The decoder stops after the first end-marked block and reports loop points as metadata. Preview WAVs contain one pass; the browser does not reproduce runtime looping, pitch curves, spatialization, SPU2 interpolation, reverb, or scheduling.

### Fidelity check and unresolved differences

A private comparison of a complete stereo track against FFmpeg produced 8,282,148 Rust samples per channel versus 8,282,176 FFmpeg samples per channel. Over the compared extent, maximum absolute differences were 70 PCM units left and 68 right, with differences in most samples. The 28-sample length difference is one ADPCM block. Predictor rounding/history conventions and terminal-block handling differ between the implementations. **This is not a bit-exact validation.** The decoder's original-console fidelity still needs an original execution/reference comparison. Results are in `private/reports/audio-oracle.json`.

## PST texture previews

The loading-archive audit decoded all 934 PST entries. The reader also validates mip chains and decodes all 13 textures in the ATV01 material archive. It returns the largest stored level; the renderer uses five base textures selected by original PSM material bindings. This does not establish all material passes or original GS rendering state.

All header fields are little-endian `u32`:

| Byte offset | Field |
| --- | --- |
| 0 | Version `1` |
| 4 | Indexed format: `0` for 4-bit, `1` for 8-bit |
| 8 | Level count |
| 12 | RGBA palette, 64 bytes for 4-bit or 1024 bytes for 8-bit |
| After palette, repeated per level | Height, width, zero, zero |
| After each dimensions/flags record | Packed pixel indices for that level |

Rectangular loading-bar assets establish **height before width**; square images alone would not distinguish them. Four-bit pixels use the low nibble first and direct palette indices. Eight-bit palette lookup exchanges index bits 3 and 4, matching the observed CSM1 ordering. The current reader copies stored RGBA values directly. Final GS alpha/blending interpretation is not established by the preview. Every level and the complete file extent are validated; unknown versions, formats, and flags are rejected. Original mip selection, dirt passes, and dynamic number-plate material binding remain unresolved.

## ELF reconnaissance

The exact executable is ELF32 little-endian MIPS, entry `0x00100008`, with no named symbols. The Rust tool validates load-segment extents and scans for direct JAL instruction patterns targeting loaded memory. It reports 5,599 candidate targets with call sites. The executable's load segment contains mixed code and data; bit patterns in data can look like calls. Indirect calls are not covered. Candidates are not verified functions, names, or boundaries and must not be used as a recovered function map without further analysis.

REA currently rejects this executable's machine-specific MIPS architecture. Direct Ghidra analysis with the R5900 extension works and verifies every original load byte; see [native analysis evidence](REA.md). Decompiled output still requires instruction review, particularly around VU operations, and does not establish runtime behavior by itself.

## Validation boundaries

Synthetic Rust tests exercise boundary-crossing archive extraction, corrupt/truncated input, path rejection, ADPCM sign/predictor/loop/stereo behavior, and indexed texture ordering/dimensions. Browser checks exercise the compiled WASM readers and the file-selection path with both synthetic fixtures and selected original content. Full private audits provide exact-disc coverage for the formats listed above. None of these checks executes the original game or proves recovered physics, animations, rendering, timing, or audio mixing.
