-module(wf_test_market_socket).

-export([open/0, upgrade/1, send/3, close/1]).

open() ->
    Owner = self(),
    Connection = spawn(fun connection/0),
    Owner ! {gun_up, Connection, http},
    {ok, Connection}.

upgrade(Connection) ->
    Stream = make_ref(),
    self() ! {gun_upgrade, Connection, Stream, [<<"websocket">>], []},
    Stream.

send(Connection, Stream, Message) ->
    test_process() ! {market_socket_send, self(), Connection, Stream, Message},
    ok.

close(Connection) ->
    Connection ! stop,
    ok.

connection() ->
    receive stop -> ok end.

test_process() ->
    {ok, Pid} = application:get_env(retinue_daemon, market_socket_test_process),
    Pid.
