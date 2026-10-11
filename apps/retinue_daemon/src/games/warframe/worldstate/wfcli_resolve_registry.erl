%%%-------------------------------------------------------------------
%% Resolver map registry (language, item, node, mod).
%%%-------------------------------------------------------------------
-module(wfcli_resolve_registry).

-export([language_map/0, item_map/0, node_map/0, mod_map/0, mod_name_index/0,
         invalidate_mod_cache/0]).

-define(ITEM_KEYS, [<<"displayName">>, <<"name">>, <<"locName">>, <<"ItemName">>]).
-define(MOD_MAP_KEY, {wfcli, mod_map, 2}).
-define(MOD_NAME_INDEX_KEY, {wfcli, mod_name_index, 2}).

language_map() ->
    case persistent_term:get({wfcli, lang_map}, undefined) of
        M when is_map(M) -> M;
        _ ->
            Map = lower_key_map(load_language_map()),
            persistent_term:put({wfcli, lang_map}, Map),
            Map
    end.

item_map() ->
    case persistent_term:get({wfcli, item_map}, undefined) of
        M when is_map(M) -> M;
        _ ->
            Map = lower_key_map(load_item_map()),
            persistent_term:put({wfcli, item_map}, Map),
            Map
    end.

node_map() ->
    case persistent_term:get({wfcli, node_map}, undefined) of
        M when is_map(M) -> M;
        _ ->
            Map = lower_key_map(load_node_map()),
            persistent_term:put({wfcli, node_map}, Map),
            Map
    end.

mod_map() ->
    case persistent_term:get(?MOD_MAP_KEY, undefined) of
        M when is_map(M) -> M;
        _ ->
            {Map, NameIndex} = load_mod_caches(),
            persistent_term:put(?MOD_MAP_KEY, Map),
            persistent_term:put(?MOD_NAME_INDEX_KEY, NameIndex),
            Map
    end.

mod_name_index() ->
    case persistent_term:get(?MOD_NAME_INDEX_KEY, undefined) of
        M when is_map(M) -> M;
        _ ->
            {Map, NameIndex} = load_mod_caches(),
            persistent_term:put(?MOD_MAP_KEY, Map),
            persistent_term:put(?MOD_NAME_INDEX_KEY, NameIndex),
            NameIndex
    end.

-doc "Drop mod metadata projections after source or projection changes.".
-spec invalidate_mod_cache() -> ok.
invalidate_mod_cache() ->
    persistent_term:erase(?MOD_MAP_KEY),
    persistent_term:erase(?MOD_NAME_INDEX_KEY),
    persistent_term:erase({wfcli, mod_map}),
    persistent_term:erase({wfcli, mod_name_index}),
    ok.

load_language_map() ->
    case find_lang_file() of
        {ok, Path} ->
            read_lang_file(Path);
        error ->
            case wfcli_worldstate:update_languages() of
                ok ->
                    case find_lang_file() of
                        {ok, Path2} -> read_lang_file(Path2);
                        _ -> warn_once(lang_map_missing), #{}
                    end;
                _ -> warn_once(lang_map_missing), #{}
            end
    end.

read_lang_file(Path) ->
    case file:read_file(Path) of
        {ok, Bin} ->
            try jsone:decode(Bin, [{object_format, map}]) of
                M when is_map(M) -> M;
                _ -> #{}
            catch _:_ -> #{}
            end;
        _ -> #{}
    end.

find_lang_file() ->
    freshest_metadata_path("languages.json").

load_node_map() ->
    case find_node_file() of
        {ok, Path} ->
            read_node_file(Path);
        error ->
            case wfcli_worldstate:update_nodes() of
                ok ->
                    case find_node_file() of
                        {ok, Path2} -> read_node_file(Path2);
                        _ -> warn_once(node_map_missing), #{}
                    end;
                _ -> warn_once(node_map_missing), #{}
            end
    end.

read_node_file(Path) ->
    case file:read_file(Path) of
        {ok, Bin} ->
            try jsone:decode(Bin, [{object_format, map}]) of
                M when is_map(M) -> M;
                _ -> #{}
            catch _:_ -> #{}
            end;
        _ -> #{}
    end.

find_node_file() ->
    freshest_metadata_path("solNodes.json").

freshest_metadata_path(Name) ->
    case lists:filter(fun filelib:is_file/1, wfcli_worldstate:metadata_paths(Name)) of
        [] -> error;
        [First | Rest] ->
            Path = lists:foldl(fun newer_path/2, First, Rest),
            {ok, Path}
    end.

newer_path(Path, Current) ->
    case filelib:last_modified(Path) > filelib:last_modified(Current) of
        true -> Path;
        false -> Current
    end.

load_item_map() ->
    case ensure_exports_available() of
        ok -> ok;
        {error, _} -> ok
    end,
    Files = existing_exports(),
    lists:foldl(fun add_export_names/2, #{}, Files).

load_mod_caches() ->
    case ensure_exports_available() of
        ok -> ok;
        {error, _} -> ok
    end,
    Path = export_path("ExportUpgrades_en.json"),
    case file:read_file(Path) of
        {ok, Bin} ->
            case jsone:decode(Bin, [{object_format, map}]) of
                M when is_map(M) ->
                    List = maps:get(<<"ExportUpgrades">>, M, []),
                    fold_mod_caches(List);
                _ ->
                    case jsone:decode(Bin, [{object_format, proplist}]) of
                        L when is_list(L) ->
                            case proplists:get_value(<<"ExportUpgrades">>, L, undefined) of
                                Mods when is_list(Mods) -> fold_mod_caches(Mods);
                                _ -> {#{}, #{}}
                            end;
                        _ -> {#{}, #{}}
                    end
            end;
        _ -> {#{}, #{}}
    end.

fold_mod_caches(List) when is_list(List) ->
    lists:foldl(fun add_mod_entries/2, {#{}, #{}}, List);
fold_mod_caches(_) -> {#{}, #{}}.

add_mod_entries(Entry, {ModAcc, NameAcc}) when is_map(Entry) ->
    case maps:get(<<"uniqueName">>, Entry, undefined) of
        undefined -> {ModAcc, NameAcc};
        Key ->
            Details = mod_details_from_entry(Entry),
            ModAcc1 = case Details of
                #{} -> maps:put(to_bin(Key), Details, ModAcc);
                _ -> ModAcc
            end,
            NameAcc1 = add_mod_name_entry(Entry, NameAcc),
            {ModAcc1, NameAcc1}
    end;
add_mod_entries(_, Acc) -> Acc.

add_mod_name_entry(Entry, Acc) when is_map(Entry) ->
    Name = maps:get(<<"name">>, Entry, undefined),
    case normalize_mod_name(Name) of
        undefined -> Acc;
        Key ->
            Pol = polarity_from_export(maps:get(<<"polarity">>, Entry, undefined)),
            Cost = cost_from(maps:get(<<"baseDrain">>, Entry, undefined),
                             maps:get(<<"fusionLimit">>, Entry, undefined)),
            Unique = maps:get(<<"uniqueName">>, Entry, undefined),
            Exclude = maps:get(<<"excludeFromCodex">>, Entry, false),
            Entry1 = #{polarity => Pol, cost => Cost, unique => Unique, exclude_from_codex => Exclude},
            Existing = maps:get(Key, Acc, []),
            maps:put(Key, [Entry1 | Existing], Acc)
    end;
add_mod_name_entry(_, Acc) -> Acc.

normalize_mod_name(undefined) -> undefined;
normalize_mod_name(null) -> undefined;
normalize_mod_name(Bin) when is_binary(Bin) ->
    string:lowercase(binary_to_list(Bin));
normalize_mod_name(Str) when is_list(Str) ->
    string:lowercase(Str);
normalize_mod_name(Atom) when is_atom(Atom) ->
    string:lowercase(atom_to_list(Atom));
normalize_mod_name(_) -> undefined.

lower_key_map(Map) ->
    maps:fold(
      fun(Key, Val, Acc) ->
          KeyList = retinue_text:to_list(Key),
          LowerKey = retinue_text:to_binary(string:lowercase(KeyList)),
          maps:put(LowerKey, Val, Acc)
      end,
      #{},
      Map).

polarity_from_export(<<"AP_ATTACK">>) -> madurai;
polarity_from_export(<<"AP_DEFENSE">>) -> vazarin;
polarity_from_export(<<"AP_TACTIC">>) -> naramon;
polarity_from_export(<<"AP_POWER">>) -> zenurik;
polarity_from_export(<<"AP_WARD">>) -> unairu;
polarity_from_export(<<"AP_UMBRA">>) -> umbral;
polarity_from_export(<<"AP_PRECEPT">>) -> penjaga;
polarity_from_export(<<"AP_ANY">>) -> none;
polarity_from_export(<<"AP_UNIVERSAL">>) -> none;
polarity_from_export(Pol) -> wfcli_forma_model:normalize_polarity(Pol).

cost_from(undefined, _) -> undefined;
cost_from(Base, undefined) when is_integer(Base) -> abs(Base);
cost_from(Base, Limit) when is_integer(Base), is_integer(Limit) ->
    Max = case Base < 0 of
        true -> Base - Limit;
        false -> Base + Limit
    end,
    abs(Max);
cost_from(_, _) -> undefined.

mod_details_from_entry(Entry) ->
    Name = maps:get(<<"name">>, Entry, undefined),
    Rarity = maps:get(<<"rarity">>, Entry, undefined),
    Polarity = maps:get(<<"polarity">>, Entry, undefined),
    BaseDrain = maps:get(<<"baseDrain">>, Entry, undefined),
    FusionLimit = maps:get(<<"fusionLimit">>, Entry, undefined),
    Type = maps:get(<<"type">>, Entry, undefined),
    Compat = maps:get(<<"compatName">>, Entry, undefined),
    LevelStats = maps:get(<<"levelStats">>, Entry, undefined),
    #{name => Name,
      rarity => Rarity,
      polarity => Polarity,
      base_drain => BaseDrain,
      fusion_limit => FusionLimit,
      type => Type,
      compat => Compat,
      level_stats => LevelStats}.

ensure_exports_available() ->
    Missing = [F || F <- wfcli_worldstate:resolver_export_files(), not filelib:is_file(export_path(F))],
    case Missing of
        [] -> ok;
        _ ->
            Results = [wfcli_worldstate:update_export(F) || F <- Missing],
            case lists:any(fun(R) -> R =/= ok end, Results) of
                true -> {error, fetch_failed};
                false -> ok
            end
    end.

existing_exports() ->
    [export_path(F) || F <- wfcli_worldstate:resolver_export_files(), filelib:is_file(export_path(F))].

export_path(File) ->
    Paths = wfcli_worldstate:metadata_paths(File),
    Existing = [P || P <- Paths, filelib:is_file(P)],
    case Existing of
        [Path | _] -> Path;
        [] ->
            case Paths of
                [Path | _] -> Path;
                [] -> wfcli_paths:cache_file(File)
            end
    end.

add_export_names(Path, Acc) ->
    case file:read_file(Path) of
        {ok, Bin} ->
            case jsone:decode(Bin, [{object_format, map}]) of
                M when is_map(M) -> merge_export_map(M, Acc);
                _ ->
                    case jsone:decode(Bin, [{object_format, proplist}]) of
                        List when is_list(List) -> merge_export_list(List, Acc);
                        _ -> Acc
                    end
            end;
        _ -> Acc
    end.

merge_export_map(Map, Acc) ->
    lists:foldl(
      fun({K, V}, A) when is_binary(K), is_map(V) ->
              maybe_put_item(K, V, A);
         ({_K, V}, A) when is_list(V) ->
              merge_export_list(V, A);
         (_, A) -> A
      end, Acc, maps:to_list(Map)).

merge_export_list(List, Acc) when is_list(List) ->
    lists:foldl(
      fun(V, A) when is_map(V) ->
              case maps:get(<<"uniqueName">>, V, undefined) of
                  undefined -> A;
                  K -> maybe_put_item(K, V, A)
              end;
         (_, A) -> A
      end, Acc, List);
merge_export_list(_, Acc) -> Acc.

maybe_put_item(K, V, Acc) ->
    Name = first_present(?ITEM_KEYS, V),
    case Name of
        undefined -> Acc;
        <<>> -> Acc;
        N -> maps:put(to_bin(K), N, Acc)
    end.

first_present([], _Map) -> undefined;
first_present([Key | Rest], Map) ->
    case maps:get(Key, Map, undefined) of
        undefined -> first_present(Rest, Map);
        <<>> -> first_present(Rest, Map);
        Val -> Val
    end.

to_bin(V) ->
    retinue_text:to_binary(V).

warn_once(Key) ->
    case persistent_term:get({wfcli, warn, Key}, false) of
        true -> ok;
        false ->
            persistent_term:put({wfcli, warn, Key}, true),
            logger:warning("~p cache unavailable; using raw identifiers", [Key])
    end.
