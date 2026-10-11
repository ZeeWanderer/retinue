-module(wfcli_process_SUITE).

-export([all/0, cli_main/0, failed_plan_preserves_output/1, partial_plan_exits_nonzero/1,
         partial_query_prints_success/1, empty_json_arrays/1, literal_help/1,
         invalid_options_do_not_prompt/1, malformed_mcp_stays_alive/1,
         circuit_commands/1, structured_extract/1, contextual_help/1, help_option_value/1]).

-include_lib("eunit/include/eunit.hrl").

all() -> [failed_plan_preserves_output, partial_plan_exits_nonzero,
          partial_query_prints_success, empty_json_arrays, literal_help,
          invalid_options_do_not_prompt, malformed_mcp_stays_alive,
          circuit_commands, structured_extract, contextual_help, help_option_value].

failed_plan_preserves_output(Config) ->
    Output = filename:join(proplists:get_value(priv_dir, Config), "existing.plan.yml"),
    ok = file:write_file(Output, <<"keep existing plan\n">>),
    Args = ["forma-plan", "--config", fixture("no_solution.yml"), "--output", Output],
    {1, Stdout, _} = run(Args, Config),
    contains(Stdout, <<"no_plan">>),
    ?assertEqual(nomatch, binary:match(Stdout, <<"plan output:">>)),
    ?assertEqual({ok, <<"keep existing plan\n">>}, file:read_file(Output)),
    ok = file:delete(Output),
    {1, _, _} = run(Args, Config),
    ?assertNot(filelib:is_file(Output)).

partial_plan_exits_nonzero(Config) ->
    Output = filename:join(proplists:get_value(priv_dir, Config), "partial.plan.yml"),
    {1, _, _} = run(["forma-plan", "--config", fixture("simple_capacity.yml"),
                     "--config", fixture("no_solution.yml"), "--output", Output], Config),
    {ok, Bin} = file:read_file(Output),
    {ok, [Plan]} = wfcli_visualize:load_plan(Bin),
    ?assertEqual(fixture("simple_capacity.yml"), maps:get(config, Plan)).

partial_query_prints_success(Config) ->
    Missing = filename:join(proplists:get_value(priv_dir, Config), "missing-exports"),
    {1, Out, _} = run(["query", "dataset=mods|drops", "--exports-dir", Missing,
                       "--knowledge-dir", fixture("knowledge"), "--limit", "1"], Config),
    contains(Out, <<"== Drops ==">>),
    contains(Out, <<"showing 1">>).

empty_json_arrays(Config) ->
    Commands = [["items", "--exports-dir", fixture("exports"), "--name", "no_such_item_98765"],
                ["drops", "no_such_item_98765", "--knowledge-dir", fixture("knowledge")]],
    lists:foreach(fun(Args) ->
        {0, Out, _} = run(Args ++ ["--format", "json"], Config),
        Json = jsone:decode(Out),
        ?assertEqual([], maps:get(<<"results">>, Json)),
        ?assertEqual(0, maps:get(<<"count">>, Json))
    end, Commands).

literal_help(Config) ->
    {0, Out, _} = run(["query", "dataset=mods", "--exports-dir", fixture("exports"),
                       "--", "--help"], Config),
    ?assertEqual(nomatch, binary:match(Out, <<"USAGE:">>)),
    contains(Out, <<"== Mods ==">>).

invalid_options_do_not_prompt(Config) ->
    lists:foreach(fun(Args) ->
        {2, Out, Err} = run(Args, Config),
        ?assertEqual(<<>>, Out),
        contains(Err, <<"error:">>),
        ?assertEqual(nomatch, binary:match(<<Out/binary, Err/binary>>, <<"enter to accept">>))
    end, [["--no-suggest-prompt", "mods", "--limti", "1"],
          ["mods", "--limti", "1"], ["fissures", "--xyz-invalid-option"],
          ["items", "--polarity", "V"], ["query", "dataset=mods", "--ttl", "60junk"],
          ["companion", "start", "unexpected"], ["daemon", "start", "--idle-timeout"]]).

contextual_help(Config) ->
    lists:foreach(fun(Args) ->
        {0, Out, Err} = run(Args, Config),
        contains(Out, <<"--idle-timeout">>),
        ?assertEqual(<<>>, Err)
    end, [["daemon", "start", Ending] || Ending <- ["help", "--help", "-h"]] ++
         [["--no-suggest-prompt", "help", "daemon", "start"]]).

help_option_value(Config) ->
    {0, Out, Err} = run(["mods", "--exports-dir", fixture("exports"),
                         "--format", "json", "--name", "help"], Config),
    ?assertEqual([], maps:get(<<"results">>, jsone:decode(Out))),
    ?assertEqual(<<>>, Err).

malformed_mcp_stays_alive(Config) ->
    Input = <<"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":null}\n"
              "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n">>,
    {0, Out, _} = run(["mcp"], [{input, Input} | Config]),
    [Error, Pong] = [jsone:decode(Line) || Line <- binary:split(Out, <<"\n">>, [global]),
                                         Line =/= <<>>],
    ?assertEqual(-32602, maps:get(<<"code">>, maps:get(<<"error">>, Error))),
    ?assertEqual(2, maps:get(<<"id">>, Pong)),
    ?assertEqual(#{}, maps:get(<<"result">>, Pong)).

circuit_commands(Config) ->
    CacheArgs = cache_args(Config),
    {0, Both, _} = run(["circuit" | CacheArgs], Config),
    contains(Both, <<"Garuda">>),
    contains(Both, <<"Gammacor">>),
    {0, Both, _} = run(["endless-xp" | CacheArgs], Config),
    {0, Normal, _} = run(["circuit", "normal" | CacheArgs], Config),
    contains(Normal, <<"Garuda">>),
    ?assertEqual(nomatch, binary:match(Normal, <<"Gammacor">>)),
    {0, Steel, _} = run(["circuit", "steel-path" | CacheArgs], Config),
    contains(Steel, <<"Gammacor">>),
    ?assertEqual(nomatch, binary:match(Steel, <<"Garuda">>)),
    ?assertEqual(nomatch, binary:match(Both, <<"Rhino">>)),
    lists:foreach(fun(Command) ->
        {0, Help, _} = run([Command, "steel-path", "--help"], Config),
        contains(Help, <<"Incarnon Genesis">>),
        ?assertEqual(nomatch, binary:match(Help, <<"unknown help topic">>))
    end, ["circuit", "endless-xp"]).

structured_extract(Config) ->
    {0, Out, _} = run(["query", "dataset=worldstate type=raw_worldstate extract=data.EndlessXpSchedule"
                      | cache_args(Config)], Config),
    contains(Out, <<"CategoryChoices">>),
    contains(Out, <<"Garuda">>).

cache_args(Config) ->
    Cache = filename:join(proplists:get_value(priv_dir, Config), "circuit.json"),
    {ok, Bin} = file:read_file(fixture("circuit_schedule.json")),
    Raw = jsone:decode(Bin),
    Delta = (erlang:system_time(second) - maps:get(<<"Time">>, Raw) - 60) * 1000,
    Schedules = [maps:map(fun(Key, Value) ->
        case Key of
            <<"Activation">> -> shift_date(Value, Delta);
            <<"Expiry">> -> shift_date(Value, Delta);
            _ -> Value
        end
    end, Schedule) || Schedule <- maps:get(<<"EndlessXpSchedule">>, Raw)],
    ok = file:write_file(Cache, jsone:encode(Raw#{<<"EndlessXpSchedule">> := Schedules})),
    ["--cache", Cache, "--ttl", "999999999", "--raw"].

shift_date(#{<<"$date">> := #{<<"$numberLong">> := Value}}, Delta) ->
    #{<<"$date">> => #{<<"$numberLong">> => integer_to_binary(binary_to_integer(Value) + Delta)}}.

fixture(Name) -> filename:join([code:lib_dir(retinue_cli), "test", "fixtures", Name]).

contains(Text, Expected) -> ?assertNotEqual(nomatch, binary:match(Text, Expected)).

run(Args, Config) ->
    Root = filename:join("/tmp", "wfcli-process-" ++ os:getpid() ++ "-" ++
                         integer_to_list(erlang:unique_integer([positive]))),
    ok = file:make_dir(Root),
    Input = filename:join(Root, "stdin"),
    Stderr = filename:join(Root, "stderr"),
    ok = file:write_file(Input, proplists:get_value(input, Config, <<>>)),
    Erl = filename:join([code:root_dir(), "bin", "erl"]),
    Paths = [filename:absname(Path) || Path <- code:get_path(), filelib:is_dir(Path)],
    Command = ["+S", "2:2", "+SDcpu", "1", "+SDio", "1", "-noshell", "-pa" | Paths] ++
              ["-s", atom_to_list(?MODULE), "cli_main", "-extra", Root | Args],
    Env = [{"WFCLI_TEST_INPUT", Input}, {"WFCLI_TEST_STDERR", Stderr}] ++
          [{Key, filename:join(Root, Dir)} || {Key, Dir} <-
            [{"XDG_CACHE_HOME", "cache"}, {"XDG_STATE_HOME", "state"},
             {"XDG_DATA_HOME", "data"}, {"XDG_CONFIG_HOME", "config"}]],
    Port = open_port({spawn_executable, os:find_executable("sh")},
                     [binary, exit_status, {env, Env},
                      {args, ["-c", "exec \"$@\" <\"$WFCLI_TEST_INPUT\" 2>\"$WFCLI_TEST_STDERR\"",
                              "wfcli-test", Erl | Command]}]),
    try
        {Code, Output} = collect(Port, [], erlang:monotonic_time(millisecond) + 30000),
        {ok, Error} = file:read_file(Stderr),
        {Code, Output, Error}
    after
        case erlang:port_info(Port) of undefined -> ok; _ -> port_close(Port) end,
        file:del_dir_r(Root)
    end.

collect(Port, Acc, Deadline) ->
    receive
        {Port, {data, Data}} -> collect(Port, [Data | Acc], Deadline);
        {Port, {exit_status, Code}} -> {Code, iolist_to_binary(lists:reverse(Acc))}
    after max(0, Deadline - erlang:monotonic_time(millisecond)) ->
        ct:fail({cli_timeout, iolist_to_binary(lists:reverse(Acc))})
    end.

cli_main() ->
    [Root | Args] = init:get_plain_arguments(),
    ok = wfcli_test_daemon:start(filename:join(Root, "daemon")),
    retinue_cli:main(warframe, Args),
    halt(0).
