-module(wf_companion_diagnostics_eunit).
-include_lib("eunit/include/eunit.hrl").

target_and_admission_test() ->
    ?assertMatch({{error, no_companion_connected}, #{}},
                 submit(#{action => status}, #{})),
    Peer = spawn(fun() -> receive stop -> ok end end),
    try
        Connections = (connections())#{Peer => #{client => <<"wfcompanion">>, os_pid => 20}},
        ?assertMatch({{error, ambiguous_companion}, #{}}, submit(#{action => status}, Connections)),
        ?assertMatch({{error, companion_diagnostics_unavailable}, #{}},
                     submit(#{action => status, pid => 20}, Connections)),
        {{ok, Ref}, Pending} = submit(#{action => status, pid => 10}, Connections),
        {Id, _} = command(),
        ?assertEqual(32, byte_size(Id)),
        ?assertMatch({companion_diagnostics, _}, Ref),
        ?assertEqual(#{}, wf_companion_diagnostics:cancel(Ref, Pending)),
        ?assertEqual({Id, #{<<"action">> => <<"cancel">>}}, command())
    after Peer ! stop end,
    ?assertMatch({{error, invalid_diagnostic_request}, #{}}, submit(watch(0), connections())),
    ?assertMatch({{error, invalid_diagnostic_request}, #{}}, submit(watch(1801), connections())),
    ?assertMatch({{error, diagnostics_busy}, _},
                 wf_companion_diagnostics:submit(self(), #{action => status}, connections(),
                     maps:from_list([{N, #{}} || N <- lists:seq(1, 16)]))).

stream_credit_and_terminal_test() ->
    {{ok, Ref}, Pending} = submit(watch(60), connections()),
    {Id, _} = command(),
    Data = #{<<"state">> => <<"running">>, <<"samples">> => 1},
    ?assertEqual(Pending, wf_companion_diagnostics:reply(other, Id, Data, Pending)),
    Running = wf_companion_diagnostics:reply(self(), Id, Data, Pending),
    ?assertEqual({ok, Data}, result(Ref)),
    ?assertEqual(Running, wf_companion_diagnostics:reply(self(), Id, Data, Running)),
    ?assertEqual(Running, wf_companion_diagnostics:consume(other, Ref, Running)),
    Ready = wf_companion_diagnostics:consume(self(), Ref, Running),
    ?assertEqual({Id, #{<<"action">> => <<"credit">>}}, command()),
    Waiting = wf_companion_diagnostics:reply(self(), Id, Data, Ready),
    ?assertEqual({ok, Data}, result(Ref)),
    Done = #{<<"state">> => <<"completed">>},
    ?assertEqual(#{}, wf_companion_diagnostics:reply(self(), Id, Done, Waiting)),
    ?assertEqual({ok, Done}, result(Ref)).

timeout_and_disconnect_test() ->
    {{ok, Ref}, Pending} = submit(watch(60), connections()),
    {Id, _} = command(),
    #{Id := #{timer := OldTimer}} = Pending,
    Running = wf_companion_diagnostics:reply(self(), Id, #{<<"state">> => <<"running">>}, Pending),
    result(Ref),
    ?assertEqual(Running, wf_companion_diagnostics:timeout(Id, OldTimer, Running)),
    #{Id := #{timer := Timer}} = Running,
    ?assertEqual(#{}, wf_companion_diagnostics:timeout(Id, Timer, Running)),
    ?assertEqual({error, companion_diagnostic_timeout}, result(Ref)),
    ?assertEqual({Id, #{<<"action">> => <<"cancel">>}}, command()),
    {{ok, Ref2}, Pending2} = submit(#{action => status}, connections()),
    command(),
    ?assertEqual(#{}, wf_companion_diagnostics:down(make_ref(), self(), Pending2)),
    ?assertEqual({error, companion_disconnected}, result(Ref2)).

client_exit_cancels_watch_test() ->
    Client = spawn(fun() -> receive stop -> ok end end),
    {{ok, _}, Pending} = wf_companion_diagnostics:submit(Client, watch(60), connections(), #{}),
    {Id, _} = command(),
    #{Id := #{monitor := Monitor}} = Pending,
    Client ! stop,
    receive {'DOWN', Monitor, process, Client, normal} -> ok after 1000 -> error(no_down) end,
    ?assertEqual(#{}, wf_companion_diagnostics:down(Monitor, Client, Pending)),
    ?assertEqual({Id, #{<<"action">> => <<"cancel">>}}, command()).

cli_watch_arguments_test() ->
    ?assertMatch(#{pid := 42, seconds := 10, output_format := json},
        wf_test_cli:parse(["companion", "diagnostics", "watch", "inventory",
                              "--seconds", "10", "--pid", "42", "--json"])),
    ?assertMatch({error, _, _, _}, wf_test_cli:result(
        ["companion", "diagnostics", "watch", "inventory", "--seconds", "0"])),
    ?assertEqual({help, ["companion", "diagnostics", "watch", "inventory"]},
        wf_test_cli:result(["companion", "diagnostics", "watch", "inventory", "help"])).

submit(Request, Connections) -> wf_companion_diagnostics:submit(self(), Request, Connections, #{}).
watch(Seconds) -> #{action => watch, topic => inventory, seconds => Seconds}.
connections() -> #{self() => #{client => <<"wfcompanion">>, os_pid => 10,
                              features => [<<"companion.diagnostics">>]}}.
command() -> receive {companion_diagnostics, Id, Command} -> {Id, Command}
             after 1000 -> error(no_command) end.
result(Ref) -> receive {retinue_daemon, Ref, Reply} -> Reply after 1000 -> error(no_reply) end.
