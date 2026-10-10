-module(wfcli_cli).

-export([main/1, command/0, command_names/0, public_command_names/0,
         command_groups/0, usage/0, fail/1]).

main(Args) ->
    case application:ensure_all_started(wfcli) of
        {ok, _} -> ok;
        {error, Reason} -> fail(io_lib:format("failed to start wfcli: ~p", [Reason]))
    end,
    ok = application:set_env(wfcli, use_daemon, true),
    dispatch(Args).

dispatch(Args) ->
    case wfcli_cli_args:parse(Args) of
        {help, Path} -> wfcli_help:run(Path);
        {ok, Parsed, _Path, #{handler := Handler}} ->
            wfcli_output:with(Parsed, fun() -> invoke(Handler, Parsed) end);
        {error, Path, Message, Detail} ->
            case suggest(Args, Path, Detail) of
                {ok, Corrected} -> dispatch(Corrected);
                none ->
                    io:format(standard_error, "error: ~ts~n", [Message]),
                    io:put_chars(standard_error, wfcli_help:text(Path)),
                    halt(2)
            end
    end.

invoke({Module, Function}, Parsed) -> Module:Function(Parsed);
invoke(Function, Parsed) -> Function(Parsed).

command() ->
    #{commands => maps:from_list(lists:append([Commands || {_, Commands} <- groups()])),
      arguments => [(wfcli_cli_args:flag(output_format, "json", "print JSON (UTC timestamps)"))#{
                        action => {store, json}},
                    wfcli_cli_args:flag(utc, "utc", "display timestamps in UTC"),
                    wfcli_cli_args:flag(no_suggest_prompt, "no-suggest-prompt",
                                       "do not prompt to correct spelling")]}.

groups() ->
    [{"Tools", [{"forma-plan", wfcli_forma_plan:command()},
                {"build-stats", wfcli_build_stats_cli:command()},
                {"visualize", wfcli_visualize:command()},
                {"notifications", wfcli_notification_cli:command()},
                {"watch", wfcli_worldstate_cli:watch_command()}]},
     {"Data", [{"query", wfcli_query_cli:command()},
               {"player", wfcli_query_cli:player_command()},
               {"market", wfcli_market_cli:command()}] ++ wfcli_catalog_cli:commands()},
     {"Worldstate", wfcli_worldstate_cli:commands()},
     {"Applications", [{"daemon", wfcli_daemon_cli:command()},
                       {"companion", wfcli_companion_cli:command()},
                       {"gui", wfcli_gui_cli:command()},
                       {"mcp", wfcli_mcp_cli:command()}]},
     {"Utility", [{"diagnostics", wfcli_diagnostics_cli:command()},
                  {"update", wfcli_update_cli:command()},
                  {"completion", wfcli_completion:command()},
                  {"paths", wfcli_path_cli:command()},
                  {"help", wfcli_help:command()}]}].

command_groups() ->
    [{Name, [{Cmd, maps:get(help, Node, "")} || {Cmd, Node} <- Commands,
                                              maps:get(help, Node, "") =/= hidden]}
     || {Name, Commands} <- groups()].

command_names() -> maps:keys(maps:get(commands, command())).
public_command_names() ->
    [Name || {_, Rows} <- command_groups(), {Name, _} <- Rows].

usage() -> io:put_chars(wfcli_help:text([])).

fail(Message) ->
    io:format(standard_error, "error: ~ts~n", [Message]),
    halt(1).

suggest(Args, Path, {undefined, Unknown}) when is_list(Unknown) ->
    case lists:member("--no-suggest-prompt", lists:takewhile(fun(A) -> A =/= "--" end, Args))
         orelse not wfcli_cli_args:interactive() of
        true -> none;
        false ->
            Node = wfcli_cli_args:node(Path, command()),
            Candidates = maps:keys(maps:get(commands, Node, #{})) ++
                         wfcli_cli_args:options(Node),
            suggest_unknown(Args, Unknown, Candidates)
    end;
suggest(_Args, _Path, _Detail) -> none.

suggest_unknown(Args, Unknown, Candidates) ->
    case {length([A || A <- Args, A =:= Unknown]),
          wfcli_cli_suggest:suggest_match(Unknown, Candidates)} of
        {1, {ok, Replacement}} when Replacement =/= Unknown ->
            io:format(standard_error, "unknown argument: ~s. use ~s? [enter to accept] ",
                      [Unknown, Replacement]),
            case io:get_line("") of
                Line when is_list(Line) ->
                    case string:lowercase(string:trim(Line)) of
                        Answer when Answer =:= ""; Answer =:= "y"; Answer =:= "yes" ->
                            {ok, [case A of Unknown -> Replacement; _ -> A end || A <- Args]};
                        _ -> none
                    end;
                _ -> none
            end;
        _ -> none
    end.
