# Third-party code notices

## PL_MPEG MPEG tables

`crates/atv-core/src/ipu_tables.rs` adapts the MPEG AC VLC, zigzag and default intra quantization tables from [PL_MPEG](https://github.com/phoboslab/pl_mpeg), revision `c871f2be022ece7ef4f64230b4fb8e1fb9eb6023`. The source identifies its license as SPDX MIT and its author as Dominic Szablewski. Its README and root repository inventory were read; no separate AGENTS.md or license file was present. Decoder implementation is independently written; the adapted table representations remain attributed here.

Copyright (c) Dominic Szablewski

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

## Symphonia (desktop soundtrack decoder)

The desktop builds of `atv-game` depend on the crates.io packages `symphonia` 0.6
(`aac` and `isomp4` features: `symphonia-core`, `symphonia-common`,
`symphonia-metadata`, `symphonia-codec-aac`, `symphonia-format-isomp4`) to decode the
bundled AAC soundtrack songs. They are pure Rust, by Philip Deljanov and the Symphonia
contributors, <https://github.com/pdeljanov/Symphonia>. Their license is the Mozilla
Public License 2.0, a file-level copyleft. The crates are used unmodified, and their
source is available from crates.io and the repository above, which MPL-2.0 section
3.2 requires for binary distribution (the desktop release archives). If a Symphonia
source file is ever modified, the modified file must stay under MPL-2.0 and be
published. The project's own code is not affected. The web build does not include
them; the browser decodes the same songs itself.

## Research and verification only

FFmpeg 8's IPU decoder was read as a format reference and the installed executable used as an independent pixel oracle. Its LGPL 2.1-or-later notice and license were read. No FFmpeg implementation or source table was copied into this project. Temporary reference material and original game data stay ignored/private.
