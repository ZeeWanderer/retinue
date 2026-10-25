REBAR3 ?= rebar3
.DEFAULT_GOAL := all
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
PREVIEW_SCALE ?= 1
PREVIEW_LAYOUT ?= comparison
PREVIEW_SETS ?= companion reference
PREVIEW_SCENES ?= all
PREVIEW_RESOLUTIONS ?= 1920x1080 2560x1440
PREVIEW_DEPS = $(if $(findstring companion,$(PREVIEW_SETS)),dev-companion)
ICON_OPTICS_SCALE ?= 1.25
ICON_OPTICS_OUTPUT ?= $(CURDIR)/previews/optics
ICON_OPTICS_CONFIG ?= $(CURDIR)/tools/icon-optics/foundry.json
BENCH_SAMPLES ?= 60
BENCH_THREADS ?= 1
BENCH_HZ ?= 0
BENCH_CASES ?=
BENCH_SCALES ?= 1,1.25,1.5
BENCH_OUTPUT ?= $(CURDIR)/research/retinue/benchmarks/overlays/$(shell date -u +%Y%m%dT%H%M%SZ)

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
	gui-qt-dev gui-qt-prod gui-qt-configure-dev gui-qt-configure-prod \
	dev-erlang prod-erlang dev-companion prod-companion links \
	debug-bridge native-bridges previews previews-all previews-wf previews-poe2 icon-optics ui-reference-setup fix-executables \
	native-compile-commands bench-overlays bench-overlay-scheduling test test-erlang test-companion test-poe2 test-gui test-release test-staging test-build check fmt-check xref package clean

# These phases share build trees; their internal compiler jobs remain parallel.
.NOTPARALLEL: check test

gui-dev: sccache-setup
	+$(CARGO) build --locked -p wfgui
	./scripts/stage-gui dev

gui-prod: export WFCLI_CPU_BASELINE := $(or $(WFCLI_CPU_BASELINE),$(if $(filter x86_64,$(shell uname -m)),x86-64-v2))
gui-prod: sccache-setup
	+RUSTFLAGS="$(RUSTFLAGS) $(if $(WFCLI_CPU_BASELINE),-Ctarget-cpu=$(WFCLI_CPU_BASELINE) -Clink-arg=-march=$(WFCLI_CPU_BASELINE))" $(CARGO) build --locked --release -p wfgui
	./scripts/stage-gui prod

test-gui: sccache-setup
	+$(CARGO) test --locked -p retinue-ui --no-default-features --features desktop
	+$(CARGO) test --locked -p wfgui

.PHONY: test-gui-live
test-gui-live: sccache-setup
	+$(CARGO) test --locked -p wfgui --bin wfgui live_daemon_accepts_subscription_and_activity_query -- --ignored

.PHONY: bench-gui
bench-gui: sccache-setup
	@test -n "$(GUI_BENCH_OUTPUT)" || { printf '%s\n' 'Set GUI_BENCH_OUTPUT to a JSON report path'; exit 2; }
	+GUI_BENCH_OUTPUT="$(GUI_BENCH_OUTPUT)" GUI_BENCH_LIVE="$(GUI_BENCH_LIVE)" $(CARGO) test --locked --profile $(or $(GUI_BENCH_PROFILE),test) -p wfgui --bin wfgui $(or $(GUI_BENCH_CASE),rail_animation_benchmark) -- --ignored --test-threads=1

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

.PHONY: previews-gui
previews-gui: gui-dev
	GUI_PREVIEW_PAGES='$(GUI_PREVIEW_PAGES)' GUI_PREVIEW_SCALES='$(GUI_PREVIEW_SCALES)' \
	GUI_PREVIEW_RESOLUTIONS='$(GUI_PREVIEW_RESOLUTIONS)' GUI_PREVIEW_LIVE='$(GUI_PREVIEW_LIVE)' \
	node tools/ui-reference/gui-previews.mjs

gui-qt-configure-dev: sccache-setup
	test -n "$(NINJA)"
	+cmake --fresh --preset gui-dev --log-level=WARNING -DCMAKE_MAKE_PROGRAM="$(NINJA)"

gui-qt-dev: gui-qt-configure-dev
	+cmake --build --preset gui-dev
	./scripts/stage-gui dev qt

gui-qt-configure-prod: sccache-setup
	test -n "$(NINJA)"
	+cmake --fresh --preset gui-prod --log-level=WARNING -DCMAKE_MAKE_PROGRAM="$(NINJA)"

gui-qt-prod: gui-qt-configure-prod
	+cmake --build --preset gui-prod
	./scripts/stage-gui prod qt

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
	ln -sfn dev/bin/wfgui-qt wfgui-qtd
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
	ln -sfn prod/bin/wfgui-qt wfgui-qt

debug-bridge: sccache-setup
	./scripts/build-debug-bridge

native-bridges: debug-bridge

native-compile-commands: sccache-setup
	./scripts/native-compile-commands

previews: previews-all

previews-all previews-wf previews-poe2: $(PREVIEW_DEPS)
	PREVIEW_SCALE='$(PREVIEW_SCALE)' \
	PREVIEW_LAYOUT='$(PREVIEW_LAYOUT)' \
	PREVIEW_MEDIA='$(PREVIEW_MEDIA)' \
	PREVIEW_SETS='$(PREVIEW_SETS)' \
	PREVIEW_SCENES='$(PREVIEW_SCENES)' \
	PREVIEW_RESOLUTIONS='$(PREVIEW_RESOLUTIONS)' \
	./scripts/generate-previews $(if $(filter previews-all,$@),all,$(if $(filter previews-wf,$@),warframe,poe2))

icon-optics: gui-qt-configure-dev
	+cmake --build --preset gui-dev --target wfgui_icon_optics
	QT_QPA_PLATFORM=offscreen _build/cmake/gui-dev/apps/wfgui/qt/wfgui-icon-optics \
		--config "$(ICON_OPTICS_CONFIG)" --ui-scale "$(ICON_OPTICS_SCALE)" \
		--output-dir "$(ICON_OPTICS_OUTPUT)"

ui-reference-setup:
	./scripts/setup-ui-reference

fix-executables:
	bash ./scripts/fix-executables

test: test-erlang test-companion test-gui test-staging test-build test-poe2

test-erlang:
	./scripts/test-quiet eunit
	./scripts/test-quiet ct

test-companion: native-bridges
	python3 -m unittest discover -s tools/render-bench
	+$(CARGO) test --locked --quiet -p retinue-client -p retinue-native -p retinue-ui -p retinue-poe2 -p retinue-warframe -p retinue-inspect -p arccompanion --features retinue-ui/benchmark
	+$(CARGO) test --locked --quiet --manifest-path $(COMPANION_MANIFEST)
	+$(CARGO) test --locked --quiet --manifest-path $(POE2_COMPANION_MANIFEST)

bench-overlays: export RETINUE_BENCH_OUTPUT := $(abspath $(BENCH_OUTPUT))
bench-overlays: export RETINUE_BENCH_SAMPLES := $(BENCH_SAMPLES)
bench-overlays: export RETINUE_BENCH_THREADS := $(BENCH_THREADS)
bench-overlays: export RETINUE_BENCH_HZ := $(BENCH_HZ)
bench-overlays: export RETINUE_BENCH_CASES := $(BENCH_CASES)
bench-overlays: native-bridges
	+$(CARGO) test --release --locked -p poe2companion --lib overlay_benchmark -- --ignored --nocapture --test-threads=1
	+$(CARGO) test --release --locked -p wfcompanion --bin wfcompanion overlay_benchmark -- --ignored --nocapture --test-threads=1

bench-overlay-scheduling: BENCH_THREADS = 4
bench-overlay-scheduling: export RETINUE_BENCH_OUTPUT := $(abspath $(BENCH_OUTPUT))
bench-overlay-scheduling: export RETINUE_BENCH_SAMPLES := $(BENCH_SAMPLES)
bench-overlay-scheduling: export RETINUE_BENCH_THREADS = $(BENCH_THREADS)
bench-overlay-scheduling: export RETINUE_BENCH_CASES := $(BENCH_CASES)
bench-overlay-scheduling: export RETINUE_BENCH_SCALES := $(BENCH_SCALES)
bench-overlay-scheduling: native-bridges
	+$(CARGO) test --release --locked -p poe2companion --lib overlay_scheduling_benchmark -- --ignored --nocapture --test-threads=1

test-poe2: dev-erlang dev-companion
	python3 ./scripts/tests/poe2_reports.py
	python3 ./scripts/tests/poe2.py --stage dev

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
