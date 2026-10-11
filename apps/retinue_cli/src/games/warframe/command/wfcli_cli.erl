-module(wfcli_cli).
-export([command/0, command_names/0, public_command_names/0, command_groups/0]).

command() ->
    #{commands => maps:from_list(lists:append([Commands || {_, Commands} <- groups()]))}.

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
     {"Applications", [{"daemon", retinue_daemon_cli:command()},
                       {"companion", wfcli_companion_cli:command()},
                       {"gui", wfcli_gui_cli:command()},
                       {"mcp", wfcli_mcp_cli:command()}]},
     {"Utility", [{"diagnostics", wfcli_diagnostics_cli:command()},
                  {"update", wfcli_update_cli:command()},
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
