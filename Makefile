.PHONY: verify verify-full inventory import

verify:
	python3 tools/verify_setup.py

verify-full:
	python3 tools/verify_setup.py --full

inventory:
	python3 tools/verify_setup.py --inventory

import:
	python3 tools/import_disc.py "private/source/ATV Offroad Fury 2 (USA) (v2.00).zip"

