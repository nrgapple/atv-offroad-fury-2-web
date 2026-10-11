# Third-party code notices

## PL_MPEG MPEG tables

`crates/atv-core/src/ipu_tables.rs` adapts the MPEG AC VLC, zigzag and default intra quantization tables from [PL_MPEG](https://github.com/phoboslab/pl_mpeg), revision `c871f2be022ece7ef4f64230b4fb8e1fb9eb6023`. The source identifies its license as SPDX MIT and its author as Dominic Szablewski. Its README and root repository inventory were read; no separate AGENTS.md or license file was present. Decoder implementation is independently written; the adapted table representations remain attributed here.

Copyright (c) Dominic Szablewski

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

## Research and verification only

FFmpeg 8's IPU decoder was read as a format reference and the installed executable used as an independent pixel oracle. Its LGPL 2.1-or-later notice and license were read. No FFmpeg implementation or source table was copied into this project. Temporary reference material and original game data stay ignored/private.
