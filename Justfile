set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

VENV_LOCATION := "./venv_3_11"

# This is a bit of a hack. In order to get the env vars from the venv to propagate correctly,
# we run the actual _test target inside a per-OS "shebang", which Just executes in a single shell.

[windows]
test:
	#!powershell.exe
	{{VENV_LOCATION}}/Scripts/Activate.ps1
	just _test

[unix]
test:
	#!/usr/bin/env bash
	source {{VENV_LOCATION}}/bin/activate
	just _test

_test:
	# Make sure pip and maturin are installed/up-to-date in the venv
	python -m pip install --upgrade pip
	python -m pip install maturin

	# This is equivalent to maturin develop for our purposes - maturin is faster
	# python -m pip install . --use-feature=in-tree-build
	maturin develop --extras=typing,test

	# Run tests
	cargo testall
	mypy ./python/turnip_text --strict
	pytest tests

[windows]
example:
	#!powershell.exe
	{{VENV_LOCATION}}/Scripts/Activate.ps1
	just _example

[unix]
example:
	#!/usr/bin/env bash
	source {{VENV_LOCATION}}/bin/activate
	just _example

_example:
	maturin develop --extras=typing,test
	python3 ./examples/phdprop.py