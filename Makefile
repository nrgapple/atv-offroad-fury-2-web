export CARGO_TARGET_DIR ?= $(CURDIR)/target
CARGO ?= $(HOME)/.cargo/bin/cargo
NODE ?= node
ISO ?= private/source/ATV Offroad Fury 2 (USA) (v2.00).iso
ELF ?= private/extracted/SCUS_972.11
BIND ?= 127.0.0.1
PORT ?= 4173
ATV_RELEASE ?= $(HOME)/.local/bin/atv-release
RELEASE_ARGS ?=

.PHONY: verify verify-full inventory import check web relay serve audit code-map native-analyze browser-check release release-status

release:
	@echo "Experimental release queue disabled. Use the GitHub Actions deployment and checkpoint R2 flow in research/CI_CHECKPOINTS.md."
	@exit 1

release-status:
	$(ATV_RELEASE) status

verify:
	python3 tools/verify_setup.py

verify-full:
	python3 tools/verify_setup.py --full

inventory:
	python3 tools/verify_setup.py --inventory

import:
	python3 tools/import_disc.py "private/source/ATV Offroad Fury 2 (USA) (v2.00).zip"

check:
	$(CARGO) fmt --all -- --check
	$(CARGO) test --locked --workspace
	$(CARGO) clippy --locked --workspace --all-targets -- -D warnings
	$(NODE) --test web/input.test.mjs web/content.test.mjs web/touch.test.mjs web/bindings.test.mjs web/music.test.mjs web/audio.test.mjs web/diagnostics.test.mjs web/multiplayer.test.mjs web/motorcycle-presentation.test.mjs web/minimap.test.mjs web/frontend.test.mjs
	python3 -m py_compile tools/*.py

web:
	python3 tools/build_web.py

relay:
	cd crates/atv-relay && PATH="$(dir $(CARGO)):$(PATH)" $(HOME)/.cargo/bin/worker-build --release

serve:
	python3 tools/serve_web.py --bind $(BIND) --port $(PORT)

audit:
	$(CARGO) run --locked --release -p atv-tools -- audit-iso "$(ISO)" private/reports/content-audit.json

code-map:
	$(CARGO) run --locked --release -p atv-tools -- elf "$(ELF)" private/reports/code-candidates.json

native-analyze:
	python3 tools/analyze_ps2.py

browser-check:
	$(NODE) tools/test_browser.mjs

