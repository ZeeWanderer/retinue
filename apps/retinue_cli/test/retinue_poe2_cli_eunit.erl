-module(retinue_poe2_cli_eunit).

-include_lib("eunit/include/eunit.hrl").

status_connection_is_separate_from_observed_state_test() ->
    Data = #{<<"observers">> => [#{client => <<"poe2companion">>, mode => <<"watch">>}],
             <<"source">> => <<"/logs/Client.txt">>, <<"revision">> => 8,
             <<"updated_at">> => 1000,
             <<"current">> => #{<<"area_name">> => <<"The Riverbank">>,
                                <<"area_id">> => <<"G1_1">>, <<"area_level">> => 2,
                                <<"seed">> => 123, <<"client_pid">> => 42,
                                <<"actors">> => #{<<"TestName">> => #{
                                    <<"character_class">> => <<"Mercenary">>,
                                    <<"character_level">> => 3, <<"observed_deaths">> => 1}}}},
    Text = retinue_output:with(#{utc => true}, fun() -> format(status, Data) end),
    lists:foreach(fun(Part) -> contains(Text, Part) end,
                  [<<"connection: connected (1)">>, <<"source: /logs/Client.txt">>,
                   <<"revision: 8">>, <<"last event received: 1970-01-01T00:00:01.000Z">>,
                   <<"Last-observed state">>, <<"zone: The Riverbank">>,
                   <<"area id: G1_1">>, <<"area level: 2">>, <<"seed: 123">>,
                   <<"log client pid: 42">>, <<"actors (observed names)">>,
                   <<"TestName: class Mercenary; level 3; observed deaths 1">>]),
    Disconnected = format(status, Data#{<<"observers">> => []}),
    contains(Disconnected, <<"connection: not connected">>),
    contains(Disconnected, <<"Last-observed state">>),
    contains(Disconnected, <<"zone: The Riverbank">>),
    ?assertEqual(nomatch, binary:match(Text, <<"game running">>)).

empty_and_partial_status_test() ->
    Empty = format(status, #{<<"observers">> => []}),
    contains(Empty, <<"connection: not connected">>),
    contains(Empty, <<"source: unknown">>),
    contains(Empty, <<"last event received: unknown">>),
    contains(Empty, <<"Last-observed state: no observations">>),
    Partial = format(status, #{<<"current">> => #{<<"actors">> => #{
        <<"SeenDeath">> => #{<<"observed_deaths">> => 1},
        <<"SeenLevel">> => #{<<"character_level">> => 4}}}}),
    contains(Partial, <<"connection: unknown">>),
    contains(Partial, <<"SeenDeath: class unknown; level unknown; observed deaths 1">>),
    contains(Partial, <<"SeenLevel: class unknown; level 4; observed deaths unknown">>).

events_preserve_source_local_time_and_order_test() ->
    Events = [#{<<"kind">> => <<"area_generated">>, <<"area_id">> => <<"G1_1">>,
                <<"area_level">> => 2, <<"seed">> => 123},
              #{<<"kind">> => <<"area_name">>, <<"area_name">> => <<"The Riverbank">>},
              #{<<"kind">> => <<"character_level">>, <<"character">> => <<"TestName">>,
                <<"character_class">> => <<"Mercenary">>, <<"character_level">> => 3},
              #{<<"kind">> => <<"death">>, <<"character">> => <<"TestName">>}],
    Text = retinue_output:with(#{utc => true}, fun() ->
        format(events, [E#{<<"timestamp_local">> => <<"2026/10/11 03:04:05">>,
                          <<"offset">> => N * 100, <<"client_pid">> => 42, <<"tick_ms">> => N}
                        || {E, N} <- lists:zip(Events, lists:seq(1, 4))])
    end),
    contains(Text, <<"timestamps are game-local">>),
    contains(Text, <<"2026/10/11 03:04:05  Area generated: G1_1; level 2; seed 123">>),
    contains(Text, <<"Zone: The Riverbank">>),
    contains(Text, <<"Character level: TestName; Mercenary; level 3">>),
    contains(Text, <<"Death: TestName">>),
    contains(Text, <<"log pid: 42; tick: 4 ms; offset: 400">>),
    {Area, _} = binary:match(Text, <<"Area generated:">>),
    {Death, _} = binary:match(Text, <<"Death:">>),
    ?assert(Area < Death),
    ?assertEqual(nomatch, binary:match(Text, <<"2026-10-11T">>)),
    ?assertEqual(<<"No observed PoE2 log events.\n">>, format(events, [])).

format(Action, Data) -> unicode:characters_to_binary(retinue_poe2_cli:format(Action, Data)).
contains(Text, Part) -> ?assertNotEqual(nomatch, binary:match(Text, Part)).
