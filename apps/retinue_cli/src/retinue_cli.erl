-module(retinue_cli).

-export([main/1, main/2, command/0, context/0]).

main(Args) ->
    case os:getenv("RETINUE_CLI_SCOPE") of
        "warframe" -> main(warframe, Args);
        "poe2" -> main(poe2, Args);
        "arc" -> main(arc, Args);
        _ -> main(retinue, Args)
    end.

main(Scope, []) -> main(Scope, ["--help"]);
main(Scope, Args) ->
    case application:ensure_all_started(retinue_cli) of
        {ok, _} -> ok;
        {error, Reason} -> retinue_cli_runtime:fail(io_lib:format("failed to start retinue: ~p", [Reason]))
    end,
    ok = application:set_env(retinue_cli, use_daemon, true),
    retinue_cli_runtime:run(scoped_args(Scope, Args), context()).

scoped_args(retinue, Args) -> Args;
scoped_args(Scope, Args) when Scope =:= warframe; Scope =:= poe2; Scope =:= arc ->
    [atom_to_list(Scope) | Args].

context() ->
    #{tree => command(), progname => "retinue",
      groups => [{"Games", ["warframe", "poe2", "arc"]},
                 {"Platform", platform_commands()}]}.

command() ->
    Platform = #{"daemon" => retinue_daemon_cli:command(),
                 "paths" => retinue_path_cli:command(),
                 "diagnostics" => retinue_diagnostics_cli:command(),
                 "completion" => retinue_completion:command(),
                 "mcp" => retinue_mcp_cli:command(),
                 "help" => retinue_help:command()},
    WarframeScope = scope("warframe", wfcli_cli:command()),
    Poe2Scope = scope("poe2", #{commands => #{"companion" => retinue_poe2_cli:command()}}),
    ArcScope = scope("arc", #{commands => #{"companion" => retinue_arc_cli:command()}}),
    #{arguments => [(retinue_cli_args:flag(output_format, "json", "print JSON (UTC timestamps)"))#{
                       action => {store, json}},
                    retinue_cli_args:flag(utc, "utc", "display timestamps in UTC"),
                    retinue_cli_args:flag(no_suggest_prompt, "no-suggest-prompt",
                                         "do not prompt to correct spelling")],
      commands => Platform#{
        "warframe" => WarframeScope#{help => "Warframe tools, data and companion"},
        "poe2" => Poe2Scope#{help => "Path of Exile 2 companion"},
        "arc" => ArcScope#{help => "ARC Raiders companion scaffold"}}}.

platform_commands() -> ["daemon", "paths", "diagnostics", "completion", "mcp", "help"].

scope(Name, Node) ->
    Commands = maps:get(commands, Node),
    Node#{commands := Commands#{"help" => retinue_help:command([Name]),
                                "completion" => retinue_completion:command([Name])}}.
