%%%-------------------------------------------------------------------
%% Canonical typed read model over the lossless player observation.
%%%-------------------------------------------------------------------
-module(wfcli_player_projection).

-export([build/1, from_observation/1, audit/1, summary/1]).

-define(SCHEMA_VERSION, 1).
-define(VIEW_CACHE, wfcli_player_view_cache).

-type origin() :: {binary(), [binary() | non_neg_integer()]}.
-type projection() :: map().

-doc "Build the typed player read model; raw observation remains the source of truth.".
-spec build(map()) -> projection().
build(Snapshot) ->
    Data = maps:get(data, Snapshot, #{}),
    Observation = maps:get(<<"inventory">>, Data, #{}),
    Key = {player_projection, maps:get(revision, Snapshot, 0),
           maps:get(updated_at, Snapshot, undefined), erlang:phash2(Observation),
           ?MODULE:module_info(md5), wfcli_player_items:module_info(md5),
           wfcli_player_schema:module_info(md5)},
    case cache_lookup(Key) of
        {ok, Projection} -> Projection;
        error ->
            Projection = from_observation(Observation),
            cache_store(Key, Projection),
            Projection
    end.

-doc "Build a projection directly from one inventory observation.".
-spec from_observation(map()) -> projection().
from_observation(Observation) when is_map(Observation) ->
    Raw = map_value(maps:get(<<"raw">>, Observation, #{})),
    Index = map_value(maps:get(<<"index">>, Observation, #{})),
    ItemProjection0 = wfcli_player_items:build(Raw),
    ItemProjection = legacy_items(ItemProjection0, Raw, Index),
    Profile = player_profile(Observation, Raw),
    ProfileEntity = profile_entity(Profile, Observation, Raw),
    Mastery = legacy_list(
                <<"mastery">>, mastery_records(Raw), Raw, [<<"XPInfo">>],
                Index, player_mastery),
    Recipes = legacy_list(
                <<"pending_recipes">>, recipe_records(Raw), Raw,
                [<<"PendingRecipes">>], Index, player_recipe),
    Missions = legacy_list(
                 <<"missions">>, mission_records(Raw), Raw, [<<"Missions">>],
                 Index, player_mission),
    Skills = player_skills(Raw, Index),
    SkillEntities = skill_records(Skills, Raw),
    Affiliations = affiliation_records(Raw),
    FocusUpgrades = focus_upgrade_records(Raw),
    FocusPools = focus_pool_records(Raw),
    Boosters = booster_records(Raw),
    Challenges = challenge_records(Raw),
    ProgressEntities = Mastery ++ Recipes ++ Missions ++ SkillEntities ++
                       Affiliations ++ FocusUpgrades ++ FocusPools ++
                       Boosters ++ Challenges,
    Entities0 = maps:get(entities, ItemProjection, []) ++ ProgressEntities,
    Entities = case ProfileEntity of
        undefined -> Entities0;
        _ -> [ProfileEntity | Entities0]
    end,
    RawEntities = complete_raw_entities(Entities, raw_entities(Observation, Raw)),
    ItemProjection#{
      <<"schema">> => ?SCHEMA_VERSION,
      <<"profile">> => Profile,
      <<"mastery">> => Mastery,
      <<"pending_recipes">> => Recipes,
      <<"missions">> => Missions,
      <<"player_skills">> => Skills,
      <<"affiliations">> => Affiliations,
      <<"focus_upgrades">> => FocusUpgrades,
      <<"focus_pools">> => FocusPools,
      <<"boosters">> => Boosters,
      <<"challenges">> => Challenges,
      <<"raw">> => Raw,
      entities => Entities,
      raw_entities => RawEntities,
      schema_issues => wfcli_player_schema:audit(Raw)};
from_observation(_Observation) ->
    from_observation(#{}).

-doc "Report shape drift in understood player records without rejecting the payload.".
-spec audit(map()) -> [map()].
audit(Snapshot) ->
    maps:get(schema_issues, build(Snapshot), []).

-doc "Return stable counts for status presentation without exposing a companion-built index.".
-spec summary(map()) -> map().
summary(Snapshot) ->
    Projection = build(Snapshot),
    #{schema => maps:get(<<"schema">>, Projection),
      equipment => length(maps:get(<<"equipment">>, Projection, [])),
      items => length(maps:get(<<"items">>, Projection, [])),
      stacks => length(maps:get(<<"stacks">>, Projection, [])),
      upgrades => length(maps:get(<<"upgrades">>, Projection, [])),
      configs => length(maps:get(<<"configs">>, Projection, [])),
      loadouts => length(maps:get(<<"loadouts">>, Projection, [])),
      mastery => length(maps:get(<<"mastery">>, Projection, [])),
      pending_recipes => length(maps:get(<<"pending_recipes">>, Projection, [])),
      schema_issues => length(maps:get(schema_issues, Projection, []))}.

player_profile(Observation, Raw) ->
    Captured = present_map(map_value(maps:get(<<"profile">>, Observation, #{}))),
    RawFields = lists:foldl(
      fun({RawKey, Key}, Acc) ->
          optional(Key, maps:get(RawKey, Raw, undefined), Acc)
      end,
      #{},
      profile_fields()),
    maps:merge(RawFields, Captured).

profile_entity(Profile, _Observation, _Raw) when map_size(Profile) =:= 0 -> undefined;
profile_entity(Profile, Observation, Raw) ->
    RawProfile = maps:with([RawKey || {RawKey, _} <- profile_fields()], Raw),
    Captured = map_value(maps:get(<<"profile">>, Observation, #{})),
    Data = case map_size(Captured) of
        0 -> RawProfile;
        _ -> RawProfile#{<<"captured">> => Captured}
    end,
    RawOrigins = [origin([RawKey])
                  || {RawKey, _} <- profile_fields(), maps:is_key(RawKey, Raw)],
    Origins = case map_size(Captured) of
        0 -> RawOrigins;
        _ -> [observation_origin([<<"profile">>]) | RawOrigins]
    end,
    Profile#{entity_type => player_profile,
             origin => first_origin(Origins, observation_origin([<<"profile">>])),
             origins => Origins,
             raw => Data,
             <<"id">> => <<"profile">>}.

profile_fields() ->
    [{<<"PlayerLevel">>, <<"player_level">>},
     {<<"RegularCredits">>, <<"regular_credits">>},
     {<<"PremiumCredits">>, <<"premium_credits">>},
     {<<"PremiumCreditsFree">>, <<"premium_credits_free">>},
     {<<"FusionPoints">>, <<"fusion_points">>},
     {<<"TradesRemaining">>, <<"trades_remaining">>},
     {<<"DailyFocus">>, <<"daily_focus">>},
     {<<"FocusCapacity">>, <<"focus_capacity">>},
     {<<"LastRegionPlayed">>, <<"last_region_played">>}].

mastery_records(Raw) ->
    records(<<"XPInfo">>, maps:get(<<"XPInfo">>, Raw, []), player_mastery,
      fun(Index, Item) ->
          ItemType = maps:get(<<"ItemType">>, Item, <<>>),
          #{<<"id">> => fallback_id(ItemType, <<"mastery">>, Index),
            <<"item_type">> => ItemType,
            <<"xp">> => integer(maps:get(<<"XP">>, Item, 0), 0)}
      end).

recipe_records(Raw) ->
    records(<<"PendingRecipes">>, maps:get(<<"PendingRecipes">>, Raw, []),
      player_recipe,
      fun(Index, Item) ->
          ItemType = maps:get(<<"ItemType">>, Item, <<>>),
          InstanceId = id_value(maps:get(<<"ItemId">>, Item, undefined)),
          Base = #{<<"id">> => fallback_id(InstanceId, <<"recipe">>, Index),
                   <<"item_type">> => ItemType,
                   <<"instance_id">> => nullable(InstanceId)},
          optional(<<"completion_date">>, maps:get(<<"CompletionDate">>, Item, undefined),
                   Base)
      end).

mission_records(Raw) ->
    records(<<"Missions">>, maps:get(<<"Missions">>, Raw, []), player_mission,
      fun(Index, Item) ->
          Tag = maps:get(<<"Tag">>, Item, <<>>),
          #{<<"id">> => fallback_id(Tag, <<"mission">>, Index),
            <<"tag">> => Tag,
            <<"completes">> => integer(maps:get(<<"Completes">>, Item, 0), 0),
            <<"tier">> => integer(maps:get(<<"Tier">>, Item, 0), 0)}
      end).

affiliation_records(Raw) ->
    records(<<"Affiliations">>, maps:get(<<"Affiliations">>, Raw, []),
      player_affiliation,
      fun(Index, Item) ->
          Tag = maps:get(<<"Tag">>, Item, <<>>),
          Base = #{<<"id">> => fallback_id(Tag, <<"affiliation">>, Index),
                   <<"tag">> => Tag,
                   <<"standing">> => integer(maps:get(<<"Standing">>, Item, 0), 0),
                   <<"title">> => integer(maps:get(<<"Title">>, Item, 0), 0)},
          copy_fields([{<<"initiated">>, <<"Initiated">>},
                       {<<"free_favors_earned">>, <<"FreeFavorsEarned">>},
                       {<<"free_favors_used">>, <<"FreeFavorsUsed">>},
                       {<<"weekly_missions">>, <<"WeeklyMissions">>}],
                      Item, Base)
      end).

focus_upgrade_records(Raw) ->
    records(<<"FocusUpgrades">>, maps:get(<<"FocusUpgrades">>, Raw, []),
      player_focus_upgrade,
      fun(Index, Item) ->
          ItemType = maps:get(<<"ItemType">>, Item, <<>>),
          Base = #{<<"id">> => fallback_id(ItemType, <<"focus-upgrade">>, Index),
                   <<"item_type">> => ItemType,
                   <<"level">> => integer(maps:get(<<"Level">>, Item, 0), 0),
                   <<"is_universal">> => maps:get(<<"IsUniversal">>, Item, false) =:= true},
          Base
      end).

focus_pool_records(Raw) ->
    case maps:get(<<"FocusXP">>, Raw, #{}) of
        Pools when is_map(Pools) ->
            [record(player_focus_pool, [<<"FocusXP">>, School],
                    #{School => Xp},
                    #{<<"id">> => School, <<"school">> => School,
                      <<"xp">> => integer(Xp, 0)})
             || {School, Xp} <- lists:sort(maps:to_list(Pools))];
        _ -> []
    end.

booster_records(Raw) ->
    records(<<"Boosters">>, maps:get(<<"Boosters">>, Raw, []), player_booster,
      fun(Index, Item) ->
          ItemType = maps:get(<<"ItemType">>, Item, <<>>),
          Base = #{<<"id">> => fallback_id(ItemType, <<"booster">>, Index),
                   <<"item_type">> => ItemType},
          optional(<<"expiry">>, maps:get(<<"ExpiryDate">>, Item, undefined), Base)
      end).

challenge_records(Raw) ->
    records(<<"ChallengeProgress">>, maps:get(<<"ChallengeProgress">>, Raw, []),
      player_challenge,
      fun(Index, Item) ->
          Name = maps:get(<<"Name">>, Item, <<>>),
          Base = #{<<"id">> => fallback_id(Name, <<"challenge">>, Index),
                   <<"name">> => Name,
                   <<"progress">> => integer(maps:get(<<"Progress">>, Item, 0), 0)},
          optional(<<"completed">>, maps:get(<<"Completed">>, Item, undefined), Base)
      end).

player_skills(Raw, Index) ->
    case maps:get(<<"PlayerSkills">>, Raw, undefined) of
        Skills when is_map(Skills) -> Skills;
        _ -> map_value(maps:get(<<"player_skills">>, Index, #{}))
    end.

skill_records(Skills, Raw) ->
    IsRaw = maps:is_key(<<"PlayerSkills">>, Raw),
    [begin
         Path = case IsRaw of
             true -> [<<"PlayerSkills">>, Skill];
             false -> [<<"index">>, <<"player_skills">>, Skill]
         end,
         record_at(player_skill, Path, IsRaw, #{Skill => Rank},
                   #{<<"id">> => Skill, <<"skill">> => Skill,
                     <<"rank">> => Rank})
     end
     || {Skill, Rank} <- lists:sort(maps:to_list(Skills)), is_integer(Rank)].

records(Collection, Values, Type, FieldsFun) when is_list(Values) ->
    [record(Type, [Collection, Index], Item, FieldsFun(Index, Item))
     || {Index, Item} <- lists:enumerate(0, Values), is_map(Item)];
records(_Collection, _Values, _Type, _FieldsFun) -> [].

record(Type, Path, Raw, Fields) ->
    Origin = origin(Path),
    Fields#{entity_type => Type, origin => Origin,
            origins => [Origin], raw => Raw,
            <<"collection">> => first_path(Path)}.

record_at(Type, Path, true, Raw, Fields) -> record(Type, Path, Raw, Fields);
record_at(Type, Path, false, Raw, Fields) ->
    Origin = observation_origin(Path),
    Fields#{entity_type => Type, origin => Origin,
            origins => [Origin], raw => Raw,
            <<"collection">> => first_path(Path)}.

legacy_items(Projection, Raw, Index) ->
    Equipment = legacy_list(<<"equipment">>, maps:get(<<"equipment">>, Projection),
                            Raw, wfcli_player_items:equipment_collections(),
                            Index, player_equipment),
    Stacks = legacy_list(<<"stacks">>, maps:get(<<"stacks">>, Projection),
                         Raw, stack_collections(), Index, player_stack),
    LegacyEntities = [Entity || Entity <- Equipment ++ Stacks,
                                not lists:member(Entity,
                                  maps:get(entities, Projection, []))],
    Projection#{<<"equipment">> => Equipment,
                <<"stacks">> => Stacks,
                entities => maps:get(entities, Projection, []) ++ LegacyEntities}.

legacy_list(Key, Current, Raw, RawKeys, Index, Type) ->
    case {Current, lists:any(fun(RawKey) -> maps:is_key(RawKey, Raw) end, RawKeys)} of
        {[], false} -> legacy_records(Key, maps:get(Key, Index, []), Type);
        _ -> Current
    end.

legacy_records(Group, Values, Type) when is_list(Values) ->
    [begin
         Origin = observation_origin([<<"index">>, Group, Index]),
         Id = maps:get(<<"instance_id">>, Item,
                       fallback_id(maps:get(<<"item_type">>, Item, undefined),
                                   Group, Index)),
         Item#{entity_type => Type, origin => Origin, origins => [Origin],
               raw => Item, <<"id">> => Id}
     end
     || {Index, Item} <- lists:enumerate(0, Values), is_map(Item)];
legacy_records(_Group, _Values, _Type) -> [].

raw_entities(Observation, Raw) ->
    Envelope = maps:without([<<"raw">>, <<"index">>], Observation),
    EnvelopeEntities = [raw_entity(observation_origin([Key]), Key, Value)
                        || {Key, Value} <- lists:sort(maps:to_list(Envelope))],
    EnvelopeEntities ++ lists:append(
      [raw_value_entities(Key, Value) || {Key, Value} <- lists:sort(maps:to_list(Raw))]).

complete_raw_entities(Typed, Raw) ->
    Seen = maps:from_keys([maps:get(origin, Record) || Record <- Raw], true),
    {_Seen, Added} = lists:foldl(
      fun(Record, {Seen0, Acc}) ->
          Origin = maps:get(origin, Record),
          case maps:is_key(Origin, Seen0) of
              true -> {Seen0, Acc};
              false ->
                  Collection = maps:get(<<"collection">>, Record, <<"inventory">>),
                  RawRecord = raw_entity(Origin, Collection, maps:get(raw, Record, #{})),
                  {Seen0#{Origin => true}, [RawRecord | Acc]}
          end
      end,
      {Seen, []},
      Typed),
    Raw ++ lists:reverse(Added).

raw_value_entities(<<"LoadOutPresets">>, Presets) when is_map(Presets) ->
    lists:append(
      [[raw_entity(origin([<<"LoadOutPresets">>, Group, Index]), Group, Item)
        || {Index, Item} <- lists:enumerate(0, Values)]
       || {Group, Values} <- lists:sort(maps:to_list(Presets)), is_list(Values)]);
raw_value_entities(Key, Values) when is_list(Values) ->
    [raw_entity(origin([Key, Index]), Key, Value)
     || {Index, Value} <- lists:enumerate(0, Values)];
raw_value_entities(Key, Values) when is_map(Values),
                                    (Key =:= <<"PlayerSkills">> orelse
                                     Key =:= <<"FocusXP">>) ->
    [raw_entity(origin([Key, Child]), Key, #{Child => Value})
     || {Child, Value} <- lists:sort(maps:to_list(Values))];
raw_value_entities(Key, Value) ->
    [raw_entity(origin([Key]), Key, #{Key => Value})].

raw_entity(Origin, Collection, Raw) ->
    Path = element(2, Origin),
    Fields0 = #{<<"id">> => <<"raw:", (path_text(Path))/binary>>,
                <<"name">> => raw_name(Collection, Raw),
                <<"collection">> => Collection,
                <<"path">> => path_text(Path)},
    Fields = case Raw of
        Map when is_map(Map) ->
            copy_fields([{<<"item_type">>, <<"ItemType">>},
                         {<<"xp">>, <<"XP">>},
                         {<<"count">>, <<"ItemCount">>}], Map, Fields0);
        _ -> Fields0
    end,
    Fields#{entity_type => player_raw, origin => Origin,
            origins => [Origin], raw => Raw}.

raw_name(Collection, Raw) when is_map(Raw) ->
    first_present([maps:get(<<"ItemName">>, Raw, undefined),
                   maps:get(<<"Name">>, Raw, undefined),
                   maps:get(<<"Tag">>, Raw, undefined),
                   maps:get(<<"ItemType">>, Raw, undefined)], Collection);
raw_name(Collection, _Raw) -> Collection.

first_present([], Default) -> Default;
first_present([Value | _], _Default) when is_binary(Value), byte_size(Value) > 0 -> Value;
first_present([_ | Rest], Default) -> first_present(Rest, Default).

-spec origin([binary() | non_neg_integer()]) -> origin().
origin(Path) -> {<<"inventory">>, [<<"raw">> | Path]}.

-spec observation_origin([binary() | non_neg_integer()]) -> origin().
observation_origin(Path) -> {<<"inventory">>, Path}.

first_origin([Origin | _], _Default) -> Origin;
first_origin([], Default) -> Default.

first_path([Path | _]) when is_binary(Path) -> Path;
first_path(_) -> <<"inventory">>.

path_text(Path) ->
    iolist_to_binary(lists:join($., [path_segment(Value) || Value <- Path])).

path_segment(Value) when is_binary(Value) -> Value;
path_segment(Value) when is_integer(Value) -> integer_to_binary(Value);
path_segment(Value) -> retinue_text:to_binary(Value).

present_map(Map) -> maps:filter(fun(_Key, Value) -> present(Value) end, Map).
present(undefined) -> false;
present(null) -> false;
present(_) -> true.

map_value(Value) when is_map(Value) -> Value;
map_value(_Value) -> #{}.

copy_fields([], _Raw, Acc) -> Acc;
copy_fields([{Target, Source} | Rest], Raw, Acc) ->
    copy_fields(Rest, Raw, optional(Target, maps:get(Source, Raw, undefined), Acc)).

optional(_Key, undefined, Acc) -> Acc;
optional(_Key, null, Acc) -> Acc;
optional(Key, Value, Acc) -> Acc#{Key => Value}.

nullable(undefined) -> null;
nullable(Value) -> Value.

integer(Value, _Default) when is_integer(Value) -> Value;
integer(_Value, Default) -> Default.

fallback_id(Value, _Prefix, _Index) when is_binary(Value), byte_size(Value) > 0 -> Value;
fallback_id(_Value, Prefix, Index) ->
    <<Prefix/binary, "#", (integer_to_binary(Index))/binary>>.

id_value(Value) when is_binary(Value), byte_size(Value) > 0 -> Value;
id_value(#{<<"$oid">> := Value}) -> id_value(Value);
id_value(#{<<"$numberLong">> := Value}) -> id_value(Value);
id_value(Value) when is_integer(Value) -> integer_to_binary(Value);
id_value(_Value) -> undefined.

stack_collections() ->
    [<<"FlavourItems">>, <<"MiscItems">>, <<"Recipes">>, <<"Consumables">>,
     <<"LevelKeys">>, <<"FusionTreasures">>, <<"CrewShipAmmo">>].

cache_lookup(Key) ->
    try ets:lookup(?VIEW_CACHE, Key) of
        [{Key, Projection}] -> {ok, Projection};
        [] -> error
    catch error:badarg -> error
    end.

cache_store(Key, Projection) ->
    try ets:insert(?VIEW_CACHE, {Key, Projection})
    catch error:badarg -> false
    end,
    ok.
