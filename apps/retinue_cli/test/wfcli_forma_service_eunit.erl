%%%-------------------------------------------------------------------
%% Queue ownership tests for daemon-side Forma work.
%%%-------------------------------------------------------------------
-module(wfcli_forma_service_eunit).

-include_lib("eunit/include/eunit.hrl").

client_exit_cancels_running_plan_test_() ->
    {setup,
     fun wfcli_test_daemon:start/0,
     fun(_Setup) -> wfcli_test_daemon:stop() end,
     fun() ->
         Parent = self(),
         Client = spawn(fun client_wait/0),
         Work = fun() ->
             Parent ! {planner_worker, self()},
             receive continue -> {ok, unexpected} end
         end,
         {ok, _Ref} = wfcli_forma_service:submit(Client, #{test_fun => Work}),
         Worker = receive
             {planner_worker, Pid} -> Pid
         after 1000 -> error(planner_worker_not_started)
         end,
         Monitor = erlang:monitor(process, Worker),
         exit(Client, kill),
         receive
             {'DOWN', Monitor, process, Worker, killed} -> ok
         after 1000 -> error(planner_worker_not_cancelled)
         end,
         wait_idle(20)
     end}.

client_wait() ->
    receive stop -> ok end.

wait_idle(0) ->
    ?assertEqual(#{processing => false, queued => 0}, wfcli_forma_service:status());
wait_idle(Attempts) ->
    case wfcli_forma_service:status() of
        #{processing := false, queued := 0} -> ok;
        _ ->
            timer:sleep(10),
            wait_idle(Attempts - 1)
    end.
