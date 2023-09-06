set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

VENV_LOCATION := "./venv"

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
	@python -m pip install . --use-feature=in-tree-build
	python -m pip install maturin
	maturin develop --extras=typing,test
	cargo testall
	mypy .\python\turnip_text --strict
	pytest tests
