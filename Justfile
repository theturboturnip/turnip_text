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
	export LD_LIBRARY_PATH="{{VENV_LOCATION}}/lib64${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
	just _test

_test:
	# Make sure pip and maturin are installed/up-to-date in the venv
	python -m pip install --upgrade pip pandoc
	python -m pip install maturin

	# This is equivalent to maturin develop for our purposes - maturin is faster
	# python -m pip install . --use-feature=in-tree-build
	maturin develop --extras=typing,test

	# Run tests
	cargo testall
	# It is useful for type-args to be optional, e.g. XScopeBuilder helpers are optionally typed on their return type
	mypy ./python/turnip_text --strict --disable-error-code=type-arg
	mypy ./python/tests
	pytest ./python/tests/

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
	python -m turnip_text.cli render ./examples/phdprop.ttext -o ./examples/output/ --setup-args "biblatex_bib:phdprop_bib_biblatex.bib" "csl_bib:phdprop_bib_csl.json" --format latex markdown html pandoc-docx

[windows]
upgrade_deps:
	#!powershell.exe
	{{VENV_LOCATION}}/Scripts/Activate.ps1
	python -m pip install --upgrade pip pandoc
	python -m pip install maturin

[unix]
upgrade_deps:
	#!/usr/bin/env bash
	source {{VENV_LOCATION}}/bin/activate
	python -m pip install --upgrade pip pandoc
	python -m pip install maturin

[windows]
regen_typestubs:
	#!powershell.exe
	{{VENV_LOCATION}}/Scripts/Activate.ps1
	python ./python/dev/generate_pandoc_typestub.py ./python/turnip_text/render/pandoc/pandoc_types.pyi

[unix]
regen_typestubs:
	#!/usr/bin/env bash
	source {{VENV_LOCATION}}/bin/activate
	python ./python/dev/generate_pandoc_typestub.py ./python/turnip_text/render/pandoc/pandoc_types.pyi
