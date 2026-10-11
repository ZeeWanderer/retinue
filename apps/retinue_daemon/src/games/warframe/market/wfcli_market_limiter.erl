%%%-------------------------------------------------------------------
%% Shared Warframe.market request scheduler.
%%%-------------------------------------------------------------------
-module(wfcli_market_limiter).

-behaviour(gen_server).

-export([start_link/0, wait/0, status/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(DEFAULT_INTERVAL_MS, 334).

-doc "Start the scheduler shared by public and authenticated Market clients.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() -> gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Reserve the next request slot and wait until it starts.".
-spec wait() -> ok.
wait() ->
    case whereis(?SERVER) of
        undefined -> ok;
        _Pid ->
            Delay = gen_server:call(?SERVER, reserve),
            timer:sleep(Delay)
    end.

-doc "Return scheduler timing for diagnostics.".
-spec status() -> map().
status() -> gen_server:call(?SERVER, status).

init([]) -> {ok, #{next_at => 0}}.

handle_call(reserve, _From, State) ->
    Now = erlang:monotonic_time(millisecond),
    Start = case maps:get(next_at, State, 0) of
        0 -> Now;
        NextAt -> max(Now, NextAt)
    end,
    {reply, Start - Now, State#{next_at => Start + request_interval()}};
handle_call(status, _From, State) ->
    Now = erlang:monotonic_time(millisecond),
    Wait = case maps:get(next_at, State, 0) of
        0 -> 0;
        NextAt -> max(0, NextAt - Now)
    end,
    {reply, #{interval_ms => request_interval(),
              wait_ms => Wait}, State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

handle_cast(_Message, State) -> {noreply, State}.
handle_info(_Message, State) -> {noreply, State}.

code_change(_OldVsn, State, _Extra) ->
    {ok, State#{next_at => maps:get(next_at, State, 0)}}.

request_interval() ->
    case application:get_env(retinue_daemon, market_request_interval_ms) of
        {ok, Value} when is_integer(Value), Value >= 0 -> Value;
        _ -> ?DEFAULT_INTERVAL_MS
    end.
