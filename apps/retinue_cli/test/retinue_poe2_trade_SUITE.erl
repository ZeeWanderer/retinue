-module(retinue_poe2_trade_SUITE).
-export([all/0, supervised_restart/1, anonymous_search/1]).
-include_lib("eunit/include/eunit.hrl").

all() -> [supervised_restart, anonymous_search].

supervised_restart(Config) ->
    Old = application:get_env(retinue_daemon, poe2_session_file),
    application:set_env(retinue_daemon, poe2_session_file,
                        filename:join(proplists:get_value(priv_dir, Config), "session.json")),
    {ok, Sup} = retinue_poe2_sup:start_link(),
    unlink(Sup),
    try
        Session = whereis(retinue_poe2_service),
        Trade = whereis(retinue_poe2_trade),
        ?assert(is_pid(whereis(retinue_poe2_catalog))),
        ?assertEqual(retinue_poe2_sup, retinue_sup:owner(retinue_poe2_catalog)),
        ?assert(is_pid(Trade)),
        ?assertEqual(retinue_poe2_sup, retinue_sup:owner(retinue_poe2_trade)),
        ?assert(is_pid(whereis(retinue_poe2_prediction))),
        ?assertEqual(retinue_poe2_sup, retinue_sup:owner(retinue_poe2_prediction)),
        Ref = monitor(process, Trade),
        exit(Trade, kill),
        receive {'DOWN', Ref, process, Trade, killed} -> ok after 1000 -> error(not_stopped) end,
        await_restart(Trade, 100),
        ?assertEqual(Session, whereis(retinue_poe2_service)),
        ?assertMatch(#{<<"schema">> := 2}, retinue_poe2_service:status())
    after
        gen_server:stop(Sup),
        case Old of {ok, Value} -> application:set_env(retinue_daemon, poe2_session_file, Value);
                    undefined -> application:unset_env(retinue_daemon, poe2_session_file) end
    end.

await_restart(_, 0) -> error(not_restarted);
await_restart(Old, Attempts) ->
    case whereis(retinue_poe2_trade) of
        Pid when is_pid(Pid), Pid =/= Old -> ok;
        _ -> timer:sleep(10), await_restart(Old, Attempts - 1)
    end.

anonymous_search(_) ->
    {ok, Server} = retinue_poe2_trade:start_link(#{name => undefined,
        enrich => fun retinue_poe2_trade_profiles:prepare/1,
        http => fun retinue_poe2_trade_eunit:http/4}),
    try
        Text = <<"Item Class: Helmets\nRarity: Unique\nGoldrim\nFelt Cap\n--------\n",
                 "Item Level: 77\n--------\n+31(25-40)% to all Elemental Resistances">>,
        {ok, Evaluation} = retinue_poe2_trade:request(Server, #{<<"op">> => <<"trade_item">>, <<"text">> => Text}),
        ?assertEqual(<<"Standard">>, maps:get(<<"league">>, Evaluation)),
        [Modifier] = maps:get(<<"modifiers">>, maps:get(<<"item">>, Evaluation)),
        ?assertMatch(#{<<"ranges">> := #{<<"allmods">> := #{<<"min">> := 31},
                                       <<"fuzzy">> := #{<<"min">> := 28}}}, Modifier),
        Filter = (maps:get(<<"fuzzy">>, maps:get(<<"ranges">>, Modifier)))#{
            <<"id">> => maps:get(<<"id">>, Modifier)},
        {ok, Data} = retinue_poe2_trade:request(Server, #{<<"op">> => <<"trade_search">>,
            <<"text">> => Text, <<"league">> => <<"Standard">>, <<"status">> => <<"securable">>,
            <<"base">> => true, <<"filters">> => [Filter]}),
        Encoded = iolist_to_binary(retinue_local_protocol:encode(#{<<"id">> => 1,
            <<"ok">> => true, <<"dataset">> => <<"trade">>, <<"data">> => Data})),
        ?assertMatch({ok, #{<<"data">> := #{<<"total">> := 25, <<"listings">> := [_]}}},
                     retinue_local_protocol:decode(Encoded))
    after gen_server:stop(Server) end.
