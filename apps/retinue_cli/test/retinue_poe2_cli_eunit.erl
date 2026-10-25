-module(retinue_poe2_cli_eunit).
-include_lib("eunit/include/eunit.hrl").

status_connection_is_separate_from_observed_state_test() ->
    Data = #{<<"observers">> => [#{client => <<"poe2companion">>, mode => <<"watch">>}],
             <<"status">> => <<"observed">>, <<"revision">> => 8,
             <<"updated_at">> => 1000, <<"observed_at">> => 31000,
             <<"process">> => retinue_poe2_fixture:process(),
             <<"current">> => retinue_poe2_fixture:snapshot(), <<"error">> => null},
    Text = retinue_output:with(#{utc => true}, fun() -> format(status, Data) end),
    lists:foreach(fun(Part) -> contains(Text, Part) end,
                  [<<"PoE2 native session">>, <<"connection: connected (1)">>,
                   <<"revision: 8">>, <<"last changed: 1970-01-01T00:00:01.000Z">>,
                   <<"last observed: 1970-01-01T00:00:31.000Z">>,
                   <<"scene: in game">>, <<"zone: The Riverbank">>, <<"in town: no">>,
                   <<"process: 42">>, <<"character: ExampleCharacter">>, <<"class: Huntress">>,
                   <<"level: 2">>, <<"XP: 100">>, <<"gold: 42">>, <<"life: 100">>,
                   <<"inventory: closed">>, <<"stash: unknown">>]),
    Disconnected = format(status, Data#{<<"observers">> => []}),
    contains(Disconnected, <<"connection: not connected">>),
    contains(Disconnected, <<"zone: The Riverbank">>),
    ?assertEqual(nomatch, binary:match(Text, <<"log observer">>)).

waiting_error_and_partial_status_test() ->
    Empty = format(status, (retinue_poe2_session:empty())#{<<"observers">> => []}),
    contains(Empty, <<"connection: not connected">>),
    contains(Empty, <<"status: waiting">>),
    contains(Empty, <<"last observed: unknown">>),
    contains(Empty, <<"Native state: no observation">>),
    Error = format(status, (retinue_poe2_session:empty())#{<<"status">> => <<"error">>,
        <<"error">> => <<"layout unresolved">>, <<"process">> => retinue_poe2_fixture:process()}),
    contains(Error, <<"status: error">>),
    contains(Error, <<"error: layout unresolved">>),
    Partial = format(status, #{<<"current">> => (retinue_poe2_fixture:snapshot())#{
        <<"player">> => null, <<"area">> => null, <<"panels">> => #{},
        <<"issues">> => [#{<<"field">> => <<"player">>, <<"reason">> => <<"component unreadable">>}]}}),
    lists:foreach(fun(Part) -> contains(Partial, Part) end,
        [<<"character: unknown">>, <<"zone: unknown">>, <<"panels: unknown">>,
         <<"read issues">>, <<"player: component unreadable">>]).

social_account_is_observed_not_authenticated_test() ->
    Current = (retinue_poe2_fixture:snapshot())#{<<"social">> => #{
        <<"local">> => #{<<"character">> => <<"ExampleCharacter">>,
                         <<"account">> => <<"Example#1234">>, <<"league">> => <<"Standard">>},
        <<"party">> => []}},
    Text = format(status, #{<<"current">> => Current}),
    contains(Text, <<"account (observed): Example#1234">>),
    contains(Text, <<"league (observed): Standard">>),
    contains(Text, <<"party members: 0">>),
    ?assertEqual(nomatch, binary:match(Text, <<"authenticated">>)).

entity_observation_status_test() ->
    Current = (retinue_poe2_fixture:snapshot())#{<<"entities">> => #{
        <<"count">> => 12, <<"unique_monsters">> => [#{<<"name">> => <<"Example Unique">>}]}},
    Text = format(status, #{<<"current">> => Current}),
    contains(Text, <<"nearby entities: 12">>),
    contains(Text, <<"unique monsters: 1">>),
    Event = #{<<"observed_at">> => 1, <<"kind">> => <<"unique_monster_death">>,
              <<"name">> => <<"Example Unique">>},
    contains(format(events, [Event]), <<"Unique monster death: Example Unique (killer unknown)">>).

events_use_output_timestamps_test() ->
    {ok, Sample} = retinue_poe2_session:sample(retinue_poe2_fixture:sample(1)),
    {changed, Initial} = retinue_poe2_session:apply(Sample, retinue_poe2_session:empty()),
    Dead = retinue_poe2_fixture:with_player(retinue_poe2_fixture:sample(2),
                                          #{<<"experience">> => 200, <<"life">> => 0}),
    {changed, Last} = retinue_poe2_session:apply(Dead, Initial),
    Events = maps:get(<<"events">>, Last),
    Text = retinue_output:with(#{utc => true}, fun() -> format(events, Events) end),
    lists:foreach(fun(Part) -> contains(Text, Part) end,
        [<<"1970-01-01T00:00:01.000Z  Scene: in game">>, <<"Zone: The Riverbank">>,
         <<"Character: ExampleCharacter; class Huntress">>, <<"Panel: inventory; closed">>,
         <<"XP: ExampleCharacter; 100 -> 200">>, <<"Level: ExampleCharacter; unknown -> 2">>,
         <<"Gold: ExampleCharacter; unknown -> 42">>,
         <<"1970-01-01T00:00:02.000Z  Death: ExampleCharacter">>]),
    {Area, _} = binary:match(Text, <<"Zone:">>),
    {Death, _} = binary:match(Text, <<"Death:">>),
    ?assert(Area < Death),
    ?assertEqual(nomatch, binary:match(Text, <<"game-local">>)),
    ?assertEqual(<<"No observed PoE2 session events.\n">>, format(events, [])).

display_does_not_execute_terminal_controls_test() ->
    Text = format(status, #{<<"error">> => <<"read", 27, "[31mfailed", 10, "line">>}),
    ?assertEqual(nomatch, binary:match(Text, <<27>>)),
    contains(Text, <<"error: read [31mfailed line">>).

format(Action, Data) -> unicode:characters_to_binary(retinue_poe2_cli:format(Action, Data)).
contains(Text, Part) -> ?assertNotEqual(nomatch, binary:match(Text, Part)).
