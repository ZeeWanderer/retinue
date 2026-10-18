-module(retinue_cli_SUITE).

-export([all/0, cli_main/0, help_scopes/1, warframe_alias/1,
         poe2_companion/1, poe2_observed_json/1, completion_scopes/1, health_commands/1]).

-include_lib("eunit/include/eunit.hrl").

all() -> [help_scopes, warframe_alias, poe2_companion, poe2_observed_json, completion_scopes, health_commands].

health_commands(Config) ->
    {0, Text} = run("retinue", ["doctor"], Config),
    contains(Text, <<"2 checks have not run or are waiting for the game.">>),
    contains(Text, <<"Incident history: no unresolved adapter failures">>),
    {0, Json} = run("wfcli", ["doctor", "--json"], Config),
    ?assertMatch(#{<<"status">> := <<"incomplete">>, <<"failures">> := 0}, jsone:decode(Json)),
    {0, Incidents} = run("retinue", ["diagnostics", "incidents", "--json"], Config),
    ?assertMatch(#{<<"checks">> := [], <<"retention">> := #{<<"max_checks">> := 512}}, jsone:decode(Incidents)),
    {1, Unavailable} = run("checks-corrupt", ["diagnostics", "incidents"], Config),
    contains(Unavailable, <<"Stored check history is unavailable">>),
    contains(Unavailable, <<"Use doctor for live health.">>),
    {1, CorruptDoctor} = run("checks-corrupt", ["doctor"], Config),
    contains(CorruptDoctor, <<"could not verify stored failures">>),
    {1, FailedDoctor} = run("checks-open", ["doctor", "--json"], Config),
    ?assertMatch(#{<<"status">> := <<"failed">>, <<"failures">> := 1}, jsone:decode(FailedDoctor)),
    {0, RecoveredDoctor} = run("checks-recovered", ["doctor", "--json"], Config),
    ?assertMatch(#{<<"status">> := <<"incomplete">>, <<"failures">> := 0}, jsone:decode(RecoveredDoctor)),
    {0, RecoveredIncidents} = run("checks-recovered", ["diagnostics", "incidents", "--json"], Config),
    ?assertMatch(#{<<"checks">> := []}, jsone:decode(RecoveredIncidents)),
    {0, RecoveryHistory} = run("checks-recovered", ["diagnostics", "incidents", "--all", "--json"], Config),
    ?assertMatch(#{<<"checks">> := [#{<<"last_result">> := <<"resolved">>}]}, jsone:decode(RecoveryHistory)),
    {0, WFHelp} = run("wfcli", ["--help"], Config),
    contains(WFHelp, <<"Everyday:">>),
    contains(WFHelp, <<"doctor">>),
    {0, Canonical} = run("wfcli", ["diagnostics", "capture", "status", "--help"], Config),
    contains(Canonical, <<"capture status">>).

help_scopes(Config) ->
    {0, Root} = run("retinue", ["--help"], Config),
    {0, Root} = run("retinue", [], Config),
    contains(Root, <<"retinue <command>">>),
    contains(Root, <<"warframe">>),
    contains(Root, <<"poe2">>),
    {0, Warframe} = run("retinue", ["warframe", "mods", "--help"], Config),
    {0, Warframe} = run("wfcli", ["mods", "--help"], Config),
    contains(Warframe, <<"retinue warframe mods">>),
    {0, Poe2} = run("poe2cli", ["companion", "status", "--help"], Config),
    contains(Poe2, <<"retinue poe2 companion status">>),
    {0, Query} = run("wfcli", ["--json", "help", "query-syntax"], Config),
    contains(maps:get(<<"help">>, jsone:decode(Query)), <<"retinue warframe query">>).

warframe_alias(Config) ->
    Exports = filename:join([code:lib_dir(retinue_cli), "test", "fixtures", "exports"]),
    Args = ["mods", "--exports-dir", Exports, "--name", "help", "--json"],
    {0, Output} = run("wfcli", Args, Config),
    {0, Output} = run("retinue", ["warframe" | Args], Config),
    ?assertEqual([], maps:get(<<"results">>, jsone:decode(Output))),
    {2, Error} = run("retinue", ["mods"], Config),
    contains(Error, <<"error:">>).

poe2_companion(Config) ->
    lists:foreach(fun(Action) ->
        {0, Output} = run("poe2cli", ["--json", "companion", Action], Config),
        case Action of
            "status" -> ?assertMatch(#{<<"game">> := <<"poe2">>}, jsone:decode(Output));
            "events" -> ?assertEqual([], jsone:decode(Output))
        end,
        {0, Canonical} = run("retinue", ["poe2", "companion", Action, "--json"], Config),
        ?assertEqual(jsone:decode(Output), jsone:decode(Canonical))
    end, ["status", "events"]),
    {0, Status} = run("retinue", ["poe2", "companion", "status"], Config),
    contains(Status, <<"PoE2 native session">>),
    contains(Status, <<"Native state: no observation">>),
    {0, Events} = run("poe2cli", ["companion", "events"], Config),
    contains(Events, <<"No observed PoE2 session events.">>).

poe2_observed_json(Config) ->
    {0, StatusText} = run("poe2fixture", ["companion", "status", "--json", "--utc"], Config),
    Status = jsone:decode(StatusText),
    ?assert(is_integer(maps:get(<<"updated_at">>, Status))),
    ?assertEqual(1, maps:get(<<"revision">>, Status)),
    ?assertEqual(2, maps:get(<<"schema">>, Status)),
    ?assertEqual(<<"fixture">>, maps:get(<<"stream">>, Status)),
    ?assertEqual(retinue_poe2_fixture:snapshot(), maps:get(<<"current">>, Status)),
    Events = maps:get(<<"events">>, Status),
    ?assertEqual(7, length(Events)),
    ?assert(lists:all(fun(E) -> maps:get(<<"observed_at">>, E) =:= 1000 end, Events)),
    {0, EventsText} = run("poe2fixture", ["companion", "events", "--json", "--utc"], Config),
    ?assertEqual(Events, jsone:decode(EventsText)).

completion_scopes(Config) ->
    lists:foreach(fun({Name, Prefix, Args, Expected}) ->
        {0, Output} = run(Name, Prefix ++ ["completion", "candidates", "--" | Args], Config),
        ?assertEqual(Expected, Output)
    end, [{"retinue", [], ["wa"], <<"warframe\n">>},
          {"retinue", ["warframe"], ["mods", "--pol"], <<"--polarity\n">>},
          {"wfcli", [], ["mods", "--pol"], <<"--polarity\n">>},
          {"poe2cli", [], ["companion", "ev"], <<"events\n">>}]).

run(Name, Args, _Config) ->
    Root = filename:join("/tmp", "retinue-cli-" ++ os:getpid() ++ "-" ++
                         integer_to_list(erlang:unique_integer([positive]))),
    Erl = filename:join([code:root_dir(), "bin", "erl"]),
    Paths = [filename:absname(Path) || Path <- code:get_path(), filelib:is_dir(Path)],
    Command = ["+S", "2:2", "+SDcpu", "1", "+SDio", "1", "-noshell", "-pa" | lists:reverse(Paths)] ++
              ["-s", atom_to_list(?MODULE), "cli_main", "-extra", Root, Name | Args],
    Env = [{Key, filename:join(Root, Dir)} || {Key, Dir} <-
             [{"XDG_CACHE_HOME", "cache"}, {"XDG_STATE_HOME", "state"},
              {"XDG_DATA_HOME", "data"}, {"XDG_CONFIG_HOME", "config"}]],
    Port = open_port({spawn_executable, Erl},
                     [binary, exit_status, stderr_to_stdout, {env, Env}, {args, Command}]),
    try collect(Port, [], erlang:monotonic_time(millisecond) + 30000)
    after
        case erlang:port_info(Port) of undefined -> ok; _ -> port_close(Port) end,
        file:del_dir_r(Root)
    end.

collect(Port, Acc, Deadline) ->
    receive
        {Port, {data, Data}} -> collect(Port, [Data | Acc], Deadline);
        {Port, {exit_status, Status}} -> {Status, iolist_to_binary(lists:reverse(Acc))}
    after max(0, Deadline - erlang:monotonic_time(millisecond)) ->
        ct:fail({cli_timeout, iolist_to_binary(lists:reverse(Acc))})
    end.

cli_main() ->
    [Root, Name | Args] = init:get_plain_arguments(),
    DaemonRoot = filename:join(Root, "daemon"),
    case Name of
        "checks-corrupt" ->
            Path = filename:join(DaemonRoot, "checks.json"),
            ok = filelib:ensure_dir(Path),
            ok = file:write_file(Path, <<"invalid JSON">>);
        _ -> ok
    end,
    ok = retinue_test_daemon:start(DaemonRoot),
    case Name of
        "retinue" -> retinue_cli:main(retinue, Args);
        "checks-corrupt" -> retinue_cli:main(retinue, Args);
        Name when Name =:= "checks-open"; Name =:= "checks-recovered" ->
            Time = erlang:system_time(millisecond),
            Context = #{<<"version">> => <<"0.1.0">>, <<"session">> => <<"previous">>},
            retinue_checks:observe(<<"warframe.companion">>, Context,
                #{<<"relic_rewards">> => #{<<"state">> => <<"fail">>, <<"checked_at">> => Time - 1000,
                                          <<"error">> => <<"anchor not found">>}}),
            case Name of
                "checks-recovered" -> retinue_checks:observe(<<"warframe.companion">>, Context,
                    #{<<"relic_rewards">> => #{<<"state">> => <<"pass">>, <<"checked_at">> => Time}});
                _ -> ok
            end,
            _ = retinue_checks:report(),
            retinue_cli:main(warframe, Args);
        "wfcli" -> retinue_cli:main(warframe, Args);
        "poe2cli" -> retinue_cli:main(poe2, Args);
        "poe2fixture" ->
            Sample = retinue_poe2_fixture:sample(1),
            {ok, _} = retinue_poe2_service:request(
                #{<<"op">> => <<"publish">>, <<"dataset">> => <<"session">>, <<"data">> => Sample}),
            retinue_cli:main(poe2, Args)
    end,
    halt(0).

contains(Text, Expected) -> ?assertNotEqual(nomatch, binary:match(Text, Expected)).
