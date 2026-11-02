-module(wf_player_view_cache).
-export([get/2]).
-define(TABLE, wf_player_view_cache).

get(Key, Build) ->
    Slot = element(1, Key),
    case lookup(Slot) of
        [{Slot, Key, _, {ready, Value}}] -> Value;
        _ ->
            Token = make_ref(),
            Pending = {Slot, Key, Token, pending},
            Stored = try ets:insert(?TABLE, Pending) catch error:badarg -> false end,
            Value = Build(),
            case Stored of
                true ->
                    Ready = {Slot, Key, Token, {ready, Value}},
                    %% A newer request or invalidation retires this worker's reservation.
                    try ets:select_replace(?TABLE, [{Pending, [], [{const, Ready}]}])
                    catch error:badarg -> 0
                    end;
                false -> ok
            end,
            Value
    end.

lookup(Slot) ->
    try ets:lookup(?TABLE, Slot) catch error:badarg -> [] end.
