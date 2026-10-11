%%%-------------------------------------------------------------------
%% Thin Gun adapter for the Warframe.market WebSocket.
%%%-------------------------------------------------------------------
-module(wfcli_market_socket).

-export([open/0, upgrade/1, send/3, close/1]).

-doc "Open the WFM TLS connection with Gun reconnect disabled.".
-spec open() -> {ok, pid()} | {error, term()}.
open() ->
    gun:open("ws.warframe.market", 443,
             #{protocols => [http], retry => 0,
               ws_opts => #{keepalive => 30000}}).

-doc "Upgrade a connected Gun process using the required WFM subprotocol.".
-spec upgrade(pid()) -> gun:stream_ref().
upgrade(Connection) ->
    gun:ws_upgrade(Connection, "/socket", [],
                   #{protocols => [{<<"wfm">>, gun_ws_h}],
                     keepalive => 30000}).

-doc "Encode and send one WFM message.".
-spec send(pid(), gun:stream_ref(), map()) -> ok.
send(Connection, Stream, Message) ->
    gun:ws_send(Connection, Stream, {text, jsone:encode(Message)}).

-doc "Close a Gun connection without waiting for WFM.".
-spec close(pid()) -> ok.
close(Connection) ->
    gun:close(Connection).
