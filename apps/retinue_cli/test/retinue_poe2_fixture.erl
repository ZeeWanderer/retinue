-module(retinue_poe2_fixture).
-export([sample/1, snapshot/0, player/0, process/0, waiting/1, failed/1,
         with_snapshot/2, with_player/2]).

sample(Sequence) ->
    #{<<"stream">> => <<"fixture">>, <<"sequence">> => Sequence,
      <<"observed_at">> => Sequence * 1000, <<"status">> => <<"observed">>,
      <<"process">> => process(), <<"snapshot">> => snapshot(), <<"error">> => null}.

process() ->
    #{<<"pid">> => 42, <<"started">> => 1000,
      <<"executable">> => <<"/game/PathOfExile_x64.exe">>, <<"image_base">> => 16#140000000}.

snapshot() ->
    #{<<"scene">> => <<"in_game">>, <<"states">> => [<<"InGameState">>],
      <<"player">> => player(),
      <<"area">> => #{<<"name">> => <<"The Riverbank">>, <<"in_town">> => false,
                      <<"change_key">> => 7},
      <<"panels">> => #{<<"inventory">> => false, <<"stash">> => null}, <<"issues">> => []}.

player() ->
    #{<<"name">> => <<"ExampleCharacter">>, <<"class">> => <<"Huntress">>,
      <<"level">> => 2, <<"experience">> => 100, <<"gold">> => 42, <<"life">> => 100}.

waiting(Sequence) ->
    (sample(Sequence))#{<<"status">> => <<"waiting">>, <<"process">> => null,
                        <<"snapshot">> => null}.

failed(Sequence) ->
    (sample(Sequence))#{<<"status">> => <<"error">>, <<"snapshot">> => null,
                        <<"error">> => <<"game state changed during observation">>}.

with_snapshot(Sample, Fields) ->
    Sample#{<<"snapshot">> => maps:merge(maps:get(<<"snapshot">>, Sample), Fields)}.

with_player(Sample, Fields) ->
    Snapshot = maps:get(<<"snapshot">>, Sample),
    with_snapshot(Sample, #{<<"player">> => maps:merge(maps:get(<<"player">>, Snapshot), Fields)}).
