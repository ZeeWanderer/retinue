%%%-------------------------------------------------------------------
%% Unified resolver entry for worldstate identifiers.
%%%-------------------------------------------------------------------
-module(wf_resolve).

-export([resolve/3, resolve_description/2]).

-doc "Resolve localization description metadata while preserving raw-mode behavior.".
-spec resolve_description(term(), map()) -> string().
resolve_description(V, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        false -> "";
        true -> language_description(V)
    end.

resolve(Key0, V, Opts) ->
    Key = normalize_key(Key0),
    case Key of
        "missiontype" -> resolve_mission_type(V, Opts);
        "modifier" -> resolve_modifier(V, Opts);
        "activemissiontier" -> resolve_modifier(V, Opts);
        "node" -> resolve_node(V, Opts);
        "faction" -> resolve_faction(V, Opts);
        "season" -> resolve_season(V, Opts);
        "item" -> resolve_item_name(V, Opts);
        "dt" -> resolve_dt_tag(V, Opts);
        "any" -> resolve_any(V, Opts);
        _ -> resolve_any(V, Opts)
    end.

resolve_any(V, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        false -> retinue_text:to_list(V);
        true ->
            case language_name(V) of
                undefined ->
                    Value = retinue_text:to_list(V),
                    case {lists:prefix("MT_", string:uppercase(Value)), lists:prefix("voidt", normalize_key(Value))} of
                        {true, _} -> mission_type_name(string:uppercase(Value));
                        {_, true} -> fissure_tier_name("VoidT" ++ lists:nthtail(5, normalize_key(Value)));
                        _ -> resolve_item_name(V, Opts)
                    end;
                Name -> Name
            end
    end.

resolve_mission_type(V, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        false -> retinue_text:to_list(V);
        true ->
            case language_name(V) of
                undefined ->
                    Value = retinue_text:to_list(V),
                    mission_type_name(normalize_mission_type(Value));
                Name -> Name
            end
    end.

resolve_modifier(V, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        false -> retinue_text:to_list(V);
        true ->
            Value = retinue_text:to_list(V),
            fissure_tier_name(normalize_fissure_tier(Value))
    end.

resolve_node(V, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        true ->
            case node_name(V) of
                undefined -> retinue_text:to_list(V);
                Name -> Name
            end;
        false -> retinue_text:to_list(V)
    end.

resolve_faction(V, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        false -> retinue_text:to_list(V);
        true ->
            Map = faction_map(),
            case map_get_ci_simple(V, Map) of
                undefined -> retinue_text:to_list(V);
                Name -> Name
            end
    end.

resolve_item_name(V, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        true ->
            case language_name(V) of
                undefined ->
                    case item_name(V) of
                        undefined -> retinue_text:to_list(V);
                        Name -> Name
                    end;
                Name -> Name
            end;
        false -> retinue_text:to_list(V)
    end.

resolve_season(V, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        false -> retinue_text:to_list(V);
        true ->
            case language_name(V) of
                undefined ->
                    Value = retinue_text:to_list(V),
                    season_code_name(normalize_season_code(Value));
                Name -> Name
            end
    end.

resolve_dt_tag(V, Opts) ->
    Value = retinue_text:to_list(V),
    case maps:get(raw, Opts, false) of
        true -> Value;
        false ->
            Upper = string:uppercase(Value),
            case lists:prefix("DT_", Upper) of
                true -> dt_humanize(Upper);
                false -> resolve_any(Value, Opts)
            end
    end.

dt_humanize("DT_" ++ Rest) ->
    Words = string:tokens(string:lowercase(Rest), "_"),
    join_list([capitalize_word(W) || W <- Words], " ");
dt_humanize(Value) ->
    Value.

capitalize_word("") -> "";
capitalize_word([C | Rest]) ->
    string:uppercase([C]) ++ Rest.

join_list([], _Sep) -> "";
join_list([One], _Sep) -> One;
join_list([H | T], Sep) -> H ++ Sep ++ join_list(T, Sep).

mission_type_name(Type) ->
    maps:get(Type, mission_type_map(), Type).

%% Worldstate uses internal mission enums rather than localization keys.
mission_type_map() ->
    #{
      "MT_ALCHEMY" => "Alchemy",
      "MT_ARMAGEDDON" => "Void Armageddon",
      "MT_ARTIFACT" => "Disruption",
      "MT_ASCENSION" => "Ascension",
      "MT_ASSAULT" => "Assault",
      "MT_ASSASSINATION" => "Assassination",
      "MT_ARENA" => "Rathuum",
      "MT_CAPTURE" => "Capture",
      "MT_CORRUPTION" => "Void Flood",
      "MT_COUNTER_INTEL" => "Deception",
      "MT_DEFAULT" => "Unknown",
      "MT_DEFENSE" => "Defense",
      "MT_DESCENT" => "The Descendia",
      "MT_ENDLESS_CAPTURE" => "Legacyte Harvest",
      "MT_ENDLESS_DUVIRI" => "The Circuit",
      "MT_ENDLESS_EXTERMINATION" => "Sanctuary Onslaught",
      "MT_EVACUATION" => "Defection",
      "MT_EXCAVATE" => "Excavation",
      "MT_EXTERMINATION" => "Exterminate",
      "MT_HIVE" => "Hive Sabotage",
      "MT_INTEL" => "Spy",
      "MT_LANDSCAPE" => "Free Roam",
      "MT_MOBILE_DEFENSE" => "Mobile Defense",
      "MT_OFFERING" => "Shrine Defense",
      "MT_PURIFY" => "Infested Salvage",
      "MT_PVP" => "Conclave",
      "MT_PVPVE" => "Faceoff",
      "MT_RACE" => "Rush",
      "MT_RAILJACK" => "Skirmish",
      "MT_RESCUE" => "Rescue",
      "MT_RETRIEVAL" => "Hijack",
      "MT_SABOTAGE" => "Sabotage",
      "MT_SECTOR" => "Solar Rail Conflict",
      "MT_SURVIVAL" => "Survival",
      "MT_TERRITORY" => "Interception",
      "MT_VAULTS" => "Netracells",
      "MT_VOID_CASCADE" => "Void Cascade"
     }.

fissure_tier_name("VoidT1") -> "Lith";
fissure_tier_name("VoidT2") -> "Meso";
fissure_tier_name("VoidT3") -> "Neo";
fissure_tier_name("VoidT4") -> "Axi";
fissure_tier_name("VoidT5") -> "Requiem";
fissure_tier_name("VoidT6") -> "Omnia";
fissure_tier_name(Other) -> Other.

season_code_name("CST_SPRING") -> "Spring";
season_code_name("CST_SUMMER") -> "Summer";
season_code_name("CST_FALL") -> "Fall";
season_code_name("CST_AUTUMN") -> "Autumn";
season_code_name("CST_WINTER") -> "Winter";
season_code_name(Other) -> Other.

faction_map() ->
    #{
      <<"fc_grineer">> => "Grineer",
      <<"fc_corpus">> => "Corpus",
      <<"fc_infestation">> => "Infestation",
      <<"fc_corrupted">> => "Corrupted",
      <<"fc_orokin">> => "Orokin",
      <<"fc_sentient">> => "Sentient",
      <<"fc_scaldra">> => "Scaldra",
      <<"fc_techrot">> => "Techrot",
      <<"fc_mitw">> => "Man in the Wall"
     }.

language_name(V) ->
    Map = wf_resolve_registry:language_map(),
    case map_get_ci(V, Map) of
        undefined -> undefined;
        #{<<"value">> := Val} -> retinue_text:to_list(Val);
        Val when is_binary(Val) -> retinue_text:to_list(Val);
        _ -> undefined
    end.

language_description(V) ->
    Map = wf_resolve_registry:language_map(),
    case map_get_ci(V, Map) of
        #{<<"desc">> := Desc} -> retinue_text:to_list(Desc);
        _ -> ""
    end.

node_name(Node) ->
    Map = wf_resolve_registry:node_map(),
    case map_get_ci(Node, Map) of
        undefined -> maps:get(to_bin(normalize_key(Node)), node_name_fallbacks(), undefined);
        #{<<"value">> := Val} -> retinue_text:to_list(Val);
        Val when is_binary(Val) -> retinue_text:to_list(Val);
        _ -> undefined
    end.

node_name_fallbacks() ->
    #{<<"solnode801">> => "Sanctuary Onslaught",
      <<"solnode802">> => "Elite Sanctuary Onslaught"}.

item_name(V) ->
    Map = wf_resolve_registry:item_map(),
    KeyBin = to_bin(V),
    case item_name_lookup(KeyBin, Map) of
        undefined ->
            case item_name_fallback(KeyBin, Map) of
                undefined ->
                    case wf_builtin_metadata:equipment(KeyBin) of
                        #{name := BuiltinName} -> retinue_text:to_list(BuiltinName);
                        undefined -> language_item_name(KeyBin)
                    end;
                Name -> Name
            end;
        Name -> Name
    end.

item_name_lookup(KeyBin, Map) ->
    case map_get_ci(KeyBin, Map) of
        undefined -> undefined;
        Name when is_binary(Name) -> retinue_text:to_list(Name);
        Name when is_list(Name) -> Name;
        _ -> undefined
    end.

item_name_fallback(KeyBin, Map) ->
    Candidates = item_name_candidates(KeyBin),
    item_name_from_candidates(Candidates, Map).

item_name_from_candidates([], _Map) -> undefined;
item_name_from_candidates([Key | Rest], Map) ->
    case item_name_lookup(Key, Map) of
        undefined -> item_name_from_candidates(Rest, Map);
        Name -> Name
    end.

language_item_name(KeyBin) ->
    Map = wf_resolve_registry:language_map(),
    case map_get_ci(KeyBin, Map) of
        undefined -> undefined;
        #{<<"value">> := Val} -> retinue_text:to_list(Val);
        Val when is_binary(Val) -> retinue_text:to_list(Val);
        _ -> undefined
    end.

item_name_candidates(KeyBin) ->
    Key = retinue_text:to_list(KeyBin),
    Rewrites = [
        string:replace(Key, "/Lotus/StoreItems/Types/", "/Lotus/Types/", all),
        string:replace(Key, "/Lotus/StoreItems/", "/Lotus/", all),
        string:replace(Key, "/Lotus/Types/StoreItems/", "/Lotus/Types/", all),
        string:replace(Key, "/StoreItems/", "/", all)
    ],
    lists:reverse(
      lists:foldl(
        fun(Val, Acc) ->
            case Val of
                "" -> Acc;
                _ when Val =:= Key -> Acc;
                _ ->
                    Bin = to_bin(Val),
                    case lists:member(Bin, Acc) of
                        true -> Acc;
                        false -> [Bin | Acc]
                    end
            end
        end,
        [],
        Rewrites)).

to_bin(V) ->
    retinue_text:to_binary(V).

map_get_ci(Key, Map) ->
    KeyList = retinue_text:to_list(Key),
    KeyBin = retinue_text:to_binary(normalize_key(KeyList)),
    maps:get(KeyBin, Map, undefined).

map_get_ci_simple(Key, Map) ->
    KeyList = retinue_text:to_list(Key),
    KeyBin = retinue_text:to_binary(normalize_key(KeyList)),
    maps:get(KeyBin, Map, undefined).

normalize_mission_type(Value) ->
    Upper = string:uppercase(Value),
    case lists:prefix("MT_", Upper) of
        true -> Upper;
        false -> Value
    end.

normalize_fissure_tier(Value) ->
    Lower = normalize_key(Value),
    case lists:prefix("voidt", Lower) of
        true -> "VoidT" ++ lists:nthtail(5, Lower);
        false -> Value
    end.

normalize_season_code(Value) ->
    Upper = string:uppercase(Value),
    case lists:prefix("CST_", Upper) of
        true -> Upper;
        false -> Value
    end.

normalize_key(Value) ->
    string:lowercase(retinue_text:to_list(Value)).
