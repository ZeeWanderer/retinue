-module(wf_companion_diagnostics).

-export([submit/4, consume/3, reply/4, cancel/2, down/3, timeout/3, close/1]).

-define(MAX_REQUESTS, 16).
-define(REPLY_TIMEOUT, 5000).

submit(Client, Request, Connections, Pending) when map_size(Pending) < ?MAX_REQUESTS ->
    case {command(Request), target(Request, Connections)} of
        {{error, Reason}, _} -> {{error, Reason}, Pending};
        {_, {error, Reason}} -> {{error, Reason}, Pending};
        {{ok, Command, Duration}, {ok, Companion}} ->
            Id = binary:encode_hex(crypto:strong_rand_bytes(16), lowercase),
            Ref = {companion_diagnostics, make_ref()},
            Monitor = erlang:monitor(process, Client),
            Timer = erlang:start_timer(?REPLY_TIMEOUT, self(), {diagnostic_timeout, Id}),
            Entry = #{client => Client, monitor => Monitor, ref => Ref,
                      companion => Companion, timer => Timer, duration => Duration,
                      watch => Duration > 0, waiting => false, started => false},
            send(Companion, Id, Command),
            {{ok, Ref}, Pending#{Id => Entry}}
    end;
submit(_, _, _, Pending) -> {{error, diagnostics_busy}, Pending}.

command(#{action := status}) -> {ok, #{<<"action">> => <<"status">>}, 0};
command(#{action := watch, seconds := Seconds, topic := inventory})
  when is_integer(Seconds), Seconds >= 1, Seconds =< 1800 ->
    {ok, #{<<"action">> => <<"watch">>, <<"topic">> => <<"inventory">>,
           <<"seconds">> => Seconds}, Seconds * 1000};
command(#{action := stop, job := Job}) when is_binary(Job), byte_size(Job) =:= 32 ->
    {ok, #{<<"action">> => <<"stop">>, <<"job">> => Job}, 0};
command(_) -> {error, invalid_diagnostic_request}.

target(Request, Connections) ->
    Pid = maps:get(pid, Request, undefined),
    Companions = [Connection || {Connection, Info} <- maps:to_list(Connections),
                    maps:get(client, Info, undefined) =:= <<"wfcompanion">>,
                    maps:get(mode, Info, undefined) =/= <<"preview">>,
                    Pid =:= undefined orelse maps:get(os_pid, Info, undefined) =:= Pid],
    case Companions of
        [] -> {error, no_companion_connected};
        [Connection] ->
            case lists:member(<<"companion.diagnostics">>,
                               maps:get(features, maps:get(Connection, Connections), [])) of
                true -> {ok, Connection};
                false -> {error, companion_diagnostics_unavailable}
            end;
        _ -> {error, ambiguous_companion}
    end.

consume(Client, Ref, Pending) ->
    maps:map(fun(Id, #{client := Owner, ref := EntryRef, waiting := Waiting} = Entry)
                   when Owner =:= Client, EntryRef =:= Ref, Waiting ->
                     send(maps:get(companion, Entry), Id, #{<<"action">> => <<"credit">>}),
                     Entry#{waiting => false};
                (_, Entry) -> Entry
             end, Pending).

reply(Companion, Id, Data, Pending) ->
    case maps:get(Id, Pending, undefined) of
        #{companion := Companion} = Entry -> handle_reply(Id, Data, Entry, Pending);
        _ -> Pending
    end.

handle_reply(Id, #{<<"state">> := <<"running">>} = Data,
             #{watch := true, waiting := false} = Entry, Pending) ->
    deliver(Entry, {ok, Data}),
    Entry1 = case maps:get(started, Entry) of
        false ->
            erlang:cancel_timer(maps:get(timer, Entry)),
            Timer = erlang:start_timer(maps:get(duration, Entry) + ?REPLY_TIMEOUT,
                                      self(), {diagnostic_timeout, Id}),
            Entry#{timer => Timer, started => true};
        true -> Entry
    end,
    Pending#{Id => Entry1#{waiting => true}};
handle_reply(Id, #{<<"state">> := State} = Data, Entry, Pending)
  when State =:= <<"completed">>; State =:= <<"cancelled">>; State =:= <<"failed">> ->
    deliver(Entry, {ok, Data}),
    remove(Id, Pending);
handle_reply(_Id, _Data, _Entry, Pending) -> Pending.

cancel(Ref, Pending) ->
    maps:fold(fun(Id, #{ref := EntryRef}, Acc) when Ref =:= EntryRef -> cancel_id(Id, Acc);
                 (_, _, Acc) -> Acc
              end, Pending, Pending).

down(Monitor, Pid, Pending) ->
    maps:fold(fun(Id, #{companion := Companion} = Entry, Acc) when Companion =:= Pid ->
                     deliver(Entry, {error, companion_disconnected}),
                     remove(Id, Acc);
                 (Id, #{monitor := OwnerMonitor}, Acc) when Monitor =:= OwnerMonitor ->
                     cancel_id(Id, Acc);
                 (_, _, Acc) -> Acc
              end, Pending, Pending).

timeout(Id, Timer, Pending) ->
    case maps:get(Id, Pending, undefined) of
        #{timer := Timer} = Entry ->
            deliver(Entry, {error, companion_diagnostic_timeout}),
            cancel_id(Id, Pending);
        _ -> Pending
    end.

close(Pending) ->
    maps:foreach(fun(Id, Entry) ->
                     deliver(Entry, {error, diagnostic_session_closed}),
                     cancel_id(Id, Pending)
                 end, Pending),
    ok.

cancel_id(Id, Pending) ->
    Entry = maps:get(Id, Pending),
    send(maps:get(companion, Entry), Id, #{<<"action">> => <<"cancel">>}),
    remove(Id, Pending).

remove(Id, Pending) ->
    {Entry, Rest} = maps:take(Id, Pending),
    erlang:cancel_timer(maps:get(timer, Entry)),
    erlang:demonitor(maps:get(monitor, Entry), [flush]),
    Rest.

send(Companion, Id, Command) -> Companion ! {companion_diagnostics, Id, Command}.
deliver(#{client := Client, ref := Ref}, Reply) -> Client ! {retinue_daemon, Ref, Reply}.
