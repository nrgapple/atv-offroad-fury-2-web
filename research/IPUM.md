# Original IPUM image decoding

`ipu::decode_ipum` decodes the observed single-frame, flags-zero IPUM images into YUV420 and RGBA in native Rust/WebAssembly. It validates declared extent, dimensions, bitstream bounds, macroblock syntax, coefficient runs, quantizers, padding and both terminal markers. Unsupported coding flags and zero-dimension placeholders fail explicitly. Largest actual mip levels have positive dimensions.

The container starts with `ipum`, u32 bytes excluding the first eight, u16 width/height and u32 frame count. A coding-flags byte precedes MPEG-2 intra macroblocks. Frame and sequence terminators are `00 00 01 b0` and `00 00 01 b1`. The observed streams use standard zigzag, default intra quantization, zero DC-precision shift and the MPEG-1 AC VLC table with MPEG-2 escape/dequantization/mismatch behavior.

All 596 positive-dimension images embedded in ENV01 TR2 and SKY01 CUB decode with exact end markers: 340 terrain mips and 256 sky mips. Inventory and native audit stay private. Ten independent FFmpeg comparisons cover both containers and their dimension variants. Raw YUV420 differed by at most one byte value, with worst mean absolute difference 0.01823. FFmpeg's parser additionally reports the terminal sequence code as a second invalid packet; its first decoded frame is the comparison image.

The separable floating-point inverse DCT is intentionally documented as approximate relative to PS2 integer hardware. RGBA currently uses nearest-neighbor chroma and a bounded integer BT.601 conversion; bit-identical PS2 color conversion is not yet established. RGBA alpha is opaque for these decoded image planes. This is actual decoded original imagery, not generated replacement art.

MPEG VLC/quantization tables are adapted from MIT-licensed PL_MPEG. Attribution, pinned revision and license are in [THIRD_PARTY.md](THIRD_PARTY.md). FFmpeg 8's LGPL source was consulted for format interpretation and the installed executable used as a verification oracle; its implementation and tables were not incorporated.

Synthetic tests cover a neutral MPEG intra macroblock, all truncated prefixes, coding flags, dimensions and malformed terminal codes. The decoder does not yet support multi-frame movie timing or other IPU coding flags.

Stadium `stbscreen.atm` pixel-format-2 frames reuse this single-image decoder.
The ATM reader now accepts their length-prefixed compressed PST mip payloads
and checks every frame's container extent before decoding a selected frame.
The source PSM texture clock selects original LED advertisement frames; this
does not add another IPU coding mode or multi-frame MPEG movie decoding. See
[STADIUM_MATERIALS.md](STADIUM_MATERIALS.md) for the separate material checkpoint.
