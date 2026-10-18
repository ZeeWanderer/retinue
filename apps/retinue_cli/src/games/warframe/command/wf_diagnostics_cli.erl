-module(wf_diagnostics_cli).

-export([command/0, logs_command/1, logs/1]).

-ifdef(TEST).
-export([read_log/3]).
-endif.

command() ->
    Shared = retinue_diagnostics_cli:command(warframe),
    Live = wf_companion_diagnostics_cli:command(),
    Commands = maps:merge(maps:merge(maps:get(commands, Shared),
                                    wf_companion_cli:diagnostic_commands()), maps:get(commands, Live)),
    #{help => "inspect live checks, captures, incidents and logs",
      arguments => maps:get(arguments, Live),
      commands => Commands#{"logs" => logs_command(all), "unresolved" => #{
          help => "show current identity, metadata and asset failures",
          handler => fun(Args) -> show(maps:get(output_format, Args, table)) end}}}.

logs_command(Source) ->
    Selection = case Source of
        all -> [#{name => source, required => false,
                  type => {atom, [all, daemon, companion]}, help => "log source"}];
        _ -> []
    end,
    #{help => "show recent incident logs", handler => fun logs/1,
      defaults => #{source => Source},
      arguments => Selection ++ [
        (retinue_cli_args:option(lines, "lines", {integer, [{min, 1}, {max, 1000}]},
                              "maximum entries per log"))#{default => 50},
        retinue_cli_args:option(file, "file", string, "custom log path (select one source)")]}.

logs(Args) ->
    Sources = case maps:get(source, Args, all) of all -> [daemon, companion]; Source -> [Source] end,
    case length(Sources) > 1 andalso maps:is_key(file, Args) of
        true -> fail("select daemon or companion with --file");
        false -> ok
    end,
    Reports = [read_log(Source, maps:get(file, Args, retinue_incidents:path(Source)), maps:get(lines, Args, 50))
               || Source <- Sources],
    wf_diagnostics_format:print_logs(Reports, maps:get(output_format, Args, table)),
    case lists:any(fun(Report) -> maps:is_key(error, Report) end, Reports) of
        true -> halt(1);
        false -> ok
    end.

read_log(Source, Path, Limit) -> retinue_incidents:read(Source, Path, Limit).

show(Format) ->
    case retinue_client:call(warframe, resolution_issues) of
        {ok, Issues} when is_list(Issues) -> wf_diagnostics_format:print(Issues, Format);
        {ok, {error, Reason}} -> fail(retinue_client:format_error(Reason));
        {error, Reason} -> fail(retinue_client:format_error(Reason))
    end.

fail(Message) ->
    io:format(standard_error, "error: ~ts~n", [Message]),
    halt(1).
