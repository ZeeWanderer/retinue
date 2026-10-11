%%%-------------------------------------------------------------------
%% Managed WFCD metadata and official PublicExport updates.
%%%-------------------------------------------------------------------
-module(wf_metadata_update).

-export([update_nodes/0, update_languages/0, update_manifest/0,
         update_export/1, update_exports/1, update_all_exports/0, update_all/0,
         export_files/0, resolver_export_files/0, codex_export_files/0,
         write_file/2, paths/1]).

-define(EXPORT_BASE, "https://content.warframe.com/PublicExport/Manifest/").
-define(EXPORT_INDEX, "https://content.warframe.com/PublicExport/index_en.txt.lzma").
-define(EXPORT_FILES, ["ExportManifest.json",
                       "ExportRecipes_en.json",
                       "ExportUpgrades_en.json",
                       "ExportWeapons_en.json",
                       "ExportWarframes_en.json",
                       "ExportResources_en.json",
                       "ExportGear_en.json",
                       "ExportKeys_en.json",
                       "ExportRelicArcane_en.json",
                       "ExportSortieRewards_en.json",
                       "ExportCustoms_en.json",
                       "ExportDrones_en.json",
                       "ExportFlavour_en.json",
                       "ExportFusionBundles_en.json",
                       "ExportRegions_en.json",
                       "ExportSentinels_en.json"]).

-define(RESOLVER_EXPORT_FILES, ["ExportRecipes_en.json",
                                "ExportUpgrades_en.json",
                                "ExportWeapons_en.json",
                                "ExportWarframes_en.json",
                                "ExportResources_en.json",
                                "ExportGear_en.json",
                                "ExportKeys_en.json",
                                "ExportRelicArcane_en.json",
                                "ExportSortieRewards_en.json"]).

-define(CODEX_EXPORT_FILES, ["ExportWeapons_en.json",
                             "ExportWarframes_en.json",
                             "ExportResources_en.json",
                             "ExportGear_en.json",
                             "ExportKeys_en.json",
                             "ExportRelicArcane_en.json",
                             "ExportCustoms_en.json",
                             "ExportFlavour_en.json",
                             "ExportRegions_en.json",
                             "ExportSentinels_en.json",
                             "ExportDrones_en.json"]).

-type metadata_name() :: file:filename_all().
-type update_result() :: ok | {error, term()}.

-doc "Refresh node-name metadata used to resolve worldstate node ids.".
-spec update_nodes() -> update_result().
update_nodes() ->
    update_json(
        "https://raw.githubusercontent.com/WFCD/warframe-worldstate-data/master/data/solNodes.json",
        "solNodes.json", {wfcli, node_map}).

-doc "Refresh localized strings used by item, node, and mission renderers.".
-spec update_languages() -> update_result().
update_languages() ->
    update_json(
        "https://raw.githubusercontent.com/WFCD/warframe-worldstate-data/master/data/languages.json",
        "languages.json", {wfcli, lang_map}).

update_json(Url, Name, TermKey) ->
    ensure_http(),
    case httpc:request(get, {Url, []}, [{timeout, 10000}],
                       [{body_format, binary}]) of
        {ok, {{_, 200, _}, _Headers, Body}} ->
            write(Name, TermKey, Body);
        {ok, {{_, Code, _}, _Headers, Body}} ->
            {error, {http_error, Code, Body}};
        {error, Reason} ->
            {error, Reason}
    end.

-doc "Refresh only ExportManifest.json through current hashed export index.".
-spec update_manifest() -> update_result().
update_manifest() ->
    update_export("ExportManifest.json").

-doc "Refresh every configured PublicExport using one shared index fetch.".
-spec update_all_exports() -> update_result().
update_all_exports() ->
    update_exports(?EXPORT_FILES).

-doc "Refresh selected PublicExport files using one shared index fetch.".
-spec update_exports([metadata_name()]) -> update_result().
update_exports(Files) when is_list(Files) ->
    case fetch_index() of
        {ok, Index} ->
            chain([fun() -> update_export(File, Index) end
                   || File <- lists:usort(Files)]);
        {error, Reason} ->
            {error, Reason}
    end.

-doc "Refresh one PublicExport file by logical filename.".
-spec update_export(metadata_name()) -> update_result().
update_export(File) when is_list(File) ->
    case fetch_index() of
        {ok, Index} -> update_export(File, Index);
        {error, Reason} -> {error, Reason}
    end.

update_export(File, Index) ->
    ensure_http(),
    case maps:get(File, Index, undefined) of
        undefined ->
            {error, {not_in_index, File}};
        Hashed ->
            Url = ?EXPORT_BASE ++ Hashed,
            case httpc:request(get, {Url, []}, [{timeout, 20000}],
                               [{body_format, binary}]) of
                {ok, {{_, 200, _}, _Headers, Body}} ->
                    write(File, {wfcli, export, File}, Body);
                {ok, {{_, Code, _}, _Headers, Body}} ->
                    {error, {http_error, Url, Code, Body}};
                {error, Reason} ->
                    {error, Reason}
            end
    end.

fetch_index() ->
    ensure_http(),
    case httpc:request(get, {?EXPORT_INDEX, []}, [{timeout, 15000}],
                       [{body_format, binary}]) of
        {ok, {{_, 200, _}, _Headers, Body}} ->
            case decompress_lzma(Body) of
                {ok, Text} -> {ok, parse_index(Text)};
                Error -> Error
            end;
        {ok, {{_, Code, _}, _Headers, Body}} ->
            {error, {http_error, ?EXPORT_INDEX, Code, Body}};
        {error, Reason} ->
            {error, Reason}
    end.

parse_index(Bin) ->
    lists:foldl(
      fun(Line, Acc) ->
          case string:split(Line, "!", all) of
              [Base, Hashed] ->
                  case filename:extension(Base) of
                      ".json" -> maps:put(Base, Base ++ "!" ++ strip_cr(Hashed), Acc);
                      _ -> Acc
                  end;
              _ ->
                  Acc
          end
      end,
      #{},
      string:split(binary_to_list(Bin), "\n", all)).

strip_cr(Str) ->
    string:trim(Str, trailing, "\r").

decompress_lzma(Bin) when is_binary(Bin) ->
    case os:find_executable("xz") of
        false ->
            {error, no_lzma};
        Xz ->
            Tmp = temp_path(),
            case file:write_file(Tmp, Bin) of
                ok ->
                    try run_xz(Xz, Tmp)
                    after
                        _ = file:delete(Tmp)
                    end;
                {error, Reason} ->
                    {error, {temp_write_failed, Reason}}
            end
    end.

run_xz(Xz, Tmp) ->
    Port = open_port({spawn_executable, Xz},
                     [binary, exit_status, stderr_to_stdout,
                      {args, ["-d", "-c", Tmp]}]),
    collect_xz(Port, []).

collect_xz(Port, Acc) ->
    receive
        {Port, {data, Data}} ->
            collect_xz(Port, [Data | Acc]);
        {Port, {exit_status, 0}} ->
            case iolist_to_binary(lists:reverse(Acc)) of
                <<>> -> {error, lzma_failed};
                Output -> {ok, Output}
            end;
        {Port, {exit_status, Status}} ->
            {error, {lzma_failed, Status,
                     iolist_to_binary(lists:reverse(Acc))}}
    end.

temp_path() ->
    Base =
        case os:getenv("TMPDIR") of
            false -> "/tmp";
            undefined -> "/tmp";
            Value -> Value
        end,
    Unique = erlang:unique_integer([monotonic, positive]),
    filename:join(Base, io_lib:format("wf_index_~p.lzma", [Unique])).

-doc "Refresh all metadata needed for resolved worldstate/export queries.".
-spec update_all() -> update_result().
update_all() ->
    chain([fun update_nodes/0, fun update_languages/0, fun update_all_exports/0]).

-doc "Write validated metadata into retinue_daemon's preferred cache root.".
-spec write_file(metadata_name(), binary()) -> update_result().
write_file(Name, Body) when is_list(Name), is_binary(Body) ->
    write(Name, {wfcli, metadata, Name}, Body).

write(Name, TermKey, Body) ->
    case paths(Name) of
        [Path | _] ->
            Dir = filename:dirname(Path),
            case ensure_dir(Dir) of
                ok ->
                    case write_atomic(Path, Body) of
                        ok ->
                            persistent_term:erase(TermKey),
                            clear_export_cache(Name),
                            ok;
                        {error, Reason} ->
                            {error, {write_failed, Path, Reason}}
                    end;
                {error, Reason} ->
                    {error, {mkdir_failed, Dir, Reason}}
            end;
        [] ->
            {error, no_path}
    end.

write_atomic(Path, Body) ->
    Suffix = integer_to_list(erlang:unique_integer([monotonic, positive])),
    Tmp = Path ++ ".tmp." ++ Suffix,
    case file:write_file(Tmp, Body) of
        ok ->
            case file:rename(Tmp, Path) of
                ok ->
                    ok;
                {error, Reason} ->
                    _ = file:delete(Tmp),
                    {error, Reason}
            end;
        {error, Reason} ->
            {error, Reason}
    end.

clear_export_cache(Name) ->
    case lists:prefix("Export", Name) of
        true -> persistent_term:erase({wfcli, item_map});
        false -> ok
    end,
    case lists:prefix("ExportRecipes", Name) of
        true -> wf_item_catalog:invalidate();
        false -> ok
    end,
    case lists:prefix("ExportUpgrades", Name) of
        true ->
            wf_resolve_registry:invalidate_mod_cache(),
            persistent_term:erase({wfcli, mod_db});
        false ->
            ok
    end.

-doc "Return candidate metadata paths in write preference order.".
-spec paths(metadata_name()) -> [file:filename_all()].
paths(Name) ->
    [filename:join(Base, Name) || Base <- base_dirs()].

base_dirs() ->
    ScriptDir = script_dir(),
    Cwd = filename:absname("."),
    CodePriv =
        case code:priv_dir(retinue_daemon) of
            Dir when is_list(Dir) ->
                case filelib:is_dir(Dir) of
                    true -> Dir;
                    false -> undefined
                end;
            _ -> undefined
        end,
    LibPriv =
        case code:lib_dir(retinue_daemon) of
            Lib when is_list(Lib) -> filename:join(Lib, "priv");
            _ -> undefined
        end,
    unique_paths(
      [wf_paths:cache_dir(),
       CodePriv,
       LibPriv,
       filename:join([Cwd, "apps", "retinue_daemon", "priv"]),
       filename:join([Cwd, "priv"]),
       filename:join([ScriptDir, "apps", "retinue_daemon", "priv"]),
       filename:join(ScriptDir, "priv"),
       filename:join([filename:dirname(ScriptDir), "apps", "retinue_daemon", "priv"]),
       filename:join([filename:dirname(ScriptDir), "priv"])]).

unique_paths(Paths) ->
    lists:reverse(
      lists:foldl(
        fun(undefined, Acc) ->
                Acc;
           (Path, Acc) ->
                Absolute = filename:absname(Path),
                case lists:member(Absolute, Acc) of
                    true -> Acc;
                    false -> [Absolute | Acc]
                end
        end,
        [],
        Paths)).

-doc "Return every configured PublicExport filename.".
-spec export_files() -> [string()].
export_files() -> ?EXPORT_FILES.

-doc "Return PublicExports used by identifier resolution.".
-spec resolver_export_files() -> [string()].
resolver_export_files() -> ?RESOLVER_EXPORT_FILES.

-doc "Return PublicExports used by static Codex catalog.".
-spec codex_export_files() -> [string()].
codex_export_files() -> ?CODEX_EXPORT_FILES.

ensure_http() ->
    application:ensure_all_started(inets),
    application:ensure_all_started(ssl).

chain([]) ->
    ok;
chain([Fun | Rest]) ->
    case Fun() of
        ok -> chain(Rest);
        Error -> Error
    end.

script_dir() ->
    try escript:script_name() of
        Script when is_list(Script), Script =/= [] -> filename:dirname(Script);
        _ -> filename:absname(".")
    catch
        _:_ -> filename:absname(".")
    end.

ensure_dir(Dir) ->
    case filelib:is_dir(Dir) of
        true -> ok;
        false ->
            case filelib:ensure_dir(filename:join(Dir, "dummy")) of
                {error, enotdir} -> ok;
                Result -> Result
            end
    end.
