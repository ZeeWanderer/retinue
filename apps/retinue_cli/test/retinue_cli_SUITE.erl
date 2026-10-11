-module(retinue_cli_SUITE).

-export([all/0, cli_main/0, help_scopes/1, warframe_alias/1,
         poe2_companion/1, poe2_observed_json/1, completion_scopes/1]).

-include_lib("eunit/include/eunit.hrl").

all() -> [help_scopes, warframe_alias, poe2_companion, poe2_observed_json, completion_scopes].

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
    contains(Status, <<"PoE2 log observer">>),
    contains(Status, <<"Last-observed state: no observations">>),
    {0, Events} = run("poe2cli", ["companion", "events"], Config),
    contains(Events, <<"No observed PoE2 log events.">>).

poe2_observed_json(Config) ->
    {0, StatusText} = run("poe2fixture", ["companion", "status", "--json", "--utc"], Config),
    Status = jsone:decode(StatusText),
    ?assert(is_integer(maps:get(<<"updated_at">>, Status))),
    ?assertEqual(1, maps:get(<<"revision">>, Status)),
    ?assertEqual(<<"/logs/Client.txt">>, maps:get(<<"source">>, Status)),
    [Event] = maps:get(<<"events">>, Status),
    ?assertEqual(<<"2026/10/11 03:04:05">>, maps:get(<<"timestamp_local">>, Event)),
    ?assertEqual(10, maps:get(<<"offset">>, Event)),
    {0, EventsText} = run("poe2fixture", ["companion", "events", "--json", "--utc"], Config),
    ?assertEqual([Event], jsone:decode(EventsText)).

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
    ok = retinue_test_daemon:start(filename:join(Root, "daemon")),
    case Name of
        "retinue" -> retinue_cli:main(retinue, Args);
        "wfcli" -> retinue_cli:main(warframe, Args);
        "poe2cli" -> retinue_cli:main(poe2, Args);
        "poe2fixture" ->
            Event = #{<<"kind">> => <<"area_name">>, <<"area_name">> => <<"The Riverbank">>,
                      <<"timestamp_local">> => <<"2026/10/11 03:04:05">>,
                      <<"client_pid">> => 42, <<"tick_ms">> => 1000, <<"offset">> => 10},
            Batch = #{<<"source">> => <<"/logs/Client.txt">>, <<"generation">> => <<"fixture">>,
                      <<"cursor">> => 10, <<"events">> => [Event]},
            {ok, _} = retinue_poe2_service:request(
                #{<<"op">> => <<"publish">>, <<"dataset">> => <<"session">>, <<"data">> => Batch}),
            retinue_cli:main(poe2, Args)
    end,
    halt(0).

contains(Text, Expected) -> ?assertNotEqual(nomatch, binary:match(Text, Expected)).
