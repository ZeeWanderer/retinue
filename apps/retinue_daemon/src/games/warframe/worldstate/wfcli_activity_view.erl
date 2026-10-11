%%%-------------------------------------------------------------------
%% Compact semantic worldstate view used by desktop clients.
%%%-------------------------------------------------------------------
-module(wfcli_activity_view).

-export([project/1, project_fissure/2]).

-doc "Project a worldstate result into JSON-safe desktop activity data.".
-spec project(map()) -> {ok, map()} | {error, term()}.
project(#{entries := Entries} = Result) when is_list(Entries) ->
    Opts = maps:get(opts, Result, #{}),
    Now = maps:get(now_ms, Result, erlang:system_time(millisecond)),
    Fissures = [project_fissure(Entry, Opts)
                || Entry <- Entries, maps:get(type, Entry, undefined) =:= fissure],
    {ok, #{<<"fissures">> => Fissures,
           <<"cycles">> => cycles(Entries, Now),
           <<"baro">> => project_baro(Entries, Opts),
           <<"resurgence">> => project_resurgence(Entries, Opts),
           <<"source">> => value(maps:get(source, Result, undefined)),
           <<"snapshot_origin">> => value(maps:get(snapshot_origin, Result, undefined)),
           <<"snapshot_age_ms">> => value(maps:get(snapshot_age_ms, Result, 0)),
           <<"stale">> => maps:get(stale, Result, false) =:= true}};
project(_Result) -> {error, malformed_worldstate_activity}.

-doc "Project one normalized fissure for desktop and notification consumers.".
-spec project_fissure(map(), map()) -> map().
project_fissure(Entry, Opts) ->
    Row = wfcli_worldstate_projector:table_row_map(Entry, Opts),
    #{<<"id">> => value(maps:get(id, Row, maps:get(id, Entry, <<>>))),
      <<"tier">> => value(maps:get(tier, Row, <<>>)),
      <<"mission">> => value(maps:get(mission, Row, <<>>)),
      <<"node">> => value(maps:get(node, Row, <<>>)),
      <<"expiry">> => value(maps:get(expiry, Row, <<>>)),
      <<"hard">> => present(maps:get(hard, Row, false))}.

cycles(Entries, Now) ->
    CetusAnchor = cetus_anchor(Entries),
    {EarthState, EarthExpiry} = two_phase(Now, 0, 4 * 60 * 60 * 1000,
                                          4 * 60 * 60 * 1000,
                                          <<"day">>, <<"night">>),
    {CetusState, CetusExpiry} = two_phase(Now, CetusAnchor,
                                          100 * 60 * 1000, 50 * 60 * 1000,
                                          <<"day">>, <<"night">>),
    {VallisState, VallisExpiry} = two_phase(
                                   Now, datetime_ms({{2018, 11, 10}, {8, 13, 48}}),
                                   400000, 1200000, <<"warm">>, <<"cold">>),
    CambionState = case CetusState of <<"day">> -> <<"fass">>;
                                          _ -> <<"vome">> end,
    [cycle(<<"earth">>, <<"Earth">>, EarthState, EarthExpiry),
     cycle(<<"cetus">>, <<"Cetus">>, CetusState, CetusExpiry),
     cycle(<<"vallis">>, <<"Vallis">>, VallisState, VallisExpiry),
     cycle(<<"cambion">>, <<"Cambion">>, CambionState, CetusExpiry)].

cycle(Id, Name, State, Expiry) ->
    #{<<"id">> => Id, <<"name">> => Name, <<"state">> => State,
      <<"expires_at">> => Expiry}.

two_phase(Now, Anchor, FirstMs, SecondMs, First, Second) ->
    Period = FirstMs + SecondMs,
    Elapsed = positive_mod(Now - Anchor, Period),
    case Elapsed < FirstMs of
        true -> {First, Now + FirstMs - Elapsed};
        false -> {Second, Now + Period - Elapsed}
    end.

positive_mod(Value, Divisor) -> ((Value rem Divisor) + Divisor) rem Divisor.

cetus_anchor(Entries) ->
    case [date_ms(maps:get(<<"Activation">>, Data, undefined))
          || #{type := syndicate_mission, data := Data} <- Entries,
             maps:get(<<"Tag">>, Data, undefined) =:= <<"CetusSyndicate">>] of
        [Anchor | _] when is_integer(Anchor) -> Anchor;
        _ -> 0
    end.

project_baro(Entries, Opts) ->
    case [Entry || Entry <- Entries, maps:get(type, Entry, undefined) =:= baro] of
        [Entry | _] ->
            Row = wfcli_worldstate_projector:table_row_map(Entry, Opts),
            #{<<"name">> => value(maps:get(name, Row, <<"Baro Ki'Teer">>)),
              <<"node">> => value(maps:get(node, Row, <<>>)),
              <<"activation">> => value(maps:get(window_start, Row, <<>>)),
              <<"expiry">> => value(maps:get(window_end, Row, <<>>))};
        [] -> null
    end.

project_resurgence(Entries, Opts) ->
    case [Entry || Entry <- Entries, maps:get(type, Entry, undefined) =:= prime_vault] of
        [Entry | _] ->
            Row = wfcli_worldstate_projector:table_row_map(Entry, Opts),
            #{<<"name">> => <<"Prime Resurgence">>,
              <<"featured">> => display_value(maps:get(featured, Row, <<>>)),
              <<"activation">> => value(maps:get(window_start, Row, <<>>)),
              <<"expiry">> => value(maps:get(window_end, Row, <<>>))};
        [] -> null
    end.

date_ms(#{<<"$date">> := Value}) -> date_ms(Value);
date_ms(#{<<"$numberLong">> := Value}) when is_binary(Value) ->
    try binary_to_integer(Value) catch error:badarg -> undefined end;
date_ms(Value) when is_integer(Value) -> Value;
date_ms(_Value) -> undefined.

datetime_ms(DateTime) ->
    Epoch = calendar:datetime_to_gregorian_seconds({{1970, 1, 1}, {0, 0, 0}}),
    (calendar:datetime_to_gregorian_seconds(DateTime) - Epoch) * 1000.

present(false) -> false;
present(undefined) -> false;
present(null) -> false;
present(<<>>) -> false;
present([]) -> false;
present(_) -> true.

value(undefined) -> null;
value(null) -> null;
value(Value) when is_binary(Value); is_integer(Value); is_float(Value);
                  is_boolean(Value) -> Value;
value(Value) when is_atom(Value) -> atom_to_binary(Value);
value(Value) when is_list(Value) -> unicode:characters_to_binary(Value);
value(Value) -> iolist_to_binary(io_lib:format("~p", [Value])).

display_value(Value) ->
    case value(Value) of
        <<"/", _/binary>> -> <<>>;
        Display -> Display
    end.
