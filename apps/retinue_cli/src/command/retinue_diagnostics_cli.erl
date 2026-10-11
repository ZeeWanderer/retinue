-module(retinue_diagnostics_cli).
-export([command/0, logs/1]).

command() ->
    #{help => "inspect platform incident logs",
      commands => #{"logs" => #{help => "show recent daemon incidents", handler => {?MODULE, logs},
          arguments => [(retinue_cli_args:option(lines, "lines", {integer, [{min, 1}, {max, 1000}]},
                                                 "maximum entries"))#{default => 50},
                        retinue_cli_args:option(file, "file", string, "custom incident log")]}}}.

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
