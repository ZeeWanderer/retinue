REBAR3 ?= rebar3
CARGO ?= cargo
REBAR_CACHE_DIR ?= $(CURDIR)/.cache/rebar3
CARGO_TARGET_DIR ?= $(CURDIR)/_build/cargo
SCCACHE ?= $(shell command -v sccache 2>/dev/null)
SCCACHE_BASEDIRS ?= $(CURDIR)
SCCACHE_DIR ?= $(CURDIR)/.cache/sccache
SCCACHE_SERVER_UDS ?= $(CURDIR)/.cache/sccache.sock
LLVM_ROOT ?= $(shell command -v clang >/dev/null 2>&1 && dirname "$$(dirname "$$(realpath "$$(command -v clang)")")")
NINJA ?= $(shell command -v ninja 2>/dev/null)

ifneq ($(strip $(LLVM_ROOT)),)
export LLVM_ROOT
endif

ifeq ($(origin CC),default)
CC := $(if $(LLVM_ROOT),$(LLVM_ROOT)/bin/clang,clang)
endif
ifeq ($(origin CXX),default)
CXX := $(if $(LLVM_ROOT),$(LLVM_ROOT)/bin/clang++,clang++)
endif

COMPANION_MANIFEST := apps/wfcompanion/Cargo.toml
POE2_COMPANION_MANIFEST := apps/poe2companion/Cargo.toml
VERSION := $(strip $(shell cat VERSION))
PLATFORM := $(shell uname -s | tr '[:upper:]' '[:lower:]')-$(shell uname -m)
PREVIEW_MEDIA ?= all
PREVIEW_SETS ?= companion reference
PREVIEW_SCENES ?= all
PREVIEW_RESOLUTIONS ?= 1920x1080 2560x1440
PREVIEW_DEPS = $(if $(findstring companion,$(PREVIEW_SETS)),dev-companion)
ICON_OPTICS_SCALE ?= 1.25
ICON_OPTICS_OUTPUT ?= $(CURDIR)/previews/optics
ICON_OPTICS_CONFIG ?= $(CURDIR)/tools/icon-optics/foundry.json

export REBAR3 REBAR_CACHE_DIR
export CARGO CARGO_TARGET_DIR
export SCCACHE_BASEDIRS
export SCCACHE_DIR
export SCCACHE_SERVER_UDS
export CC CXX

ifneq ($(strip $(SCCACHE)),)
RUSTC_WRAPPER ?= $(SCCACHE)
CMAKE_C_COMPILER_LAUNCHER ?= $(SCCACHE)
CMAKE_CXX_COMPILER_LAUNCHER ?= $(SCCACHE)
export RUSTC_WRAPPER CMAKE_C_COMPILER_LAUNCHER CMAKE_CXX_COMPILER_LAUNCHER
endif

.PHONY: all build dev prod erlang cli daemon mcp companion inspect dev-inspect prod-inspect gui gui-dev gui-prod sccache-setup \
	gui-configure-dev gui-configure-prod \
	dev-erlang prod-erlang dev-companion prod-companion links \
	debug-bridge native-bridges previews icon-optics aleca-layout-setup fix-executables \
	native-compile-commands test test-erlang test-companion test-poe2 test-gui test-release test-staging test-build check fmt-check xref package clean

# These phases share build trees; their internal compiler jobs remain parallel.
.NOTPARALLEL: check test

all: dev
build: dev prod
	+$(MAKE) native-compile-commands

sccache-setup:
ifneq ($(strip $(SCCACHE)),)
	@mkdir -p "$(SCCACHE_DIR)" "$(dir $(SCCACHE_SERVER_UDS))"
endif

dev: dev-erlang dev-companion gui-dev links

prod: prod-erlang prod-companion gui-prod links

erlang cli daemon mcp: dev-erlang

companion: dev-companion

inspect: dev-inspect

dev-inspect: sccache-setup
	+$(CARGO) build --locked -p retinue-inspect

prod-inspect: sccache-setup
	+$(CARGO) build --locked --release -p retinue-inspect

gui: gui-dev

gui-configure-dev: sccache-setup
	test -n "$(NINJA)"
	+cmake --fresh --preset gui-dev --log-level=WARNING -DCMAKE_MAKE_PROGRAM="$(NINJA)"

gui-dev: gui-configure-dev
	+cmake --build --preset gui-dev
	./scripts/stage-gui dev

gui-configure-prod: sccache-setup
	test -n "$(NINJA)"
	+cmake --fresh --preset gui-prod --log-level=WARNING -DCMAKE_MAKE_PROGRAM="$(NINJA)"

gui-prod: gui-configure-prod
	+cmake --build --preset gui-prod
	./scripts/stage-gui prod

dev-erlang:
	$(REBAR3) escriptize
	rm -rf _build/default/rel/retinue_daemon
	$(REBAR3) release
	./scripts/stage-erlang dev

prod-erlang:
	$(REBAR3) as prod escriptize
	rm -rf _build/prod/rel/retinue_daemon
	$(REBAR3) as prod release
	./scripts/stage-erlang prod

dev-companion: sccache-setup
	+./scripts/build-companion dev

prod-companion: sccache-setup
	+./scripts/build-companion prod

links:
	@for name in wfdaemon wfdaemond; do \
		if test -L "$$name"; then \
			case "$$(readlink "$$name")" in dev/bin/wfdaemon|prod/bin/wfdaemon) rm "$$name" ;; esac; \
		fi; \
	done
	ln -sfn dev/bin/retinue retinued
	ln -sfn dev/bin/wfcli wfclid
	ln -sfn dev/bin/poe2cli poe2clid
	ln -sfn dev/bin/arccli arcclid
	ln -sfn dev/bin/retinue-daemon retinue-daemond
	ln -sfn dev/bin/retinue-inspect retinue-inspectd
	ln -sfn dev/bin/wfcompanion wfcompaniond
	ln -sfn dev/bin/poe2companion poe2companiond
	ln -sfn dev/bin/arccompanion arccompaniond
	ln -sfn dev/bin/wfinspect wfinspectd
	ln -sfn dev/bin/wf-luau-decompiler wf-luau-decompilerd
	ln -sfn dev/bin/wfgui wfguid
	ln -sfn prod/bin/retinue retinue
	ln -sfn prod/bin/wfcli wfcli
	ln -sfn prod/bin/poe2cli poe2cli
	ln -sfn prod/bin/arccli arccli
	ln -sfn prod/bin/retinue-daemon retinue-daemon
	ln -sfn prod/bin/retinue-inspect retinue-inspect
	ln -sfn prod/bin/wfcompanion wfcompanion
	ln -sfn prod/bin/poe2companion poe2companion
	ln -sfn prod/bin/arccompanion arccompanion
	ln -sfn prod/bin/wfinspect wfinspect
	ln -sfn prod/bin/wf-luau-decompiler wf-luau-decompiler
	ln -sfn prod/bin/wfgui wfgui

debug-bridge: sccache-setup
	./scripts/build-debug-bridge

native-bridges: debug-bridge

native-compile-commands: sccache-setup
	./scripts/native-compile-commands

previews: $(PREVIEW_DEPS)
	PREVIEW_MEDIA='$(PREVIEW_MEDIA)' \
	PREVIEW_SETS='$(PREVIEW_SETS)' \
	PREVIEW_SCENES='$(PREVIEW_SCENES)' \
	PREVIEW_RESOLUTIONS='$(PREVIEW_RESOLUTIONS)' \
	./scripts/generate-previews

icon-optics: gui-configure-dev
	+cmake --build --preset gui-dev --target wfgui_icon_optics
	QT_QPA_PLATFORM=offscreen _build/cmake/gui-dev/apps/wfgui/wfgui-icon-optics \
		--config "$(ICON_OPTICS_CONFIG)" --ui-scale "$(ICON_OPTICS_SCALE)" \
		--output-dir "$(ICON_OPTICS_OUTPUT)"

aleca-layout-setup:
	./scripts/setup-aleca-layout

fix-executables:
	bash ./scripts/fix-executables

test: test-erlang test-companion test-gui test-staging test-build test-poe2

test-erlang:
	./scripts/test-quiet eunit
	./scripts/test-quiet ct

test-companion: native-bridges
	+$(CARGO) test --locked --quiet -p retinue-client -p retinue-native -p retinue-warframe -p retinue-inspect -p arccompanion
	+$(CARGO) test --locked --quiet --manifest-path $(COMPANION_MANIFEST)
	+$(CARGO) test --locked --quiet --manifest-path $(POE2_COMPANION_MANIFEST)

test-poe2: dev-erlang dev-companion
	python3 ./scripts/tests/poe2.py --stage dev

test-gui:
	+./scripts/test-quiet gui

test-staging:
	./scripts/test-storage-migration
	./scripts/test-quiet staging

test-build:
	+./scripts/test-quiet build

test-release: prod
	./scripts/test-quiet release

fmt-check:
	$(CARGO) fmt --all --check

xref:
	$(REBAR3) xref

check: fmt-check xref test dev prod test-release

package: prod
	mkdir -p releases
	rm -f releases/retinue-$(VERSION)-$(PLATFORM).tar.gz releases/retinue-$(VERSION)-$(PLATFORM).zip
	tar -C prod -czf releases/retinue-$(VERSION)-$(PLATFORM).tar.gz .
	cd prod && zip -qr ../releases/retinue-$(VERSION)-$(PLATFORM).zip .

clean:
	rm -rf _build dev prod
