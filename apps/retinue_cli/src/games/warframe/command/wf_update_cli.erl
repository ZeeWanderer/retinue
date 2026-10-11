-module(wf_update_cli).

-export([command/0, run/1, metadata_selections/1]).
-ifdef(TEST).
-export([format_error/1]).
-endif.
-import(retinue_cli_args, [option/4, flag/3]).

command() ->
    Sources = [default, all, nodes, languages, manifest, exports, recipes,
               upgrades, weapons, warframes, resources, wfcd],
    #{help => "update cached knowledge base data", handler => {?MODULE, run},
      defaults => #{selections => []},
      arguments => [(flag(selections, atom_to_list(Source),
                          "refresh " ++ atom_to_list(Source)))#{action => {append, Source}}
                    || Source <- Sources] ++
                   [flag(worldstate, "worldstate", "refresh worldstate"),
                    flag(trader, "trader", "refresh trader inventory"),
                    option(cache, "cache", string, "worldstate cache file"),
                    option(trader_cache, "trader-cache", string, "trader cache file")]}.

run(Parsed) ->
    Results = refresh_metadata(metadata_selections(Parsed)) ++
              maybe_refresh_worldstate(Parsed) ++ maybe_refresh_trader(Parsed),
    Success = lists:all(fun(#{result := Result}) -> Result =:= ok end, Results),
    retinue_output:emit(#{success => Success, results => [json_result(R) || R <- Results]},
                      fun() -> lists:foreach(fun print_source_result/1, Results) end),
    case Success of
        true -> ok;
        false -> halt(1)
    end.

json_result(#{source := Source, result := ok}) -> #{source => Source, success => true};
json_result(#{source := Source, result := {error, Reason}}) ->
    #{source => Source, success => false, error => Reason}.

metadata_selections(#{selections := []} = Args) ->
    case maps:get(worldstate, Args, false) orelse maps:get(trader, Args, false) of
        true -> [];
        false -> [default]
    end;
metadata_selections(#{selections := Sources}) ->
    case lists:member(all, Sources) of true -> [all]; false -> lists:usort(Sources) end.

-doc "Ask the daemon to refresh selected managed metadata sources.".
-spec refresh_metadata([atom()]) -> [map()].
refresh_metadata([]) -> [];
refresh_metadata(Selections) ->
    Request = #{source => metadata, action => refresh, selections => Selections},
    case retinue_client:one_shot(warframe, Request) of
        {ok, #{results := Results}} -> Results;
        {error, Reason} -> [#{source => metadata, result => {error, Reason}}]
    end.

print_source_result(#{source := Source, result := ok}) ->
    io:format("updated ~s~n", [source_label(Source)]);
print_source_result(#{source := Source, result := {error, Reason}}) ->
    io:format(standard_error, "failed: ~s: ~ts~n", [source_label(Source), format_error(Reason)]).

format_error({wfcd_version_failed, Reason}) ->
    ["could not discover the latest WFCD version; existing caches kept.\n  ", format_error(Reason)];
format_error({wfcd_sources_failed, Version, Errors}) ->
    ["could not load WFCD ", Version, "; existing caches kept.",
     [["\n  ", atom_to_list(Mirror), ": ", format_error(Reason)] || {Mirror, Reason} <- Errors]];
format_error({http_status, Url, Status}) ->
    io_lib:format("HTTP ~B: ~ts (~ts)", [Status, http_hint(Status), Url]);
format_error({http_failed, Url, Reason}) ->
    io_lib:format("~ts (~p; ~ts)", [network_hint(Reason), Reason, Url]);
format_error({invalid_data, Url, Reason}) ->
    io_lib:format("~ts (~ts)", [data_hint(Reason), Url]);
format_error({file_integrity_mismatch, Url}) ->
    ["checksum mismatch; mirror content differs from its manifest (", Url, ")"];
format_error({file_size_mismatch, Url, Expected, Actual}) ->
    io_lib:format("incomplete or inconsistent download: expected ~B bytes, got ~B (~ts)",
                  [Expected, Actual, Url]);
format_error({download_size_limit, Url}) ->
    ["response exceeds the declared size or download safety limit (", Url, ")"];
format_error({http_error, Url, Status, _Body}) -> format_error({http_status, Url, Status});
format_error({http_error, Status, _Body}) -> io_lib:format("HTTP ~B: ~ts", [Status, http_hint(Status)]);
format_error({failed_connect, _} = Reason) ->
    io_lib:format("~ts (~p)", [network_hint(Reason), Reason]);
format_error(timeout) -> network_hint(timeout);
format_error({Action, Path, Reason}) when Action =:= write_failed; Action =:= mkdir_failed ->
    io_lib:format("could not save cache: ~ts (~ts)", [file:format_error(Reason), Path]);
format_error({not_in_index, File}) ->
    ["file missing from PublicExport index: ", File, "; possible upstream layout change"];
format_error(Reason) -> retinue_client:format_error(Reason).

http_hint(404) -> "file or release not found; mirror publication may be delayed or the upstream path changed";
http_hint(410) -> "upstream removed this resource";
http_hint(429) -> "rate limited; retry later";
http_hint(Status) when Status =:= 401; Status =:= 403 -> "access denied; check proxy or network restrictions";
http_hint(Status) when Status >= 500 -> "upstream service failure; retry later";
http_hint(_) -> "unexpected upstream response".

network_hint({failed_connect, Details}) ->
    case [Reason || {_, _, Reason} <- Details] of
        [Reason | _] -> network_hint(Reason);
        [] -> "connection failed"
    end;
network_hint(nxdomain) -> "DNS lookup failed; check DNS and connectivity";
network_hint(timeout) -> "request timed out; check connectivity or retry later";
network_hint(connect_timeout) -> network_hint(timeout);
network_hint(econnrefused) -> "connection refused";
network_hint(enetunreach) -> "network unreachable";
network_hint(ehostunreach) -> "host unreachable";
network_hint({tls_alert, _}) -> "TLS verification or handshake failed; check system clock, certificates and proxy";
network_hint(_) -> "network request failed".

data_hint(invalid_json) -> "response is not valid JSON; possible error page, truncated data or format change";
data_hint(catalog_size_limit) -> "catalog exceeds the download safety limit";
data_hint(catalog_file_limit) -> "catalog exceeds the file-count safety limit";
data_hint(duplicate_catalog_file) -> "manifest contains duplicate file paths";
data_hint({incomplete_catalog, Missing}) ->
    io_lib:format("required categories missing (~p); possible upstream layout change", [Missing]);
data_hint({unresolved_component, Id}) ->
    ["component reference has no matching item: ", Id, "; incomplete dataset or upstream layout change"];
data_hint({invalid_catalog_item, Id}) ->
    io_lib:format("item ~p lacks a valid name or identity; possible upstream schema change", [Id]);
data_hint(Reason) when Reason =:= package_identity_mismatch; Reason =:= invalid_package_identity;
                       Reason =:= invalid_package_version; Reason =:= invalid_file_manifest ->
    io_lib:format("unexpected package identity, version or manifest (~p); possible stale mirror or schema change", [Reason]);
data_hint(Reason) ->
    io_lib:format("unexpected data layout (~p); upstream may require an adapter update", [Reason]).

source_label(nodes) -> "solNodes.json";
source_label(languages) -> "languages.json";
source_label(manifest) -> "ExportManifest.json";
source_label(exports) -> "all PublicExport metadata";
source_label(recipes) -> "ExportRecipes_en.json";
source_label(upgrades) -> "ExportUpgrades_en.json";
source_label(weapons) -> "ExportWeapons_en.json";
source_label(warframes) -> "ExportWarframes_en.json";
source_label(resources) -> "ExportResources_en.json";
source_label(wfcd) -> "WFCD item and enemy catalogs";
source_label(star_chart) -> "Star Chart mastery metadata";
source_label(worldstate) -> "worldstate cache";
source_label(trader) -> "trader inventory cache";
source_label(Source) -> atom_to_list(Source).

maybe_refresh_worldstate(Parsed) ->
    case maps:get(worldstate, Parsed, false) of
        true ->
            Opts = refresh_opts(maps:get(cache, Parsed, undefined)),
            Result = retinue_client:one_shot(warframe, #{source => worldstate, opts => Opts,
                                                    query => undefined, type_filter => undefined,
                                                    day_filter => undefined, mode => list,
                                                    inventory => false}),
            [refresh_result(worldstate, Result)];
        false -> []
    end.

maybe_refresh_trader(Parsed) ->
    case maps:get(trader, Parsed, false) of
        true ->
            Opts = refresh_opts(maps:get(trader_cache, Parsed, undefined)),
            Result = retinue_client:one_shot(warframe, #{source => trader, opts => Opts}),
            [refresh_result(trader, Result)];
        false -> []
    end.

refresh_result(Source, {error, _} = Error) -> #{source => Source, result => Error};
refresh_result(Source, {ok, _}) -> #{source => Source, result => ok}.

refresh_opts(undefined) -> #{refresh => true};
refresh_opts(Cache) -> #{refresh => true, cache => filename:absname(Cache)}.
