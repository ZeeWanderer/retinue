REBAR3 ?= rebar3
.DEFAULT_GOAL := all
DOCS_BASE ?= /retinue/
export DOCS_BASE
CARGO ?= cargo
REBAR_CACHE_DIR ?= $(CURDIR)/.cache/rebar3
CARGO_TARGET_DIR ?= $(CURDIR)/_build/cargo
SCCACHE ?= $(shell command -v sccache 2>/dev/null)
SCCACHE_BASEDIRS ?= $(CURDIR)
SCCACHE_DIR ?= $(CURDIR)/.cache/sccache
SCCACHE_SERVER_UDS ?= $(CURDIR)/.cache/sccache.sock
LLVM_ROOT ?= $(shell command -v clang >/dev/null 2>&1 && dirname "$$(dirname "$$(realpath "$$(command -v clang)")")")

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
BENCH_SAMPLES ?= 60
BENCH_RUNS ?= 7
BENCH_SUITE ?= poe2-overlay
BENCH_THREADS ?= 1
BENCH_HZ ?= 0
BENCH_CASES ?=
BENCH_SCALES ?= 1,1.25,1.5
BENCH_OUTPUT ?= $(CURDIR)/research/retinue/benchmarks/$(shell date -u +%Y%m%dT%H%M%SZ)
PROFILE_APP ?= wfgui
PROFILE_FLAVOR ?= prod
PROFILE_OUTPUT ?= $(CURDIR)/research/retinue/profiles/$(shell date -u +%Y%m%dT%H%M%SZ)-$(PROFILE_APP)-$(PROFILE_FLAVOR)
PROFILE_TARGET_DIR ?= $(CURDIR)/_build/profiling

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
	dev-erlang prod-erlang dev-companion prod-companion links \
	debug-bridge native-bridges previews previews-all previews-wf previews-poe2 ui-reference-setup fix-executables \
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

test-gui: test-gui-app

.PHONY: test-ui test-native test-tools test-integration test-live bench bench-list bench-prepare bench-compare
test-ui: sccache-setup
	+$(CARGO) test --locked -p retinue-render -p retinue-ui -p retinue-ui-macros --no-default-features
	+$(CARGO) test --locked -p retinue-render -p retinue-ui --features retinue-ui/benchmark
	+$(CARGO) test --locked -p retinue-render -p retinue-ui --no-default-features --features retinue-ui/desktop --all-targets

test-native: sccache-setup
	+$(CARGO) test --locked --quiet -p retinue-protocol -p retinue-client -p retinue-native -p retinue-poe2 -p retinue-warframe -p retinue-inspect $(NATIVE_TEST)

test-tools:
	python3 -m unittest discover -s tools/benchmarks
	python3 -m unittest discover -s tools/profiling
	python3 -m unittest discover -s scripts/tests -p 'test_*.py'
	python3 -m unittest discover -s test/integration -p 'test_*.py'
	+$(MAKE) test-docs

.PHONY: profile-build
profile-build: sccache-setup
	+python3 tools/profiling/build.py --app "$(PROFILE_APP)" --flavor "$(PROFILE_FLAVOR)" \
		--output "$(PROFILE_OUTPUT)" --target-dir "$(PROFILE_TARGET_DIR)"

.PHONY: test-gui-app
test-gui-app: sccache-setup
	+$(CARGO) test --locked -p wfgui --all-targets $(GUI_TEST)

.PHONY: test-composition test-ui-sdk test-ui-macros view-demo view-sdk-demo view-sdk-content bench-views bench-repair bench-reuse bench-cost
test-composition: sccache-setup
	+$(CARGO) test --locked -p retinue-render -p retinue-ui -p retinue-ui-macros --no-default-features

test-ui-sdk: sccache-setup
	+$(CARGO) test --locked -p retinue-ui --no-default-features $(UI_TEST)

test-ui-macros: sccache-setup
	+$(CARGO) test --locked -p retinue-ui-macros

bench-repair: sccache-setup
	@test -n "$(REPAIR_REPORT)" || { printf '%s\n' 'Set REPAIR_REPORT to a JSONL report path'; exit 2; }
	+RETINUE_REPAIR_REPORT="$(abspath $(REPAIR_REPORT))" $(CARGO) test --locked --release -p retinue-render --features benchmark repair_benchmark -- --ignored --test-threads=1

bench-reuse: sccache-setup
	@test -n "$(REUSE_REPORT)" || { printf '%s\n' 'Set REUSE_REPORT to a JSONL report path'; exit 2; }
	+REUSE_REPORT="$(abspath $(REUSE_REPORT))" $(CARGO) test --locked --release -p retinue-render --features benchmark automatic_reuse_benchmark -- --ignored --test-threads=1

bench-cost: sccache-setup
	@test -n "$(COST_REPORT)" || { printf '%s\n' 'Set COST_REPORT to a CSV report path'; exit 2; }
	+COST_REPORT="$(abspath $(COST_REPORT))" $(CARGO) test --locked --release -p retinue-render --features benchmark native_cost_benchmark -- --ignored --test-threads=1

view-demo: sccache-setup
	+$(CARGO) run --locked --release -p retinue-ui --no-default-features --features desktop --example views -- $(VIEW_ARGS)

view-sdk-demo: sccache-setup
	+$(CARGO) run --locked -p retinue-ui --no-default-features --example retained -- $(or $(VIEW_OUTPUT),research/retinue/ui-sdk/view-runtime/example)

view-sdk-content: sccache-setup
	+$(CARGO) run --locked -p retinue-ui --no-default-features --example content -- $(or $(VIEW_OUTPUT),research/retinue/ui-sdk/content/example)

bench-views: sccache-setup
	+RETINUE_BENCH_THREADS=4 RETINUE_BENCH_OUTPUT="$(or $(VIEW_OUTPUT),research/retinue/ui-views)" $(CARGO) run --locked --release -p retinue-ui --no-default-features --features desktop --example views -- --benchmark --output "$(or $(VIEW_OUTPUT),research/retinue/ui-views)" $(VIEW_ARGS)

.PHONY: test-gui-live
test-gui-live: sccache-setup
	+$(CARGO) test --locked -p wfgui --bin wfgui live_daemon_accepts_subscription_and_activity_query -- --ignored

.PHONY: bench-gui
bench-gui: sccache-setup
	@test -n "$(GUI_BENCH_OUTPUT)" || { printf '%s\n' 'Set GUI_BENCH_OUTPUT to a JSON report path'; exit 2; }
	+GUI_BENCH_OUTPUT="$(abspath $(GUI_BENCH_OUTPUT))" GUI_BENCH_LIVE="$(GUI_BENCH_LIVE)" $(CARGO) test --locked --release -p wfgui $(if $(GUI_BENCH_FEATURES),--features $(GUI_BENCH_FEATURES)) --bin wfgui $(or $(GUI_BENCH_CASE),rail_animation_benchmark) -- --ignored --test-threads=1

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

previews: previews-all

previews-all previews-wf previews-poe2: $(PREVIEW_DEPS)
	PREVIEW_SCALE='$(PREVIEW_SCALE)' \
	PREVIEW_LAYOUT='$(PREVIEW_LAYOUT)' \
	PREVIEW_MEDIA='$(PREVIEW_MEDIA)' \
	PREVIEW_SETS='$(PREVIEW_SETS)' \
	PREVIEW_SCENES='$(PREVIEW_SCENES)' \
	PREVIEW_RESOLUTIONS='$(PREVIEW_RESOLUTIONS)' \
	./scripts/generate-previews $(if $(filter previews-all,$@),all,$(if $(filter previews-wf,$@),warframe,poe2))

ui-reference-setup:
	./scripts/setup-ui-reference

fix-executables:
	bash ./scripts/fix-executables

test: test-erlang test-native test-ui test-companion test-gui test-tools test-staging test-build test-integration

test-erlang:
	./scripts/test-quiet eunit
	./scripts/test-quiet ct

.PHONY: test-protocol
test-protocol: sccache-setup
	+$(CARGO) test --locked -p retinue-protocol -p retinue-client

test-companion: native-bridges
	+$(CARGO) test --locked --quiet -p arccompanion
	+$(CARGO) test --locked --quiet --manifest-path $(COMPANION_MANIFEST)
	+$(CARGO) test --locked --quiet --manifest-path $(POE2_COMPANION_MANIFEST)

.PHONY: test-wfcompanion test-poe2companion
test-wfcompanion: sccache-setup
	+$(CARGO) test --locked --manifest-path $(COMPANION_MANIFEST) $(COMPANION_TEST)

test-poe2companion: sccache-setup
	+$(CARGO) test --locked --manifest-path $(POE2_COMPANION_MANIFEST) $(COMPANION_TEST)

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
	+$(CARGO) build --locked -p retinue-protocol --example fixture-codec
	python3 ./test/integration/poe2.py --stage dev --codec "$(CARGO_TARGET_DIR)/debug/examples/fixture-codec"

test-integration: test-poe2

test-live:
	@test "$(LIVE_TEST)" = gui-session || { printf '%s\n' 'Select explicitly: make test-live LIVE_TEST=gui-session (needs a running daemon)'; exit 2; }
	+$(MAKE) test-gui-live

bench-list:
	python3 tools/benchmarks/run.py list

bench: sccache-setup
	+python3 tools/benchmarks/run.py record $(BENCH_SUITE) --output "$(BENCH_OUTPUT)" --repeats $(BENCH_RUNS) --samples $(BENCH_SAMPLES) --threads $(BENCH_THREADS) --hz $(BENCH_HZ) --cases "$(BENCH_CASES)"

bench-prepare: sccache-setup
	+python3 tools/benchmarks/run.py prepare $(BENCH_SUITE) --output "$(BENCH_OUTPUT)"

bench-compare:
	@test -n "$(BENCH_BEFORE)" -a -n "$(BENCH_AFTER)" -a -n "$(BENCH_POLICY)" || { printf '%s\n' 'Set BENCH_BEFORE, BENCH_AFTER and BENCH_POLICY'; exit 2; }
	python3 tools/benchmarks/compare.py "$(BENCH_BEFORE)" "$(BENCH_AFTER)" --policy "$(BENCH_POLICY)"

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

.PHONY: docs docs-setup docs-api-rust docs-api-erlang docs-site docs-check docs-serve test-docs
docs-setup: tools/docs/node_modules/.package-lock.json

tools/docs/node_modules/.package-lock.json: tools/docs/package.json tools/docs/package-lock.json
	npm ci --prefix tools/docs --no-audit --no-fund

docs-api-rust: sccache-setup
	+$(CARGO) doc --locked --workspace --exclude retinue-protocol --no-deps --document-private-items --features retinue-ui/desktop

docs-api-erlang:
	@mkdir -p _build/documentation/erlang
	@for app in retinue_core wfcore poe2core retinue_cli retinue_daemon; do \
		$(REBAR3) as docs_api ex_doc --app "$$app" --formatter html --output "$(CURDIR)/_build/documentation/erlang/$$app" || exit; \
	done
	escript tools/docs/cli.escript "$$($(REBAR3) as docs_api,docs path --base)" "$(CURDIR)/_build/documentation/cli.json"

docs-site:
	npm --prefix tools/docs run build

docs: docs-setup
	+$(MAKE) docs-api-rust
	+$(MAKE) docs-api-erlang
	+$(MAKE) docs-site

docs-check:
	node tools/docs/check.mjs

docs-serve:
	npm --prefix tools/docs run preview -- --port 4321

test-docs: docs-setup
	npm --prefix tools/docs test
