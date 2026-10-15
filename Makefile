export CARGO_TARGET_DIR ?= $(CURDIR)/target
CARGO ?= $(HOME)/.cargo/bin/cargo
NODE ?= node
ISO ?= private/source/ATV Offroad Fury 2 (USA) (v2.00).iso
ELF ?= private/extracted/SCUS_972.11
BIND ?= 127.0.0.1
PORT ?= 4173
# Extra runner flags, e.g. BROWSER_ARGS='--only test_race.mjs --verbose'. See research/BROWSER_TESTS.md.
BROWSER_ARGS ?=

.PHONY: verify verify-full inventory import check web relay serve audit code-map native-analyze browser-check browser-smoke browser-tests browser-native

verify:
	python3 tools/verify_setup.py

verify-full:
	python3 tools/verify_setup.py --full

inventory:
	python3 tools/verify_setup.py --inventory

import:
	python3 tools/import_disc.py "private/source/ATV Offroad Fury 2 (USA) (v2.00).zip"

check:
	python3 tools/check_secrets.py
	python3 tools/verify_repository.py
	python3 -m unittest discover -s tools -p 'test_security.py'
	$(CARGO) fmt --all -- --check
	$(CARGO) test --locked --workspace
	$(CARGO) clippy --locked --workspace --all-targets -- -D warnings
	$(NODE) --test --test-timeout=120000 web/water.test.mjs web/scenery-materials.test.mjs web/ambient-effects.test.mjs web/ambient-streams.test.mjs web/input.test.mjs web/content.test.mjs web/touch.test.mjs web/bindings.test.mjs web/music.test.mjs web/audio.test.mjs web/diagnostics.test.mjs web/multiplayer.test.mjs web/motorcycle-presentation.test.mjs web/minimap.test.mjs web/frontend.test.mjs web/launch.test.mjs web/install.test.mjs web/render-support.test.mjs web/speed-rush.test.mjs web/blood.test.mjs web/keyboard-help.test.mjs web/analytics.test.mjs tools/browser/manifest.test.mjs
	python3 -m py_compile tools/*.py
	python3 -m unittest discover -s tools -p 'test_worker_build_cache.py'
	python3 -m unittest discover -s tools -p 'test_devlog.py'
	python3 -m unittest discover -s tools -p 'test_ci_changes.py'
	python3 -m unittest discover -s tools -p 'test_release_workflow.py'

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

# Browser groups start their own local server on a free port; build web/dist first.
browser-check:
	$(NODE) tools/run_browser_tests.mjs --only test_browser.mjs $(BROWSER_ARGS)

browser-smoke:
	$(NODE) tools/run_browser_tests.mjs --group content-free $(BROWSER_ARGS)

browser-tests:
	$(NODE) tools/run_browser_tests.mjs --group content-free,content $(BROWSER_ARGS)

browser-native:
	$(NODE) tools/run_browser_tests.mjs --group native $(BROWSER_ARGS)

