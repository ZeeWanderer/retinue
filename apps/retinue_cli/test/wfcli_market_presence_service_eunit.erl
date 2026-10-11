%%%-------------------------------------------------------------------
%% EUnit coverage for Market WebSocket presence lifecycle.
%%%-------------------------------------------------------------------
-module(wfcli_market_presence_service_eunit).

-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

market_presence_service_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(State) -> fun() -> exercise(State) end end}.

setup() ->
    Root = filename:join(
             "/tmp", "wfcli-market-presence-" ++
                 integer_to_list(erlang:unique_integer([positive]))),
    Path = filename:join(Root, "presence.json"),
    application:set_env(retinue_daemon, market_presence_file, Path),
    application:set_env(retinue_daemon, market_socket_module, wfcli_test_market_socket),
    {ok, _Service} = wfcli_market_presence_service:start_link(),
    #{root => Root, path => Path}.

cleanup(#{root := Root}) ->
    stop(wfcli_market_presence_service),
    application:unset_env(retinue_daemon, market_presence_file),
    application:unset_env(retinue_daemon, market_socket_module),
    application:unset_env(retinue_daemon, market_socket_test_process),
    _ = file:del_dir_r(Root),
    ok.

exercise(#{path := Path}) ->
    application:set_env(retinue_daemon, market_socket_test_process, self()),
    ?assertMatch(#{<<"authenticated">> := false,
                   <<"mode">> := <<"invisible">>},
                 wfcli_market_presence_service:snapshot()),

    ok = wfcli_market_presence_service:token_changed(<<"jwt-token">>),
    {Service, Connection, Stream, Auth} = next_message(),
    ?assertEqual(<<"@wfm|cmd/auth/signIn">>, maps:get(<<"route">>, Auth)),
    ?assertEqual(<<"jwt-token">>,
                 maps:get(<<"token">>, maps:get(<<"payload">>, Auth))),

    socket_message(Service, Connection, Stream,
                   #{<<"route">> => <<"@wfm|cmd/auth/signIn:ok">>}),
    socket_message(Service, Connection, Stream,
                   #{<<"route">> => <<"@wfm|event/status/set">>,
                     <<"payload">> => #{<<"status">> => <<"invisible">>}}),
    wait_until(fun() ->
        maps:get(<<"connected">>, wfcli_market_presence_service:snapshot())
    end),

    {ok, Pending} = wfcli_market_presence_service:set_mode(<<"online">>),
    ?assertEqual(<<"online">>, maps:get(<<"desired_status">>, Pending)),
    {_Service2, Connection, Stream, StatusCommand} = next_message(),
    ?assertEqual(<<"@wfm|cmd/status/set">>,
                 maps:get(<<"route">>, StatusCommand)),
    ?assertEqual(<<"online">>,
                 maps:get(<<"status">>, maps:get(<<"payload">>, StatusCommand))),
    socket_message(Service, Connection, Stream,
                   #{<<"route">> => <<"@wfm|cmd/status/set:ok">>,
                     <<"payload">> => #{<<"status">> => <<"online">>}}),
    wait_until(fun() ->
        maps:get(<<"status">>, wfcli_market_presence_service:snapshot()) =:=
            <<"online">>
    end),
    {ok, FileInfo} = file:read_file_info(Path),
    ?assertEqual(8#600, FileInfo#file_info.mode band 8#777),

    ok = wfcli_market_presence_service:token_changed(undefined),
    wait_until(fun() ->
        maps:get(<<"authenticated">>, wfcli_market_presence_service:snapshot()) =:= false
    end).

next_message() ->
    receive
        {market_socket_send, Service, Connection, Stream, Message} ->
            {Service, Connection, Stream, Message}
    after 2000 -> error(market_socket_message_timeout)
    end.

socket_message(Service, Connection, Stream, Message) ->
    Service ! {gun_ws, Connection, Stream, {text, jsone:encode(Message)}}.

wait_until(Fun) -> wait_until(Fun, 100).
wait_until(_Fun, 0) -> error(market_presence_timeout);
wait_until(Fun, Attempts) ->
    case Fun() of
        true -> ok;
        false -> timer:sleep(10), wait_until(Fun, Attempts - 1)
    end.

stop(Name) ->
    case whereis(Name) of
        undefined -> ok;
        _Pid -> gen_server:stop(Name)
    end.
