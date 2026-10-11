-module(wf_companion_diagnostics_cli).

-export([command/0]).
-import(retinue_cli_args, [option/4]).

command() ->
    #{help => "inspect live companion collectors and bounded watches",
      arguments => [option(pid, "pid", {integer, [{min, 1}]}, "select a companion process")],
      commands => #{
        "status" => #{help => "show live counters and recent watch jobs",
                        handler => fun(Args) -> once(status, Args) end},
        "watch" => #{help => "watch existing collectors (one-second samples)",
                     commands => #{"inventory" => #{
                       help => "stream inventory pipeline counters",
                       arguments => [(option(seconds, "seconds", {integer, [{min, 1}, {max, 1800}]},
                                             "watch duration"))#{default => 60}],
                       handler => fun watch/1}}},
        "stop" => #{help => "stop a running watch",
                    arguments => [#{name => job, help => "job ID from watch or status"}],
                    handler => fun(Args) -> once(stop, Args#{job => list_to_binary(maps:get(job, Args))}) end}}}.

request(Action, Args) ->
    (maps:with([pid, seconds, job], Args))#{source => companion_diagnostics, action => Action}.

once(Action, Args) ->
    case retinue_client:one_shot(warframe, request(Action, Args)) of
        {ok, Data} -> emit(Data);
        {error, Reason} -> fail(Reason)
    end.

watch(Args) ->
    case retinue_client:subscribe(warframe, (request(watch, Args))#{topic => inventory}) of
        {ok, Handle} ->
            try watch_next(Handle)
            after retinue_client:unsubscribe(Handle)
            end;
        {error, Reason} -> fail(Reason)
    end.

watch_next(Handle) ->
    case retinue_client:next(Handle, 10000) of
        {ok, Data} ->
            emit(Data),
            case maps:get(<<"state">>, Data) of
                <<"running">> -> watch_next(Handle);
                _ -> ok
            end;
        {error, Reason} -> fail(Reason)
    end.

emit(#{<<"state">> := <<"failed">>, <<"error">> := Reason}) -> fail(Reason);
emit(Data) ->
    retinue_output:emit(fun() -> timestamps(Data) end,
                      fun() -> io:put_chars(wf_companion_format:diagnostics(Data)) end).

timestamps(Data) when is_map(Data) ->
    maps:map(fun(Key, Value) ->
                 case lists:member(Key, [<<"started_at">>, <<"expires_at">>, <<"finished_at">>,
                                         <<"inventory_received_at">>, <<"game_metadata_received_at">>,
                                         <<"last_observed_at">>]) of
                     true -> {timestamp, Value};
                     false -> timestamps(Value)
                 end
             end, Data);
timestamps(Data) when is_list(Data) -> [timestamps(Value) || Value <- Data];
timestamps(Data) -> Data.

fail(Reason) ->
    case init:get_status() of
        {stopping, _} -> ok;
        _ -> report_failure(Reason)
    end.

report_failure(Reason) ->
    Message = case Reason of
        no_companion_connected -> "no wfcompanion is connected";
        ambiguous_companion -> "multiple companions connected; select one with --pid";
        companion_diagnostics_unavailable -> "companion has no runtime diagnostics; rebuild and restart it";
        companion_disconnected -> "companion disconnected; diagnostic session ended";
        companion_diagnostic_timeout -> "companion did not answer before the diagnostic deadline";
        Value when is_binary(Value) -> Value;
        _ -> lists:flatten(io_lib:format("~tp", [Reason]))
    end,
    retinue_output:emit(#{error => Message}, fun() -> io:format(standard_error, "error: ~ts~n", [Message]) end),
    halt(1).
