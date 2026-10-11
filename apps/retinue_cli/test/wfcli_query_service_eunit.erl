%%%-------------------------------------------------------------------
%% EUnit coverage for unified query worker admission.
%%%-------------------------------------------------------------------
-module(wfcli_query_service_eunit).

-include_lib("eunit/include/eunit.hrl").

worker_limit_queues_excess_queries_test() ->
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    application:set_env(retinue_daemon, query_workers, 2),
    TestPid = self(),
    application:set_env(
      retinue_daemon, daemon_query_execute_fun,
      fun(Request) ->
          TestPid ! {query_started, maps:get(job, Request), self()},
          receive continue -> {ok, done} end
      end),
    {ok, _Activity} = retinue_activity:start_link(),
    {ok, _Worldstate} = wfcli_worldstate_service:start_link(),
    {ok, _Query} = wfcli_query_service:start_link(),
    try
        {ok, Ref1} = wfcli_query_service:submit(self(), #{job => 1}),
        {ok, Ref2} = wfcli_query_service:submit(self(), #{job => 2}),
        {ok, Ref3} = wfcli_query_service:submit(self(), #{job => 3}),
        {Job1, Worker1} = receive
            {query_started, FirstJob, FirstPid} -> {FirstJob, FirstPid}
        after 1000 -> error(first_query_not_started)
        end,
        {_Job2, Worker2} = receive
            {query_started, SecondJob, SecondPid} -> {SecondJob, SecondPid}
        after 1000 -> error(second_query_not_started)
        end,
        ?assertMatch(#{active := 2, queued := 1}, wfcli_query_service:status()),
        receive {query_started, _, _} -> error(query_limit_exceeded)
        after 100 -> ok
        end,
        Worker1 ! continue,
        _ = await(case Job1 of 1 -> Ref1; 2 -> Ref2 end),
        {3, Worker3} = receive
            {query_started, ThirdJob, ThirdPid} -> {ThirdJob, ThirdPid}
        after 1000 -> error(queued_query_not_started)
        end,
        Worker2 ! continue,
        Worker3 ! continue,
        _ = await(case Job1 of 1 -> Ref2; 2 -> Ref1 end),
        _ = await(Ref3)
    after
        gen_server:stop(wfcli_query_service),
        gen_server:stop(wfcli_worldstate_service),
        gen_server:stop(retinue_activity),
        application:unset_env(retinue_daemon, daemon_query_execute_fun),
        application:unset_env(retinue_daemon, query_workers),
        application:unset_env(retinue_daemon, daemon_idle_shutdown)
    end.

selected_datasets_execute_concurrently_test() ->
    Test = self(),
    application:set_env(
      retinue_daemon, query_dataset_execute_fun,
      fun(Dataset, _Query, _Ast, _Request) ->
          Test ! {dataset_started, Dataset, self()},
          receive continue -> #{dataset => Dataset, reply => {ok, Dataset}} end
      end),
    Caller = spawn_link(fun() ->
        Test ! {dataset_query_reply,
                wfcli_query_service:execute(
                  #{query_tokens => ["dataset=mods|items", "prime"]})}
    end),
    try
        {FirstDataset, First} = receive
            {dataset_started, Dataset1, Pid1} -> {Dataset1, Pid1}
        after 1000 -> error(first_dataset_not_started)
        end,
        {SecondDataset, Second} = receive
            {dataset_started, Dataset2, Pid2} -> {Dataset2, Pid2}
        after 1000 -> error(second_dataset_not_started)
        end,
        ?assertEqual([items, mods], lists:sort([FirstDataset, SecondDataset])),
        First ! continue,
        Second ! continue,
        receive
            {dataset_query_reply, {ok, #{datasets := Results}}} ->
                ?assertEqual([mods, items], [maps:get(dataset, Result) || Result <- Results])
        after 1000 ->
            error(dataset_query_reply_timeout)
        end
    after
        application:unset_env(retinue_daemon, query_dataset_execute_fun),
        unlink(Caller),
        exit(Caller, kill)
    end.

await(Ref) ->
    receive {retinue_daemon, Ref, Reply} -> Reply
    after 1000 -> error({query_reply_timeout, Ref})
    end.
