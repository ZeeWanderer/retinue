%%%-------------------------------------------------------------------
%% Request and subscription transport for the wfcli daemon client.
%%%-------------------------------------------------------------------
-module(retinue_client_transport).

-export([call/1, call_running/1, inspect/1, one_shot/1, subscribe/1, next/2, unsubscribe/1]).

-define(REQUEST_TIMEOUT_MS, 120000).
-define(RESUBSCRIBE_RETRIES, 40).
-define(RESUBSCRIBE_SLEEP_MS, 50).

call_running(Request) ->
    case retinue_client:status() of
        {running, Node, Info} ->
            Offered = maps:get(erlang_distribution_rpc, maps:get(protocols, Info, #{}), #{}),
            case retinue_protocol:compatibility(retinue_protocol:contract(), Offered) of
                ok ->
                    try gen_server:call({retinue_daemon, Node}, Request, 5000) of
                        {error, _} = Error -> Error;
                        Reply -> {ok, Reply}
                    catch exit:Reason -> {error, {daemon_call_failed, Reason}} end;
                {error, Mismatches} -> {error, {daemon_contract_mismatch, Mismatches}}
            end;
        {stopped, _} -> {error, daemon_stopped};
        Error -> Error
    end.

%% Unlike normal requests, diagnostics must not repair the state being inspected.
inspect(#{game := Game, source := companion_diagnostics, action := status} = Request) ->
    case retinue_client:status() of
        {running, Node, _Info} ->
            try gen_server:call({retinue_daemon, Node}, {submit, self(), Request}, 5000) of
                {ok, Ref} ->
                    try
                        receive
                            {retinue_daemon, Ref, Reply} -> Reply
                        after 6000 -> {error, companion_diagnostic_timeout}
                        end
                    after
                        try gen_server:call({retinue_daemon, Node},
                                            {unsubscribe, {Game, Ref}}, 1000)
                        catch exit:_ -> ok end
                    end;
                {error, _} = Error -> Error
            catch exit:Reason -> {error, {diagnostic_unavailable, Reason}} end;
        {stopped, _} -> {error, daemon_stopped};
        {error, _} = Error -> Error
    end.

-doc "Call daemon request API, auto-starting daemon if needed.".
-spec call(term()) -> {ok, term()} | {error, term()}.
-ifdef(TEST).
call(Request) ->
    case application:get_env(retinue_cli, test_local_daemon, false) of
        true ->
            case retinue_daemon:request(Request) of
                {error, _} = Error -> Error;
                Reply -> {ok, Reply}
            end;
        false -> remote_call(Request)
    end.
-else.
call(Request) -> remote_call(Request).
-endif.

remote_call(Request) ->
    case retinue_daemon_client:ensure_running() of
        {ok, _Status, Node} ->
            case daemon_call(Node, Request) of
                {ok, {error, Reason}} -> {error, Reason};
                {ok, Reply} -> {ok, Reply};
                {error, _Reason} = Error -> Error
            end;
        {error, Reason} -> {error, Reason}
    end.

-doc "Submit one queued request and wait for its single asynchronous response.".
-spec one_shot(map()) -> {ok, term()} | {error, term()}.
-ifdef(TEST).
one_shot(Request) ->
    case application:get_env(retinue_cli, test_local_daemon, false) of
        true -> local_one_shot(Request);
        false -> remote_one_shot(Request)
    end.
-else.
one_shot(Request) -> remote_one_shot(Request).
-endif.

remote_one_shot(Request) ->
    case subscribe_request(submit, Request) of
        {ok, Handle} ->
            Reply = next(Handle, ?REQUEST_TIMEOUT_MS),
            _ = unsubscribe(Handle),
            Reply;
        Error -> Error
    end.

-ifdef(TEST).
local_one_shot(Request) ->
    case retinue_daemon:submit(self(), Request) of
        {ok, Ref} ->
            receive
                {retinue_daemon, Ref, {ok, Reply}} -> {ok, Reply};
                {retinue_daemon, Ref, {error, Reason}} -> {error, Reason}
            after ?REQUEST_TIMEOUT_MS -> {error, timeout}
            end;
        {error, Reason} -> {error, Reason}
    end.
-endif.

-doc "Register persistent request stream; caller receives updates through `next/2`.".
-spec subscribe(map()) -> {ok, map()} | {error, term()}.
-ifdef(TEST).
subscribe(Request) ->
    case application:get_env(retinue_cli, test_local_daemon, false) of
        true -> local_subscribe(Request);
        false -> subscribe_request(subscribe, Request)
    end.

local_subscribe(Request) ->
    case retinue_daemon:subscribe(self(), Request) of
        {ok, Ref} -> {ok, #{direct_ref => Ref, game => maps:get(game, Request), node => node()}};
        {error, Reason} -> {error, Reason}
    end.
-else.
subscribe(Request) -> subscribe_request(subscribe, Request).
-endif.

-doc "Wait for next response from one daemon subscription.".
-spec next(map(), timeout()) -> {ok, term()} | {error, term()}.
next(#{direct_ref := Ref}, Timeout) ->
    receive
        {retinue_daemon, Ref, {ok, Reply}} -> consume(node(), Ref), {ok, Reply};
        {retinue_daemon, Ref, {error, Reason}} -> {error, Reason}
    after Timeout ->
        {error, timeout}
    end;
next(#{local_ref := Ref, proxy := Proxy, proxy_monitor := ProxyMonitor}, Timeout) ->
    receive
        {retinue_client, Ref, {ok, Reply}} -> Proxy ! {consumed, Ref}, {ok, Reply};
        {retinue_client, Ref, {error, Reason}} -> {error, Reason};
        {'DOWN', ProxyMonitor, process, _Pid, Reason} -> {error, {client_relay_down, Reason}}
    after Timeout ->
        {error, timeout}
    end.

-doc "Cancel daemon subscription and stop monitoring its node.".
-spec unsubscribe(map()) -> ok | {error, term()}.
unsubscribe(#{direct_ref := Ref, game := Game}) ->
    retinue_daemon:unsubscribe({Game, Ref});
unsubscribe(#{proxy := Proxy, proxy_monitor := ProxyMonitor}) ->
    Tag = make_ref(),
    Proxy ! {unsubscribe, self(), Tag},
    receive
        {Tag, Result} ->
            erlang:demonitor(ProxyMonitor, [flush]),
            Result;
        {'DOWN', ProxyMonitor, process, Proxy, _Reason} -> ok
    after 5000 ->
        erlang:demonitor(ProxyMonitor, [flush]),
        {error, unsubscribe_timeout}
    end.

subscribe_request(Kind, Request) ->
    Parent = self(),
    LocalRef = make_ref(),
    {Proxy, ProxyMonitor} = spawn_monitor(
      fun() -> subscription_proxy(Parent, LocalRef, Kind, Request) end),
    receive
        {retinue_client_ready, LocalRef, {ok, Node}} ->
            {ok, #{local_ref => LocalRef, proxy => Proxy,
                   proxy_monitor => ProxyMonitor, node => Node}};
        {retinue_client_ready, LocalRef, {error, Reason}} ->
            erlang:demonitor(ProxyMonitor, [flush]),
            {error, Reason};
        {'DOWN', ProxyMonitor, process, Proxy, Reason} ->
            {error, {client_relay_down, Reason}}
    after ?REQUEST_TIMEOUT_MS ->
        exit(Proxy, kill),
        erlang:demonitor(ProxyMonitor, [flush]),
        {error, timeout}
    end.

subscription_proxy(Client, LocalRef, Kind, Request) ->
    ClientMonitor = erlang:monitor(process, Client),
    case retinue_daemon_client:ensure_running() of
        {ok, _Status, Node} ->
            case remote_subscribe(Node, Kind, Request) of
                {ok, RemoteRef, OwnerMonitor} ->
                    Client ! {retinue_client_ready, LocalRef, {ok, Node}},
                    subscription_loop(Client, ClientMonitor, LocalRef, Node, Kind, Request,
                                      RemoteRef, OwnerMonitor);
                {error, Reason} ->
                    Client ! {retinue_client_ready, LocalRef, {error, Reason}}
            end;
        {error, Reason} -> Client ! {retinue_client_ready, LocalRef, {error, Reason}}
    end.

subscription_loop(Client, ClientMonitor, LocalRef, Node, Kind, Request,
                  RemoteRef, OwnerMonitor) ->
    receive
        {consumed, LocalRef} ->
            consume(Node, RemoteRef),
            subscription_loop(Client, ClientMonitor, LocalRef, Node, Kind, Request,
                              RemoteRef, OwnerMonitor);
        {retinue_daemon, RemoteRef, Reply} ->
            Client ! {retinue_client, LocalRef, Reply},
            subscription_loop(Client, ClientMonitor, LocalRef, Node, Kind, Request,
                              RemoteRef, OwnerMonitor);
        {'DOWN', OwnerMonitor, process, _Owner, Reason} ->
            case retry_subscription(Node, Kind, Request, ?RESUBSCRIBE_RETRIES) of
                {ok, NewRef, NewMonitor} ->
                    subscription_loop(Client, ClientMonitor, LocalRef, Node, Kind, Request,
                                      NewRef, NewMonitor);
                {error, RetryReason} ->
                    Client ! {retinue_client, LocalRef,
                              {error, {daemon_worker_stopped, Reason, RetryReason}}}
            end;
        {'DOWN', ClientMonitor, process, Client, _Reason} ->
            remote_unsubscribe(Node, maps:get(game, Request), RemoteRef);
        {unsubscribe, Client, Tag} ->
            Result = remote_unsubscribe(Node, maps:get(game, Request), RemoteRef),
            Client ! {Tag, Result}
    end.

retry_subscription(_Node, _Kind, #{source := companion_diagnostics}, _Attempts) ->
    {error, diagnostic_session_closed};
retry_subscription(Node, Kind, Request, Attempts) when Attempts > 0 ->
    case net_adm:ping(Node) of
        pang -> {error, daemon_stopped};
        pong ->
            timer:sleep(?RESUBSCRIBE_SLEEP_MS),
            case remote_subscribe(Node, Kind, Request) of
                {ok, _Ref, _Monitor} = Ok -> Ok;
                {error, _} -> retry_subscription(Node, Kind, Request, Attempts - 1)
            end
    end;
retry_subscription(_Node, _Kind, _Request, 0) -> {error, worker_restart_timeout}.

remote_subscribe(Node, Kind, Request) ->
    Function = case Kind of submit -> submit; subscribe -> subscribe end,
    case daemon_call(Node, {Function, self(), Request}) of
        {ok, {ok, Ref}} when is_reference(Ref);
                            is_tuple(Ref), tuple_size(Ref) =:= 2,
                            element(1, Ref) =:= companion_diagnostics,
                            is_reference(element(2, Ref)) ->
            case retinue_games:owner(Request) of
                undefined -> {error, {unsupported_source, maps:get(source, Request, undefined)}};
                Owner -> {ok, Ref, erlang:monitor(process, {Owner, Node})}
            end;
        {ok, {error, Reason}} -> {error, Reason};
        {ok, Other} -> {error, {unexpected_subscribe_reply, Other}};
        {error, _Reason} = Error -> Error
    end.

remote_unsubscribe(Node, Game, RemoteRef) ->
    case daemon_call(Node, {unsubscribe, {Game, RemoteRef}}) of
        {ok, Result} -> Result;
        {error, {daemon_call_failed, noconnection}} -> ok;
        {error, _Reason} = Error -> Error
    end.

consume(Node, {companion_diagnostics, _} = Ref) ->
    gen_server:cast({retinue_daemon, Node}, {diagnostics_consume, self(), Ref});
consume(_Node, _Ref) -> ok.

daemon_call(Node, Request) ->
    try gen_server:call({retinue_daemon, Node}, Request, ?REQUEST_TIMEOUT_MS) of
        Reply -> {ok, Reply}
    catch
        exit:{nodedown, _Node} -> {error, {daemon_call_failed, noconnection}};
        exit:{{nodedown, _Node}, _Call} -> {error, {daemon_call_failed, noconnection}};
        exit:{noproc, _Call} -> {error, {daemon_call_failed, noproc}};
        exit:Reason -> {error, {daemon_call_failed, Reason}}
    end.
