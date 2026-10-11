# Knowledge Catalogs

Catalogs retain their source identity: official PublicExport records and WFCD community data are
not silently merged into one provenance.

## Official Codex

`wfcli codex` uses the PublicExport files selected by `codex_export_files/0`. Records with
`excludeFromCodex=true` remain hidden unless `--include-excluded` is supplied. Catalog version is
the SHA-256 of normalized, deduplicated records.

File selection and update ownership are documented in [Export files](exports.md).

## WFCD

`wfcli enemies` and `wfcli drops` use the managed WFCD `warframe-items` data. `WFCDItems.json` is a
compact projection used by player views for names, components, mastery metadata, relic drops, and
image identities. Missing managed data is fetched on demand; `wfcli update --wfcd` forces refresh.

Each refresh resolves npm's latest `@wfcd/items` version, discovers its English JSON files from
UNPKG, and verifies their checksums. jsDelivr is the fallback for that same version. New releases
and categories require no application update; incompatible schemas fail validation. Component
references resolve within the fetched snapshot. Invalid downloads leave existing caches intact.
Each cache records the source, content hash, package version and fetch time.

The daemon checks source age hourly and refreshes data older than 24 hours. Refresh failures and
mirror failures are recorded in the [daemon incident log](../daemon.md#logs).

Recheck [upstream schemas](https://github.com/WFCD/warframe-items) before expanding a projection:
fields combine official exports, community data and wiki enrichment.

Loaders collapse display-identical enemy and drop rows. Entries with different stats,
descriptions, resistances, or drop tables remain separate.

## Runtime Ownership

`wf_exports_store` owns prepared catalogs and entity caches. `wf_source_manager` owns stale
checks and refresh jobs. Focused commands and unified query use the same daemon parser, evaluator,
and data-only replies.

See [Daemon architecture](daemon.md) for queues and cache lifetime, and
[Query language](query-language.md) for query boundaries.
