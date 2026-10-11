-module(wf_cli).
-export([command/0, command_names/0, public_command_names/0, command_groups/0]).

command() ->
    #{commands => maps:from_list(lists:append([Commands || {_, Commands} <- groups()]))}.

groups() ->
    [{"Tools", [{"forma-plan", wf_forma_plan:command()},
                {"build-stats", wf_build_stats_cli:command()},
                {"visualize", wf_visualize:command()},
                {"notifications", wf_notification_cli:command()},
                {"watch", wf_worldstate_cli:watch_command()}]},
     {"Data", [{"query", wf_query_cli:command()},
               {"player", wf_query_cli:player_command()},
               {"market", wf_market_cli:command()}] ++ wf_catalog_cli:commands()},
     {"Worldstate", wf_worldstate_cli:commands()},
     {"Applications", [{"daemon", retinue_daemon_cli:command()},
                       {"companion", wf_companion_cli:command()},
                       {"gui", wf_gui_cli:command()},
                       {"mcp", wf_mcp_cli:command()}]},
     {"Utility", [{"diagnostics", wf_diagnostics_cli:command()},
                  {"update", wf_update_cli:command()},
                  {"completion", retinue_completion:command()},
                  {"paths", retinue_path_cli:command()},
                  {"help", retinue_help:command()}]}].

command_groups() ->
    [{Name, [{Cmd, maps:get(help, Node, "")} || {Cmd, Node} <- Commands,
                                              maps:get(help, Node, "") =/= hidden]}
     || {Name, Commands} <- groups()].

command_names() -> maps:keys(maps:get(commands, command())).
public_command_names() ->
    [Name || {_, Rows} <- command_groups(), {Name, _} <- Rows].
