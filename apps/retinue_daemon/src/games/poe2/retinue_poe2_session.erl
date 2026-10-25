-module(retinue_poe2_session).

-export([empty/0, sample/1, apply/2, restore/1]).

-define(U32, 16#ffffffff).
-define(U64, 16#ffffffffffffffff).
-define(I32, 16#7fffffff).

empty() ->
    #{<<"schema">> => 2, <<"game">> => <<"poe2">>, <<"revision">> => 0,
      <<"updated_at">> => null, <<"stream">> => null, <<"sequence">> => null,
      <<"observed_at">> => null, <<"status">> => <<"waiting">>,
      <<"process">> => null, <<"current">> => null, <<"error">> => null, <<"events">> => []}.

sample(Data) ->
    try {ok, normalize_sample(Data)}
    catch throw:invalid -> {error, invalid_session_sample} end.

normalize_sample(Data) ->
    Sample = object(Data, [
        {<<"stream">>, fun(V) -> text(V, 128) end},
        {<<"sequence">>, fun u64/1}, {<<"observed_at">>, fun u64/1},
        {<<"status">>, fun(V) -> choice(V, [<<"observed">>, <<"waiting">>, <<"error">>]) end},
        {<<"process">>, fun(V) -> nullable(V, fun process/1) end},
        {<<"snapshot">>, fun(V) -> nullable(V, fun snapshot/1) end},
        {<<"error">>, fun(V) -> nullable(V, fun(T) -> text(T, 2048) end) end}]),
    case Sample of
        #{<<"status">> := <<"observed">>, <<"process">> := P, <<"snapshot">> := S,
          <<"error">> := null} when is_map(P), is_map(S) -> Sample;
        #{<<"status">> := <<"waiting">>, <<"process">> := null,
          <<"snapshot">> := null, <<"error">> := null} -> Sample;
        #{<<"status">> := <<"error">>, <<"snapshot">> := null,
          <<"error">> := Error} when is_binary(Error) -> Sample;
        _ -> throw(invalid)
    end.

process(Value) ->
    object(Value, [{<<"pid">>, fun u32/1}, {<<"started">>, fun u64/1},
                   {<<"executable">>, fun(V) -> text(V, 4096) end},
                   {<<"image_base">>, fun u64/1}]).

snapshot(Value) ->
    Snapshot = object(Value, [{<<"scene">>, fun scene/1}, {<<"states">>, fun states/1},
                   {<<"player">>, fun(V) -> nullable(V, fun player/1) end},
                   {<<"area">>, fun(V) -> nullable(V, fun area/1) end},
                   {<<"panels">>, fun panels/1},
                   {<<"issues">>, fun(V) -> array(V, 16, fun issue/1) end}]),
    WithSocial = case maps:find(<<"social">>, Value) of
        error -> Snapshot;
        {ok, Raw} ->
            Social = nullable(Raw, fun social/1),
            case {Social, maps:get(<<"player">>, Snapshot)} of
                {#{<<"local">> := #{<<"character">> := Name}}, #{<<"name">> := Name}} -> ok;
                {#{<<"local">> := Local}, _} when Local =/= null -> throw(invalid);
                _ -> ok
            end,
            Snapshot#{<<"social">> => Social}
    end,
    case maps:find(<<"entities">>, Value) of
        error -> WithSocial;
        {ok, RawEntities} -> WithSocial#{<<"entities">> => nullable(RawEntities, fun entities/1)}
    end.

entities(Value) ->
    Result = object(Value, [{<<"tree">>, fun address/1}, {<<"area_key">>, fun u32/1},
        {<<"count">>, fun(V) -> number(V, 1, 4096) end},
        {<<"unique_monsters">>, fun(V) -> array(V, 128, fun unique_monster/1) end}]),
    Monsters = maps:get(<<"unique_monsters">>, Result),
    Ids = [maps:get(<<"id">>, M) || M <- Monsters],
    Addresses = [maps:get(<<"address">>, M) || M <- Monsters],
    case length(Monsters) =< maps:get(<<"count">>, Result) andalso
         length(lists:usort(Ids)) =:= length(Ids) andalso
         length(lists:usort(Addresses)) =:= length(Addresses) of
        true -> Result#{<<"unique_monsters">> => lists:sort(
            fun(A, B) -> maps:get(<<"id">>, A) =< maps:get(<<"id">>, B) end, Monsters)};
        false -> throw(invalid)
    end.

unique_monster(Value) ->
    object(Value, [{<<"id">>, fun(V) -> number(V, 0, 16#3fffffff) end},
        {<<"address">>, fun address/1}, {<<"name">>, fun(V) -> text(V, 512) end},
        {<<"life">>, fun(V) -> number(V, 0, ?I32) end}]).

address(Value) when is_integer(Value), Value rem 8 =:= 0 ->
    number(Value, 65536, 16#7fffffff0000);
address(_) -> throw(invalid).

social(Value) ->
    Result = object(Value, [{<<"local">>, fun(V) -> nullable(V, fun identity/1) end},
                            {<<"party">>, fun(V) -> array(V, 64, fun party_member/1) end}]),
    Members = maps:get(<<"party">>, Result),
    Names = [maps:get(<<"character">>, Member) || Member <- Members],
    case length(lists:usort(Names)) =:= length(Names) of
        true -> Result;
        false -> throw(invalid)
    end.

identity(Value) ->
    object(Value, [{Key, fun(V) -> text(V, 256) end} ||
        Key <- [<<"character">>, <<"account">>, <<"league">>]]).

party_member(Value) ->
    object(Value, [{<<"character">>, fun(V) -> text(V, 256) end},
                   {<<"account">>, fun(V) -> text(V, 256) end},
                   {<<"league">>, fun(V) -> nullable(V, fun(T) -> text(T, 256) end) end},
                   {<<"class_id">>, fun(V) -> nullable(V, fun(T) -> number(T, 0, 254) end) end},
                   {<<"level">>, fun(V) -> nullable(V, fun level/1) end}]).

scene(Value) -> choice(Value, [<<"unknown">>, <<"login">>, <<"select_character">>,
                              <<"loading">>, <<"in_game">>, <<"escape_menu">>]).
states(Value) -> array(Value, 16, fun(V) -> text(V, 64) end).

player(Value) ->
    object(Value, [{<<"name">>, fun(V) -> text(V, 256) end},
                   {<<"class">>, fun character_class/1}, {<<"level">>, fun level/1},
                   {<<"experience">>, fun u32/1},
                   {<<"gold">>, fun(V) -> nullable(V, fun i32/1) end},
                   {<<"life">>, fun(V) -> nullable(V, fun(T) -> number(T, 0, ?I32) end) end}]).

character_class(Value) -> nullable(Value, fun(V) -> text(V, 128) end).
level(Value) -> number(Value, 1, 100).

area(Value) ->
    object(Value, [{<<"name">>, fun(V) -> text(V, 512) end},
                   {<<"in_town">>, fun boolean/1}, {<<"change_key">>, fun u32/1}]).

panel_names() -> [<<"passive_tree">>, <<"stash">>, <<"inventory">>,
                  <<"character_sheet">>, <<"market">>].

panels(Value) when is_map(Value), map_size(Value) =< 5 ->
    maps:from_list([{choice(K, panel_names()), nullable(V, fun boolean/1)}
                   || {K, V} <- maps:to_list(Value)]);
panels(_) -> throw(invalid).

issue(Value) ->
    object(Value, [{<<"field">>, fun(V) -> text(V, 64) end},
                   {<<"reason">>, fun(V) -> text(V, 2048) end}]).

apply(Sample, Previous) ->
    PriorSample = as_sample(Previous),
    SameStream = maps:get(<<"stream">>, Sample) =:= maps:get(<<"stream">>, Previous),
    Sequence = maps:get(<<"sequence">>, Sample),
    Last = maps:get(<<"sequence">>, Previous),
    case SameStream of
        true when Sequence =:= Last, Sample =:= PriorSample -> {retry, Previous};
        true when Sequence =:= Last -> {error, session_sequence_conflict};
        true when Sequence < Last -> {error, stale_session_sample};
        _ -> project(Sample, Previous, PriorSample)
    end.

as_sample(Status) ->
    (maps:with([<<"stream">>, <<"sequence">>, <<"observed_at">>, <<"status">>,
                <<"process">>, <<"error">>], Status))#{
        <<"snapshot">> => maps:get(<<"current">>, Status, undefined)}.

project(Sample, Previous, PriorSample) ->
    Next = maps:merge(Previous, (maps:remove(<<"snapshot">>, Sample))#{
        <<"current">> => maps:get(<<"snapshot">>, Sample)}),
    Fields = [<<"status">>, <<"process">>, <<"snapshot">>, <<"error">>],
    case maps:get(<<"stream">>, Previous) =/= null andalso
         maps:with(Fields, Sample) =:= maps:with(Fields, PriorSample) of
        true -> {heartbeat, Next};
        false ->
            Events = maps:get(<<"events">>, Previous) ++ changes(Sample, PriorSample),
            {changed, Next#{<<"revision">> => maps:get(<<"revision">>, Previous) + 1,
                            <<"updated_at">> => erlang:system_time(millisecond),
                            <<"events">> => lists:nthtail(max(0, length(Events) - 128), Events)}}
    end.

changes(#{<<"status">> := <<"observed">>, <<"snapshot">> := Current,
          <<"process">> := Process} = Sample, Previous) ->
    Before = case Previous of
        #{<<"status">> := <<"observed">>, <<"process">> := Process,
          <<"snapshot">> := Old} -> Old;
        _ -> #{}
    end,
    Player = maps:get(<<"player">>, Current),
    OldPlayer = maps:get(<<"player">>, Before, null),
    Identity = character(Player),
    OldIdentity = character(OldPlayer),
    SameCharacter = Identity =/= null andalso Identity =:= OldIdentity,
    Baseline = case SameCharacter of true -> OldPlayer; false -> null end,
    Details = change(<<"state">>, state(Before), state(Current), #{}) ++
              change(<<"area">>, maps:get(<<"area">>, Before, null),
                     maps:get(<<"area">>, Current), #{}) ++
              change(<<"character">>, OldIdentity, Identity, #{}) ++
              player_changes(Baseline, Player) ++
              panel_changes(maps:get(<<"panels">>, Before, #{}), maps:get(<<"panels">>, Current)) ++
              death(Baseline, Player) ++
              monster_deaths(Sample, Previous, SameCharacter, Before, Current),
    Stamp = (maps:with([<<"stream">>, <<"sequence">>, <<"observed_at">>], Sample))#{
        <<"process">> => maps:with([<<"pid">>, <<"started">>], Process)},
    [maps:merge(Stamp, Detail) || Detail <- Details];
changes(_, _) -> [].

state(#{<<"scene">> := _, <<"states">> := _} = Snapshot) ->
    maps:with([<<"scene">>, <<"states">>], Snapshot);
state(_) -> null.

character(null) -> null;
character(Player) -> maps:with([<<"name">>, <<"class">>], Player).

change(_, _, null, _) -> [];
change(_, Value, Value, _) -> [];
change(Kind, From, To, Extra) ->
    [Extra#{<<"kind">> => Kind, <<"from">> => From, <<"to">> => To}].

player_changes(_, null) -> [];
player_changes(Before, Player) ->
    Extra = #{<<"character">> => maps:get(<<"name">>, Player)},
    lists:append([change(Kind, player_field(Field, Before), maps:get(Field, Player), Extra)
                  || {Kind, Field} <- [{<<"xp">>, <<"experience">>},
                                        {<<"level">>, <<"level">>}, {<<"gold">>, <<"gold">>}]]).

player_field(_, null) -> null;
player_field(Field, Player) -> maps:get(Field, Player).

panel_changes(Before, Panels) ->
    lists:append([change(<<"panel">>, maps:get(Name, Before, null), Value, #{<<"panel">> => Name})
                  || {Name, Value} <- lists:sort(maps:to_list(Panels))]).

death(#{<<"life">> := Life}, #{<<"life">> := 0, <<"name">> := Name})
  when is_integer(Life), Life > 0 ->
    change(<<"death">>, Life, 0, #{<<"character">> => Name});
death(_, _) -> [].

monster_deaths(#{<<"stream">> := Stream, <<"observed_at">> := Now},
               #{<<"stream">> := Stream, <<"observed_at">> := Then}, true,
               #{<<"entities">> := #{<<"tree">> := Tree, <<"area_key">> := Key,
                                     <<"unique_monsters">> := Before}},
               #{<<"entities">> := #{<<"tree">> := Tree, <<"area_key">> := Key,
                                     <<"unique_monsters">> := Current}})
  when Now >= Then, Now - Then =< 65000 ->
    Old = maps:from_list([{maps:get(<<"id">>, M), M} || M <- Before]),
    lists:append([case maps:get(Id, Old, null) of
        #{<<"address">> := Address, <<"name">> := Name, <<"life">> := Life} when Life > 0 ->
            change(<<"unique_monster_death">>, Life, 0,
                   #{<<"id">> => Id, <<"address">> => Address, <<"name">> => Name,
                     <<"tree">> => Tree, <<"area_key">> => Key});
        _ -> []
    end || #{<<"id">> := Id, <<"address">> := Address, <<"name">> := Name,
             <<"life">> := 0} <- Current]);
monster_deaths(_, _, _, _, _) -> [].

restore(#{<<"schema">> := 2, <<"game">> := <<"poe2">>} = Cache) ->
    try
        Sample = normalize_sample(as_sample(Cache)),
        Meta = object(Cache, [{<<"revision">>, fun(V) -> number(V, 1, ?U64) end},
                              {<<"updated_at">>, fun u64/1},
                              {<<"events">>, fun(V) -> array(V, 128, fun event/1) end}]),
        {ok, maps:merge(maps:merge(empty(), Meta), (maps:remove(<<"snapshot">>, Sample))#{
            <<"current">> => maps:get(<<"snapshot">>, Sample)})}
    catch throw:invalid -> {error, invalid_session_cache} end;
restore(_) -> {error, invalid_session_cache}.

event(Value) ->
    Base = object(Value, [{<<"stream">>, fun(V) -> text(V, 128) end},
                          {<<"sequence">>, fun u64/1}, {<<"observed_at">>, fun u64/1},
                          {<<"process">>, fun(V) -> object(V, [
                              {<<"pid">>, fun u32/1}, {<<"started">>, fun u64/1}]) end}]),
    {Validate, Extra} = case maps:get(<<"kind">>, Value, undefined) of
        <<"state">> -> {fun(V) -> object(V, [{<<"scene">>, fun scene/1},
                                             {<<"states">>, fun states/1}]) end, []};
        <<"area">> -> {fun area/1, []};
        <<"character">> -> {fun(V) -> object(V, [{<<"name">>, fun(N) -> text(N, 256) end},
                                                 {<<"class">>, fun character_class/1}]) end, []};
        <<"xp">> -> {fun u32/1, [{<<"character">>, fun(V) -> text(V, 256) end}]};
        <<"level">> -> {fun level/1, [{<<"character">>, fun(V) -> text(V, 256) end}]};
        <<"gold">> -> {fun i32/1, [{<<"character">>, fun(V) -> text(V, 256) end}]};
        <<"panel">> -> {fun boolean/1, [{<<"panel">>, fun(V) -> choice(V, panel_names()) end}]};
        <<"death">> -> {fun(V) -> number(V, 0, ?I32) end,
                        [{<<"character">>, fun(V) -> text(V, 256) end}]};
        <<"unique_monster_death">> -> {fun(V) -> number(V, 0, ?I32) end,
            [{<<"id">>, fun(V) -> number(V, 0, 16#3fffffff) end},
             {<<"address">>, fun address/1}, {<<"name">>, fun(V) -> text(V, 512) end},
             {<<"tree">>, fun address/1}, {<<"area_key">>, fun u32/1}]};
        _ -> throw(invalid)
    end,
    Detail = object(Value, [{<<"from">>, fun(V) -> nullable(V, Validate) end},
                            {<<"to">>, Validate} | Extra]),
    Kind = maps:get(<<"kind">>, Value),
    case {Kind, Detail} of
        {<<"death">>, #{<<"from">> := From, <<"to">> := 0}} when is_integer(From), From > 0 -> ok;
        {<<"death">>, _} -> throw(invalid);
        {<<"unique_monster_death">>, #{<<"from">> := From, <<"to">> := 0}}
          when is_integer(From), From > 0 -> ok;
        {<<"unique_monster_death">>, _} -> throw(invalid);
        _ -> ok
    end,
    (maps:merge(Base, Detail))#{<<"kind">> => Kind}.

object(Value, Fields) when is_map(Value) ->
    maps:from_list([{Key, Normalize(maps:get(Key, Value, undefined))} || {Key, Normalize} <- Fields]);
object(_, _) -> throw(invalid).

array(Value, Max, Normalize) when is_list(Value), length(Value) =< Max ->
    [Normalize(Item) || Item <- Value];
array(_, _, _) -> throw(invalid).

nullable(null, _) -> null;
nullable(Value, Normalize) -> Normalize(Value).

text(Value, Max) when is_binary(Value), byte_size(Value) > 0, byte_size(Value) =< Max ->
    case unicode:characters_to_list(Value) of
        Chars when is_list(Chars) -> Value;
        _ -> throw(invalid)
    end;
text(_, _) -> throw(invalid).

choice(Value, Choices) ->
    case lists:member(Value, Choices) of true -> Value; false -> throw(invalid) end.
boolean(true) -> true;
boolean(false) -> false;
boolean(_) -> throw(invalid).
u32(Value) -> number(Value, 0, ?U32).
u64(Value) -> number(Value, 0, ?U64).
i32(Value) -> number(Value, -16#80000000, ?I32).
number(Value, Min, Max) when is_integer(Value), Value >= Min, Value =< Max -> Value;
number(_, _, _) -> throw(invalid).
