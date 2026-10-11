%%%-------------------------------------------------------------------
%% Versioned owner-only persistence for market metadata and quotes.
%%%-------------------------------------------------------------------
-module(wf_market_cache).

-export([path/0, empty/0, load/1, persist/2]).

-define(CACHE_VERSION, 4).

-doc "Return configured market cache path.".
-spec path() -> file:filename_all().
path() ->
    case application:get_env(retinue_daemon, market_cache) of
        {ok, Path} -> Path;
        undefined -> wf_paths:cache_file("market.term")
    end.

-doc "Return empty canonical market snapshot.".
-spec empty() -> map().
empty() ->
    #{items => [], manifest_fetched_at => undefined, manifest_attempted_at => undefined,
      updated_at => undefined, quotes => #{}, variant_quotes => #{}, details => #{},
      relics => #{}, relics_version => wf_relic_recommendations:catalog_version(),
      relics_fetched_at => undefined,
      relics_attempted_at => undefined}.

-doc "Load valid cache data; malformed or obsolete files become an empty snapshot.".
-spec load(file:filename_all()) -> map().
load(Path) ->
    case file:read_file(Path) of
        {ok, Binary} ->
            try binary_to_term(Binary, [safe]) of
                #{version := Version, snapshot := Snapshot}
                  when is_map(Snapshot), Version >= 1, Version =< ?CACHE_VERSION ->
                    normalize(Snapshot);
                _ -> empty()
            catch _:_ -> empty()
            end;
        {error, _Reason} -> empty()
    end.

-doc "Atomically persist market snapshot with owner-only permissions.".
-spec persist(file:filename_all(), map()) -> ok | {error, term()}.
persist(Path, Snapshot) ->
    case filelib:ensure_dir(Path) of
        ok ->
            Temp = Path ++ ".tmp",
            Binary = term_to_binary(#{version => ?CACHE_VERSION, snapshot => Snapshot},
                                    [compressed]),
            case file:write_file(Temp, Binary) of
                ok ->
                    _ = file:change_mode(Temp, 8#600),
                    file:rename(Temp, Path);
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end.

normalize(Snapshot) ->
    Quotes = maps:map(fun(_Slug, Quote) -> normalize_quote(Quote) end,
                      maps:get(quotes, Snapshot, #{})),
    VariantQuotes = maps:map(fun(_Key, Quote) -> normalize_quote(Quote) end,
                             maps:get(variant_quotes, Snapshot, #{})),
    #{items => maps:get(items, Snapshot, []),
      manifest_fetched_at => maps:get(manifest_fetched_at, Snapshot, undefined),
      manifest_attempted_at => maps:get(manifest_attempted_at, Snapshot, undefined),
      updated_at => maps:get(updated_at, Snapshot, undefined),
      quotes => Quotes,
      variant_quotes => VariantQuotes,
      details => maps:get(details, Snapshot, #{}),
      relics => maps:get(relics, Snapshot, #{}),
      relics_version => maps:get(relics_version, Snapshot, undefined),
      relics_fetched_at => maps:get(relics_fetched_at, Snapshot, undefined),
      relics_attempted_at => maps:get(relics_attempted_at, Snapshot, undefined)}.

normalize_quote(Quote) when is_map(Quote) ->
    case maps:get(stale, Quote, false) of
        true -> Quote#{source => stale};
        false -> Quote#{source => cached}
    end;
normalize_quote(Quote) -> Quote.
