-module(wfcli_stat_buildset).

-export([validate/1, resolve/3, summary/1, ordered_upgrades/1]).

validate(Options) when is_map(Options) ->
    Allowed = [<<"target">>, <<"buildset">>, <<"bounds">>, <<"armor">>, <<"effects">>],
    Builds = maps:get(<<"buildset">>, Options, #{}),
    Armor = maps:get(<<"armor">>, Options, 0),
    Valid = maps:keys(maps:without(Allowed, Options)) =:= [] andalso
        text(maps:get(<<"target">>, Options, undefined)) andalso
        is_map(Builds) andalso map_size(Builds) > 0 andalso map_size(Builds) =< 16 andalso
        maps:is_key(maps:get(<<"target">>, Options), Builds) andalso
        lists:all(fun({Slot, Build}) -> text(Slot) andalso member(Build) end,
                  maps:to_list(Builds)) andalso
        is_boolean(maps:get(<<"bounds">>, Options, false)) andalso
        is_integer(Armor) andalso number_between(Armor, 0, 10000000) andalso
        effects(maps:get(<<"effects">>, Options, #{})),
    case Valid of true -> ok; false -> {error, invalid_stat_request} end;
validate(_) -> {error, invalid_stat_request}.

member(#{<<"item">> := Item} = Request) ->
    text(Item) andalso
        maps:keys(maps:without([<<"item">>, <<"config">>, <<"unmodded">>], Request)) =:= [] andalso
        is_boolean(maps:get(<<"unmodded">>, Request, false)) andalso
        optional(<<"config">>, Request, fun index/1) andalso
        not (maps:get(<<"unmodded">>, Request, false) andalso maps:is_key(<<"config">>, Request));
member(#{<<"build">> := Build} = Request) ->
    map_size(Request) =:= 1 andalso normalized(Build);
member(_) -> false.

normalized(#{<<"definition_id">> := Type, <<"class">> := Class,
             <<"config">> := Config} = Build) ->
    text(Type) andalso text(Class) andalso config(Config) andalso
        optional(<<"name">>, Build, fun text/1) andalso
        optional(<<"skill_tree">>, Build, fun is_binary/1) andalso
        optional(<<"features">>, Build, fun features/1) andalso
        optional(<<"xp">>, Build, fun(V) -> is_integer(V) andalso V >= 0 end) andalso
        optional(<<"rank">>, Build, fun(V) -> Class =:= <<"warframe">> andalso
                            is_integer(V) andalso V >= 0 andalso V =< 30 end) andalso
        optional(<<"unknown_feature_flags">>, Build, fun(V) -> is_integer(V) andalso V >= 0 end) andalso
        optional(<<"shard_slots">>, Build, fun shards/1) andalso
        optional(<<"topology">>, Build, fun topology/1);
normalized(_) -> false.

features(Features) when is_map(Features) -> lists:all(fun is_boolean/1, maps:values(Features));
features(_) -> false.

config(#{<<"upgrade_slots">> := Slots} = Config) when is_list(Slots), length(Slots) =< 64 ->
    optional(<<"config_index">>, Config, fun index/1) andalso
        optional(<<"ability_ranks">>, Config, fun ability_ranks/1) andalso
        lists:all(fun(#{<<"topology_slot">> := Slot, <<"item_type">> := Type} = Upgrade) ->
                          text(Slot) andalso text(Type) andalso
                              optional(<<"name">>, Upgrade, fun text/1);
                     (_) -> false
                  end, Slots);
config(_) -> false.

ability_ranks(Ranks) when is_map(Ranks), map_size(Ranks) =< 4 ->
    lists:all(fun({Type, Rank}) -> text(Type) andalso is_integer(Rank) andalso
                                   Rank >= 0 andalso Rank =< 3 end, maps:to_list(Ranks));
ability_ranks(_) -> false.

shards(Shards) when is_list(Shards), length(Shards) =< 5 ->
    lists:all(fun(#{<<"slot_id">> := Id, <<"upgrade">> := #{<<"UpgradeType">> := Type}}) ->
                      text(Id) andalso text(Type);
                 (_) -> false
              end, Shards) andalso
        length(lists:usort([maps:get(<<"slot_id">>, S) || S <- Shards])) =:= length(Shards);
shards(_) -> false.

topology(#{<<"regions">> := Regions}) when is_list(Regions), length(Regions) =< 16 ->
    lists:all(fun(#{<<"slots">> := Slots}) when is_list(Slots), length(Slots) =< 64 ->
                      lists:all(fun(#{<<"id">> := Id}) -> text(Id); (_) -> false end, Slots);
                 (_) -> false
              end, Regions);
topology(_) -> false.

effects(Effects) when is_map(Effects), map_size(Effects) =< 128 ->
    lists:all(fun({Key, Value}) -> text(Key) andalso is_integer(Value) andalso
                                  number_between(Value, 0, 10000) end, maps:to_list(Effects));
effects(_) -> false.
text(Value) -> is_binary(Value) andalso byte_size(Value) > 0 andalso byte_size(Value) =< 256.
index(Value) -> is_integer(Value) andalso number_between(Value, 0, 99).
number_between(Value, Min, Max) -> is_number(Value) andalso Value >= Min andalso Value =< Max.
optional(Key, Map, Predicate) ->
    case maps:find(Key, Map) of error -> true; {ok, Value} -> Predicate(Value) end.

resolve(Requests, Equipment, Weapons) ->
    Builds = maps:map(fun(_, Request) -> resolve_member(Request, Equipment, Weapons) end, Requests),
    ensure(length([I || I = #{<<"class">> := <<"warframe">>} <- maps:values(Builds)]) =< 1,
           multiple_buildset_warframes),
    Ids = [Id || #{<<"instance_id">> := Id} <- maps:values(Builds), Id =/= null],
    ensure(length(Ids) =:= length(lists:usort(Ids)), duplicate_buildset_instance),
    Builds.

resolve_member(#{<<"build">> := Build}, _Equipment, Weapons) ->
    Type = maps:get(<<"definition_id">>, Build),
    Names = [maps:get(<<"name">>, W) || W <- Weapons, maps:get(<<"uniqueName">>, W) =:= Type],
    Name = case Names of [N] -> N; _ -> maps:get(<<"name">>, Build, filename:basename(Type)) end,
    Base = wfcli_build_topology:definition(maps:get(<<"class">>, Build), #{}),
    maps:merge(Base, Build#{<<"name">> => Name, <<"instance_id">> => null});
resolve_member(#{<<"item">> := Selector, <<"unmodded">> := true}, _Equipment, Weapons) ->
    Export = one([W || W <- Weapons, matches(Selector, W)],
                 {weapon_not_found_or_ambiguous, Selector}),
    Type = maps:get(<<"uniqueName">>, Export),
    Class = case maps:get(<<"productCategory">>, Export, undefined) of
        <<"Suits">> -> <<"warframe">>;
        <<"LongGuns">> -> <<"primary">>;
        <<"Pistols">> -> <<"secondary">>;
        <<"Melee">> -> <<"melee">>;
        Other ->
            case wfcli_stat_profiles:weapon(Type) of
                #{class := Known} -> Known;
                _ -> fail({unsupported_equipment_class, Other})
            end
    end,
    Build = #{<<"definition_id">> => Type, <<"class">> => Class,
              <<"config">> => #{<<"upgrade_slots">> => []}},
    Ranked = case Class of <<"warframe">> -> Build#{<<"rank">> => 30}; _ -> Build end,
    resolve_member(#{<<"build">> => Ranked}, #{}, Weapons);
resolve_member(#{<<"item">> := Selector} = Request, Equipment, _Weapons) ->
    Definitions = maps:from_list([{maps:get(<<"id">>, D), D}
                                  || D <- maps:get(<<"definitions">>, Equipment, [])]),
    Candidates = [I#{<<"name">> => maps:get(<<"name">>, D, maps:get(<<"definition_id">>, I))}
                   || I <- maps:get(<<"instances">>, Equipment, []),
                      D <- [maps:get(maps:get(<<"definition_id">>, I), Definitions, #{})],
                      Selector =:= maps:get(<<"instance_id">>, I) orelse matches(Selector, D)],
    Item = case Candidates of
        [] -> fail({equipment_not_found, Selector});
        [One] -> One;
        _ -> fail({ambiguous_equipment, Selector, [maps:get(<<"instance_id">>, I) || I <- Candidates]})
    end,
    Index = case maps:get(<<"config">>, Request, active) of
        active ->
            case maps:get(<<"active_config_indices">>, Item, []) of
                [Active] -> Active;
                _ -> fail({config_required, Selector})
            end;
        Explicit -> Explicit
    end,
    Config = one([C || C <- maps:get(<<"configs">>, Item, []),
                       maps:get(<<"config_index">>, C) =:= Index],
                 {config_not_found, Selector, Index}),
    Item#{<<"config">> => Config}.

ordered_upgrades(Item) ->
    Slots = maps:get(<<"upgrade_slots">>, maps:get(<<"config">>, Item), []),
    Regions = maps:get(<<"regions">>, maps:get(<<"topology">>, Item, #{}), []),
    Order = [maps:get(<<"id">>, S) || R <- Regions, S <- maps:get(<<"slots">>, R)],
    Positions = maps:from_list([{Id, N} || {N, Id} <- lists:enumerate(Order)]),
    Ids = [maps:get(<<"topology_slot">>, S, null) || S <- Slots],
    ensure(length(Order) =:= map_size(Positions) andalso
           length(Ids) =:= length(lists:usort(Ids)) andalso
           lists:all(fun(Id) -> maps:is_key(Id, Positions) end, Ids), invalid_build_topology),
    lists:sort(fun(A, B) -> maps:get(maps:get(<<"topology_slot">>, A), Positions) <
                           maps:get(maps:get(<<"topology_slot">>, B), Positions) end, Slots).

summary(Item) ->
    Config = maps:get(<<"config">>, Item),
    #{name => maps:get(<<"name">>, Item), definition_id => maps:get(<<"definition_id">>, Item),
      class => maps:get(<<"class">>, Item), instance_id => maps:get(<<"instance_id">>, Item, null),
      config_index => maps:get(<<"config_index">>, Config, null),
      config_fingerprint => maps:get(<<"fingerprint">>, Config, null)}.

matches(Query, Record) ->
    Query =:= maps:get(<<"uniqueName">>, Record, null) orelse
        Query =:= maps:get(<<"id">>, Record, null) orelse
        string:casefold(Query) =:= string:casefold(maps:get(<<"name">>, Record, <<>>)).
one([One], _) -> One;
one(_, Reason) -> fail(Reason).
ensure(true, _) -> ok;
ensure(false, Reason) -> fail(Reason).
fail(Reason) -> throw({stats, Reason}).
