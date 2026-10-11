%%%-------------------------------------------------------------------
%% Shared daemon activity holds and idle shutdown policy.
%%%-------------------------------------------------------------------
-module(retinue_activity).

-behaviour(gen_server).

-export([start_link/0, activity_start/0, activity_end/0, set_idle_policy/1, status/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(DEFAULT_IDLE_TIMEOUT_MS, 600000).

-type state() :: map().
-type idle_policy() :: persistent | idle | {idle, pos_integer()}.

-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Acquire one activity hold for the calling process before returning.".
-spec activity_start() -> ok.
activity_start() ->
    gen_server:call(?SERVER, activity_start).

-doc "Release one calling-process hold; an unmatched release is harmless.".
-spec activity_end() -> ok.
activity_end() ->
    gen_server:call(?SERVER, activity_end).

-doc "Set shutdown policy without discarding activity; idle restores the configured timeout.".
-spec set_idle_policy(idle_policy()) -> ok | {error, term()}.
set_idle_policy(Policy) ->
    gen_server:call(?SERVER, {set_idle_policy, Policy}).

-doc "Return idle policy, timeout, and the total number of outstanding activity holds.".
-spec status() -> map().
status() ->
    gen_server:call(?SERVER, status).

-spec init([]) -> {ok, state()}.
init([]) ->
    IdleMs = daemon_env(daemon_idle_timeout_ms, ?DEFAULT_IDLE_TIMEOUT_MS),
    IdleEnabled = case os:getenv("RETINUE_DAEMON_IDLE_POLICY") of
        "persistent" -> false;
        _ -> daemon_env(daemon_idle_shutdown, false)
    end,
    State = #{owners => #{}, monitors => #{}, idle_timer => undefined,
              idle_timeout_ms => IdleMs, default_idle_timeout_ms => IdleMs,
              idle_shutdown => IdleEnabled,
              idle_notify => daemon_env(daemon_idle_notify_pid, undefined)},
    {ok, maybe_arm_idle(State)}.

-spec handle_call(term(), gen_server:from(), state()) -> {reply, term(), state()}.
handle_call(activity_start, {Owner, _Tag}, State) ->
    {reply, ok, cancel_idle(add_activity(Owner, State))};
handle_call(activity_end, {Owner, _Tag}, State) ->
    {reply, ok, maybe_arm_idle(remove_activity(Owner, State))};
handle_call({set_idle_policy, persistent}, _From, State) ->
    {reply, ok, cancel_idle(State#{idle_shutdown => false})};
handle_call({set_idle_policy, idle}, _From, State) ->
    DefaultMs = maps:get(default_idle_timeout_ms, State),
    State1 = cancel_idle(State#{idle_shutdown => true, idle_timeout_ms => DefaultMs}),
    {reply, ok, maybe_arm_idle(State1)};
handle_call({set_idle_policy, {idle, TimeoutMs}}, _From, State)
  when is_integer(TimeoutMs), TimeoutMs > 0 ->
    State1 = cancel_idle(State#{idle_shutdown => true, idle_timeout_ms => TimeoutMs}),
    {reply, ok, maybe_arm_idle(State1)};
handle_call({set_idle_policy, Policy}, _From, State) ->
    {reply, {error, {invalid_idle_policy, Policy}}, State};
handle_call(status, _From, State) ->
    Policy = case maps:get(idle_shutdown, State) of true -> idle; false -> persistent end,
    Count = lists:sum([N || {N, _Monitor} <- maps:values(maps:get(owners, State))]),
    {reply, #{idle_policy => Policy, idle_timeout_ms => maps:get(idle_timeout_ms, State),
              external_activity => Count}, State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

-spec handle_cast(term(), state()) -> {noreply, state()}.
handle_cast(_Message, State) ->
    {noreply, State}.

-spec handle_info(term(), state()) -> {noreply, state()}.
handle_info({'DOWN', Monitor, process, Owner, _Reason}, State) ->
    case maps:get(Monitor, maps:get(monitors, State), undefined) of
        Owner -> {noreply, maybe_arm_idle(drop_activity(Owner, Monitor, State))};
        _ -> {noreply, State}
    end;
handle_info({timeout, Timer, idle}, State = #{idle_timer := Timer})
  when is_reference(Timer) ->
    perform_idle_action(State),
    {noreply, State#{idle_timer => undefined}};
handle_info(_Message, State) ->
    {noreply, State}.

-spec terminate(term(), state()) -> ok.
terminate(_Reason, State) ->
    cancel_idle(State),
    maps:foreach(fun(Monitor, _Owner) -> erlang:demonitor(Monitor, [flush]) end,
                 maps:get(monitors, State)),
    ok.

-spec code_change(term(), state(), term()) -> {ok, state()}.
code_change(_OldVsn, State, _Extra) ->
    {ok, State}.

add_activity(Owner, State) ->
    Owners = maps:get(owners, State),
    case maps:get(Owner, Owners, undefined) of
        undefined ->
            Monitor = erlang:monitor(process, Owner),
            Monitors = maps:get(monitors, State),
            State#{owners => Owners#{Owner => {1, Monitor}},
                   monitors => Monitors#{Monitor => Owner}};
        {Count, Monitor} ->
            State#{owners => Owners#{Owner := {Count + 1, Monitor}}}
    end.

remove_activity(Owner, State) ->
    Owners = maps:get(owners, State),
    case maps:get(Owner, Owners, undefined) of
        undefined -> State;
        {1, Monitor} ->
            erlang:demonitor(Monitor, [flush]),
            drop_activity(Owner, Monitor, State);
        {Count, Monitor} ->
            State#{owners => Owners#{Owner := {Count - 1, Monitor}}}
    end.

drop_activity(Owner, Monitor, State) ->
    State#{owners => maps:remove(Owner, maps:get(owners, State)),
           monitors => maps:remove(Monitor, maps:get(monitors, State))}.

maybe_arm_idle(State = #{idle_shutdown := true, owners := Owners, idle_timer := undefined})
  when map_size(Owners) =:= 0 ->
    Timer = erlang:start_timer(maps:get(idle_timeout_ms, State), self(), idle),
    State#{idle_timer => Timer};
maybe_arm_idle(State) -> State.

cancel_idle(State = #{idle_timer := undefined}) -> State;
cancel_idle(State = #{idle_timer := Timer}) ->
    _ = erlang:cancel_timer(Timer),
    State#{idle_timer => undefined}.

perform_idle_action(#{idle_notify := Pid}) when is_pid(Pid) ->
    Pid ! {wfcli_daemon_idle, self()},
    ok;
perform_idle_action(_State) ->
    _ = spawn(fun() -> timer:sleep(20), init:stop() end),
    ok.

daemon_env(Key, Default) ->
    application:get_env(retinue_daemon, Key, Default).
