# REA integration — 2026-10-11

The user explicitly selected [Reverse Engineer Anything (REA)](https://github.com/morluto/rea)
for executable research. Start with its public CLI; consider MCP when a compatible
native session can retain useful analysis between queries. Both adapters use the
same provider layer, so MCP cannot resolve a processor compatibility failure.

## Actual probe

- Acquired and ran published `rea-agents@6.4.0` using existing Node 24.19.0.
  The system Node 25 is outside REA's documented supported major versions.
- Read the upstream MIT license, repository instructions, and
  [REA workflow skill](https://github.com/morluto/rea/blob/main/.agents/skills/reverse-engineer-anything/SKILL.md).
- Kept an ignored, unmodified upstream reference at `references/rea`, revision
  `12ee785e5e61132898adecf9424ce6706a2e754a`. Repository main and the published npm
  package have different capabilities; main's documentation is not a release test.
- Asked the CLI to analyze the exact boot ELF's entry address, `0x00100008`:

  ```sh
  npx -y rea-agents@6.4.0 function "$PWD/private/extracted/SCUS_972.11" \
    0x00100008 --provider ghidra --json
  ```

  Use a supported Node on PATH. On this host, the already installed Node 24 binary
  is in `/Users/nrgapple/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin`.

The actual result is `capability_unavailable` / `unsupported_provider`, with
`architecture_unsupported`: machine-specific MIPS needs a separately supported
processor profile. This ELF declares `e_flags=0x20924000`. Independently, Ghidra
was not installed/configured at the time of the REA probe. Existing Java 26 was accepted by REA's prerequisite
check. Installing Ghidra alone would not remove the architecture rejection.

Raw output is retained in `private/reports/rea-entry-attempt.json`; probe identity
and report hashes are in `private/reports/rea-session.json`. These are diagnostic
results, not successful REA function Evidence. No pseudocode was produced, no
native session opened, and no close operation was required. The executable hash
is unchanged. No global MCP registration or approval settings were changed.

## Next compatibility gate

1. Validate a Ghidra version and the
   [Emotion Engine: Reloaded extension](https://github.com/chaoticgd/ghidra-emotionengine-reloaded)
   together on this exact ELF. Its advertised EE-specific disassembly/decompilation
   makes it a candidate. The direct-engine validation below now establishes import
   and selected function analysis, with remaining instruction-semantic limitations.
2. Establish an explicit R5900 admission/profile in REA (upstream support or a
   separately maintained, tested adapter). Its documented generic MIPS32r2 and PSP
   profiles do not establish PS2 compatibility. Do not rewrite ELF flags to bypass
   admission or treat a processor override alone as a supported profile.
3. Compare decoded ordinary, delay-slot, MMI, and VU-macro instructions against
   original bytes and an independent PS2 decoder. Preserve source hash, language,
   compiler specification, extension/tool versions, and unresolved semantics.
4. Obtain a small resource/codec routine dossier, validate it against observed
   resource data and tests, and retain complete private Evidence. Expand only after
   this gate; decompiler pseudocode is neither original source nor verified Rust.

Until then, Rust format analysis can proceed independently. The existing 5,599
JAL destination candidates are our own byte-pattern reconnaissance, not REA
functions. No REA reference implementation is linked into the game or browser.

Sources: [REA MIPS admission](https://github.com/morluto/rea/blob/main/docs/ghidra-mips.md),
[Ghidra provider](https://github.com/morluto/rea/blob/main/docs/ghidra-provider.md),
[connection recovery](https://github.com/morluto/rea/blob/main/.agents/skills/reverse-engineer-anything/references/connection-and-recovery.md).

## Direct PS2 engine validation completed later in this session

Installed Ghidra 12.1.4 and the matching Emotion Engine: Reloaded 2.1.38 extension
under ignored SSD-backed `work/toolchains/`, after reading their instructions and
Apache-2.0 licenses. Built Ghidra's ARM64 decompiler locally. Download sizes and
SHA-256 provenance are in `private/reports/ghidra-toolchain.json`. The extension
reference revision is `fcc5deac45d8f5c27c632bfe0db928630939676e`.

The exact ELF imports as `r5900:LE:32:default`, with compiler `default`. Analysis
completed and produced 7,895 function candidates. Every file-backed PT_LOAD byte
was compared with the original ELF and matched. Defined strings include mirrored
address-space copies; counts must not be described as unique original strings.
An analyzer warning at `0x003aa87c` and the loader's conflicting ROM block warning
are retained in the log. VU pseudocode has unresolved intrinsic semantics; it is
not suitable for blind translation.

`tools/AtvPrepare.java` configures the analysis; `tools/AtvRecover.java` exports
private inventory, instruction bytes, and selected decompiler output. The private
database is `work/ghidra/atv2.gpr`. Reuse it without repeating auto-analysis:

```sh
python3 tools/analyze_ps2.py 0x001e3b80 0x00168490
```

The wrapper verifies exact input identity, R5900 selection, fresh export, matching
loaded segments, and unchanged source afterward. It does not execute PS2 code.
These are **direct Ghidra observations, not REA Evidence**. REA's R5900 provider
integration is still unresolved; its rejection is not bypassed by changing headers.

Located the physics loader through exact-build tuning-string cross-references,
then followed its INI lookup and spring/damping helpers. Located terrain loading
through source-file strings and followed its tile/hierarchy readers. These support
the Rust terrain and physics-data readers. Private dossiers and source-byte assembly
are under `private/reports/ghidra/`; physics semantics and uncertainties are in
`private/reports/physics-semantics.json`. No original pseudocode is published.
