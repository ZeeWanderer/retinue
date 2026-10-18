-module(retinue_diagnostics_cli).
-export([command/0, command/1, logs/1, incidents/1]).

command() -> command(all).

command(Game) ->
    Extra = case Game of
        poe2 -> maps:get(commands, retinue_poe2_cli:command());
        _ -> #{}
    end,
    #{help => "inspect checks, incidents and logs",
      commands => Extra#{"incidents" => #{help => "show deduplicated failures and recovery evidence",
          handler => {?MODULE, incidents}, defaults => #{game => Game},
          arguments => [retinue_cli_args:flag(all, "all", "include resolved incidents")]},
        "logs" => #{help => "show recent daemon incidents", handler => {?MODULE, logs},
          arguments => [(retinue_cli_args:option(lines, "lines", {integer, [{min, 1}, {max, 1000}]},
                                                 "maximum entries"))#{default => 50},
                        retinue_cli_args:option(file, "file", string, "custom incident log")]}}}.

incidents(Args) ->
    Result = retinue_client:check_history(),
    case Result of
        {ok, Report} ->
            Scope = maps:get(game, Args, all),
            Checks = [C || C <- maps:get(checks, Report), maps:is_key(<<"first_failed_at">>, C),
                matches_scope(Scope, C),
                maps:get(all, Args, false) orelse maps:get(<<"last_result">>, C) =:= <<"open">>],
            ResultReport = Report#{checks := Checks},
            retinue_output:emit(timestamps(ResultReport), fun() ->
                print_incidents(ResultReport),
                case maps:get(evicted_since_start, Report, 0) of
                    0 -> ok;
                    N -> io:format("History reached its record limit; ~p entries evicted since daemon start.\n", [N])
                end
            end),
            case maps:get(storage_error, Report, undefined) of
                undefined -> ok;
                Error -> retinue_cli_runtime:fail(["check history storage at ", maps:get(path, Report),
                    " failed: ", retinue_text:to_list(Error), ". Use doctor for live health."])
            end;
        {error, Reason} -> retinue_cli_runtime:fail(retinue_client:format_error(Reason))
    end.

matches_scope(all, _) -> true;
matches_scope(Game, Check) ->
    binary:match(maps:get(<<"component">>, Check), <<(atom_to_binary(Game))/binary, ".">>) =:= {0, byte_size(atom_to_binary(Game)) + 1}.

print_incidents(#{checks := [], storage_error := Error}) when Error =/= undefined ->
    io:put_chars("Stored check history is unavailable; absence of incidents is not verified.\n");
print_incidents(#{checks := []}) ->
    io:put_chars("No recorded adapter incidents in this view. Use doctor to verify live health.\n");
print_incidents(#{checks := Checks}) ->
    io:put_chars("Adapter incidents (last observed results, not a live check)\n"),
    lists:foreach(fun(C) ->
        io:format("  ~ts  ~ts / ~ts\n    ~ts\n    last recorded ~ts; version ~ts; ~p failed observations\n", [
            maps:get(<<"state">>, C), maps:get(<<"component">>, C), maps:get(<<"check">>, C),
            maps:get(<<"error">>, C, <<>>), retinue_output:timestamp(maps:get(<<"checked_at">>, C)),
            retinue_text:to_list(maps:get(<<"version">>, maps:get(<<"context">>, C), <<"unknown">>)),
            maps:get(<<"failed_observations">>, C)])
    end, Checks),
    io:put_chars("Resolved after a successful check; old-version results remain unverified.\n").

timestamps(Data) when is_map(Data) ->
    maps:map(fun(Key, Value) ->
        case lists:member(Key, [<<"checked_at">>, <<"received_at">>, <<"first_failed_at">>,
                               <<"last_failed_at">>, <<"last_success_at">>, <<"resolved_at">>]) of
            true -> {timestamp, Value}; false -> timestamps(Value)
        end
    end, Data);
timestamps(Data) when is_list(Data) -> [timestamps(V) || V <- Data];
timestamps(Data) -> Data.

logs(Args) ->
    Path = maps:get(file, Args, retinue_incidents:path(daemon)),
    Report = retinue_incidents:read(daemon, Path, maps:get(lines, Args)),
    retinue_output:emit(Report, fun() ->
        io:format("~ts~n", [Path]),
        lists:foreach(fun(Entry) -> io:format("~ts~n", [retinue_json:encode(Entry)]) end,
                      maps:get(entries, Report, []))
    end),
    case maps:find(error, Report) of
        {ok, Reason} -> retinue_cli_runtime:fail(Reason);
        error -> ok
    end.
