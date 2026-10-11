%%%-------------------------------------------------------------------
%% Export file loader helpers.
%%%-------------------------------------------------------------------
-module(wf_exports).

-export([load_mods/1, load_items/2, load_raw_items/2, mod_source/1, item_sources/2,
         default_item_exports/0]).
-import(retinue_text, [to_list/1]).

-define(ITEM_KEYS, [<<"displayName">>, <<"name">>, <<"locName">>, <<"ItemName">>]).

load_mods(ExportsDir) ->
    Path = mod_source(ExportsDir),
    case read_json(Path) of
        {ok, Map} ->
            Mods = export_list(Map, "ExportUpgrades"),
            {ok, [normalize_mod_entry(M) || M <- Mods, is_map(M)]};
        {error, Reason} -> {error, {export_unavailable, "ExportUpgrades_en.json", Path, Reason}}
    end.

load_items(ExportsDir, Files0) ->
    Sources = item_sources(ExportsDir, Files0),
    load_item_sources(Sources, []).

-spec load_raw_items(file:filename_all() | undefined, string()) ->
    {ok, [map()]} | {error, term()}.
load_raw_items(ExportsDir, File) ->
    Path = export_path(File, ExportsDir),
    case read_json(Path) of
        {ok, Map} -> {ok, export_list(Map, export_list_key(File))};
        {error, Reason} -> {error, {export_unavailable, File, Path, Reason}}
    end.

-doc "Resolve mod export source path for daemon cache signatures.".
-spec mod_source(file:filename_all() | undefined) -> file:filename_all().
mod_source(ExportsDir) ->
    export_path("ExportUpgrades_en.json", ExportsDir).

-doc "Resolve normalized item export names and paths for daemon cache signatures.".
-spec item_sources(file:filename_all() | undefined, [string()]) ->
    [{string(), file:filename_all()}].
item_sources(ExportsDir, Files0) ->
    Files = case Files0 of
        [] -> default_item_exports();
        _ -> [normalize_file_name(F) || F <- Files0]
    end,
    [{File, export_path(File, ExportsDir)} || File <- Files].

load_item_sources([], Acc) ->
    {ok, lists:flatten(lists:reverse(Acc))};
load_item_sources([{File, Path} | Rest], Acc) ->
    case read_json(Path) of
        {ok, Map} ->
            Key = export_list_key(File),
            List = export_list(Map, Key),
            Items = [item_entry(File, V) || V <- List, is_map(V)],
            load_item_sources(Rest, [Items | Acc]);
        {error, Reason} ->
            {error, {export_unavailable, File, Path, Reason}}
    end.

item_entry(File, V) ->
    Base = #{
        name => item_name(V),
        uniqueName => get_string(V, <<"uniqueName">>),
        file => normalize_file_name(File),
        category => export_category(File),
        description => get_text(V, <<"description">>),
        codexSecret => get_bool(V, <<"codexSecret">>),
        excludeFromCodex => get_bool(V, <<"excludeFromCodex">>),
        productCategory => get_string(V, <<"productCategory">>),
        masteryReq => get_int(V, <<"masteryReq">>),
        totalDamage => get_number(V, <<"totalDamage">>),
        criticalChance => get_number(V, <<"criticalChance">>),
        criticalMultiplier => get_number(V, <<"criticalMultiplier">>),
        procChance => get_number(V, <<"procChance">>),
        armor => get_number(V, <<"armor">>),
        health => get_number(V, <<"health">>),
        shield => get_number(V, <<"shield">>),
        sprintSpeed => get_number(V, <<"sprintSpeed">>),
        power => get_number(V, <<"power">>),
        stamina => get_number(V, <<"stamina">>)
    },
    maps:merge(Base, file_fields(normalize_file_name(File), V)).

file_fields("ExportWeapons_en.json", V) ->
    #{
        slot => get_int(V, <<"slot">>),
        trigger => get_text(V, <<"trigger">>),
        fireRate => get_number(V, <<"fireRate">>),
        magazineSize => get_number(V, <<"magazineSize">>),
        reloadTime => get_number(V, <<"reloadTime">>),
        accuracy => get_number(V, <<"accuracy">>),
        noise => get_text(V, <<"noise">>),
        damagePerShot => get_number(V, <<"damagePerShot">>)
    };
file_fields("ExportWarframes_en.json", V) ->
    Abilities = ability_names(maps:get(<<"abilities">>, V, [])),
    #{
        power => get_number(V, <<"power">>),
        stamina => get_number(V, <<"stamina">>),
        abilities => Abilities,
        abilitiesCount => length(Abilities)
    };
file_fields("ExportRecipes_en.json", V) ->
    #{
        buildTime => get_number(V, <<"buildTime">>),
        buildPrice => get_number(V, <<"buildPrice">>),
        skipBuildTimePrice => get_number(V, <<"skipBuildTimePrice">>),
        resultType => get_text(V, <<"resultType">>),
        ingredientsCount => list_count(maps:get(<<"ingredients">>, V, [])),
        secretIngredientsCount => list_count(maps:get(<<"secretIngredients">>, V, [])),
        resultCount => get_number(V, <<"num">>),
        consumeOnUse => get_text(V, <<"consumeOnUse">>)
    };
file_fields("ExportRegions_en.json", V) ->
    #{
        systemName => get_text(V, <<"systemName">>),
        systemIndex => get_number(V, <<"systemIndex">>),
        nodeType => get_number(V, <<"nodeType">>),
        missionIndex => get_number(V, <<"missionIndex">>),
        factionIndex => get_number(V, <<"factionIndex">>),
        minEnemyLevel => get_number(V, <<"minEnemyLevel">>),
        maxEnemyLevel => get_number(V, <<"maxEnemyLevel">>)
    };
file_fields("ExportDrones_en.json", V) ->
    #{
        durability => get_number(V, <<"durability">>),
        fillRate => get_number(V, <<"fillRate">>),
        repairRate => get_number(V, <<"repairRate">>)
    };
file_fields("ExportFusionBundles_en.json", V) ->
    #{fusionPoints => get_number(V, <<"fusionPoints">>)};
file_fields(_, _) ->
    #{}.

export_category(File0) ->
    File = normalize_file_name(File0),
    Base0 = filename:basename(File, ".json"),
    Base1 = string:replace(Base0, "_en", "", all),
    string:replace(Base1, "Export", "", leading).

item_name(V) ->
    case first_present(?ITEM_KEYS, V) of
        undefined -> "";
        Val -> to_list(Val)
    end.

normalize_mod_entry(M) ->
    Desc = description_list(M),
    Stats = level_stats(M),
    Effects = all_stats(Stats),
    MaxStats = max_stats(Stats),
    #{
        name => get_string(M, <<"name">>),
        type => get_string(M, <<"type">>),
        polarity => get_string(M, <<"polarity">>),
        rarity => get_string(M, <<"rarity">>),
        baseDrain => maps:get(<<"baseDrain">>, M, undefined),
        fusionLimit => maps:get(<<"fusionLimit">>, M, undefined),
        compatName => get_string(M, <<"compatName">>),
        uniqueName => get_string(M, <<"uniqueName">>),
        description => Desc,
        effects => Effects,
        levelStats => Stats,
        max_stats => MaxStats
    }.

description_list(M) ->
    case maps:get(<<"description">>, M, undefined) of
        undefined -> [];
        Val when is_list(Val) -> [to_list(V) || V <- Val];
        Val -> [to_list(Val)]
    end.

level_stats(M) ->
    maps:get(<<"levelStats">>, M, []).

all_stats(Stats) when is_list(Stats) ->
    Raw = lists:flatten([stats_from_level(L) || L <- Stats]),
    unique_preserve([to_list(V) || V <- Raw, V =/= ""]);
all_stats(_) -> [].

stats_from_level(Level) ->
    case maps:get(<<"stats">>, Level, maps:get("stats", Level, [])) of
        List when is_list(List) -> List;
        _ -> []
    end.

unique_preserve(List) ->
    unique_preserve(List, #{}, []).

unique_preserve([], _Seen, Acc) ->
    lists:reverse(Acc);
unique_preserve([Val | Rest], Seen, Acc) ->
    Key = to_list(Val),
    case maps:is_key(Key, Seen) of
        true -> unique_preserve(Rest, Seen, Acc);
        false -> unique_preserve(Rest, Seen#{Key => true}, [Key | Acc])
    end.

max_stats(Stats) when is_list(Stats), Stats =/= [] ->
    Last = lists:last(Stats),
    case maps:get(<<"stats">>, Last, maps:get("stats", Last, [])) of
        List when is_list(List) -> [to_list(V) || V <- List];
        _ -> []
    end;
max_stats(_) -> [].

read_json(Path) ->
    case file:read_file(Path) of
        {ok, Bin} ->
            try jsone:decode(Bin, [{object_format, map}]) of
                Map when is_map(Map) -> {ok, Map};
                _ -> {error, invalid_payload}
            catch _:_ -> {error, invalid_json}
            end;
        Error -> Error
    end.

export_list(Map, Key) when is_binary(Key) ->
    maps:get(Key, Map, []);
export_list(Map, Key) ->
    export_list(Map, list_to_binary(Key)).

export_list_key(File) ->
    Base = filename:basename(File, ".json"),
    Base1 = string:replace(Base, "_en", "", all),
    Base1.

normalize_file_name(File) ->
    Base = filename:basename(File),
    case string:find(Base, ".json") of
        nomatch -> Base ++ ".json";
        _ -> Base
    end.

export_path(File, undefined) ->
    Paths = wf_worldstate:metadata_paths(File),
    Fallback = case Paths of
                   [Preferred | _] -> Preferred;
                   [] -> wf_paths:cache_file(File)
               end,
    pick_existing(Paths, Fallback);
export_path(File, Dir) ->
    filename:join(Dir, File).

pick_existing([], Fallback) -> Fallback;
pick_existing([Path | Rest], Fallback) ->
    case filelib:is_file(Path) of
        true -> Path;
        false -> pick_existing(Rest, Fallback)
    end.

get_string(Map, Key) ->
    case maps:get(Key, Map, undefined) of
        undefined -> "";
        Val -> to_list(Val)
    end.

get_text(Map, Key) ->
    case maps:get(Key, Map, undefined) of
        undefined -> "";
        Val when is_list(Val) -> string:join([to_list(V) || V <- Val], " ");
        Val -> to_list(Val)
    end.

get_int(Map, Key) ->
    case maps:get(Key, Map, undefined) of
        Val when is_integer(Val) -> Val;
        _ -> undefined
    end.

get_number(Map, Key) ->
    case maps:get(Key, Map, undefined) of
        Val when is_integer(Val); is_float(Val) -> Val;
        _ -> undefined
    end.

get_bool(Map, Key) ->
    case maps:get(Key, Map, undefined) of
        Val when is_boolean(Val) -> Val;
        _ -> undefined
    end.

list_count(List) when is_list(List) -> length(List);
list_count(_) -> 0.

ability_names(Abilities) when is_list(Abilities) ->
    [ability_name(A) || A <- Abilities, ability_name(A) =/= ""];
ability_names(_) -> [].

ability_name(A) when is_map(A) ->
    get_text(A, <<"name">>);
ability_name(_) -> "".

first_present([], _Map) -> undefined;
first_present([Key | Rest], Map) ->
    case maps:get(Key, Map, undefined) of
        undefined -> first_present(Rest, Map);
        <<>> -> first_present(Rest, Map);
        Val -> Val
    end.

default_item_exports() ->
    [F || F <- wf_worldstate:resolver_export_files(), F =/= "ExportUpgrades_en.json"].
