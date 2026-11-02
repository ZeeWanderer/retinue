%%%-------------------------------------------------------------------
%% EUnit coverage for managed source refresh policy.
%%%-------------------------------------------------------------------
-module(wf_source_manager_eunit).

-include_lib("eunit/include/eunit.hrl").

default_selection_includes_wfcd_test() ->
    ?assertEqual(
       [nodes, languages, exports, wfcd, star_chart],
       wf_source_manager:expand_selections([default])),
    ?assertEqual(
       [nodes, languages, exports, wfcd, star_chart],
       wf_source_manager:expand_selections([all])).

archimedea_requires_ordered_equipment_exports_test() ->
    Requirements = wf_source_manager:requirements("archimedea", #{}),
    ?assertEqual(["ExportWarframes_en.json", "ExportWeapons_en.json"],
                 [maps:get(id, Requirement) || Requirement <- Requirements]).

file_age_controls_staleness_test() ->
    Path = temp_path("metadata.json"),
    ok = file:write_file(Path, <<"{}">>),
    try
        {ok, Info} = file:read_file_info(Path, [{time, posix}]),
        Modified = element(6, Info),
        ?assertNot(wf_source_manager:stale_file(Path, 60, Modified + 59)),
        ?assert(wf_source_manager:stale_file(Path, 60, Modified + 60))
    after
        file:delete(Path)
    end.

wfcd_uses_embedded_fetch_time_test() ->
    Path = temp_path("WFCDEnemy.json"),
    Body = jsone:encode(
             #{<<"fetchedAt">> => 1000, <<"entries">> => [],
               <<"version">> => <<"fixture">>}),
    ok = file:write_file(Path, Body),
    try
        ?assertNot(wf_source_manager:stale_wfcd(Path, 60, 1059)),
        ?assert(wf_source_manager:stale_wfcd(Path, 60, 1060))
    after
        file:delete(Path)
    end.

periodic_refresh_queues_stale_sources_test() ->
    {ok, Activity} = retinue_activity:start_link(),
    Parent = self(),
    application:set_env(retinue_daemon, source_refresh_interval_ms, 10),
    application:set_env(retinue_daemon, source_max_age_seconds, 0),
    application:set_env(retinue_daemon, source_update_fun,
                        fun(Action) -> Parent ! {source_update, Action}, ok end),
    {ok, Pid} = wf_source_manager:start_link(),
    try
        ?assertEqual(
           [exports, languages, nodes, star_chart, wfcd],
           lists:sort(collect_updates(5, [])))
    after
        gen_server:stop(Pid),
        gen_server:stop(Activity),
        application:unset_env(retinue_daemon, source_refresh_interval_ms),
        application:unset_env(retinue_daemon, source_max_age_seconds),
        application:unset_env(retinue_daemon, source_update_fun)
    end.

collect_updates(0, Acc) -> Acc;
collect_updates(Remaining, Acc) ->
    receive
        {source_update, Action} ->
            collect_updates(Remaining - 1, [Action | Acc])
    after 1000 ->
        ?assert(false)
    end.

temp_path(Name) ->
    filename:join(
      "/tmp",
      "wfcli-source-" ++ integer_to_list(erlang:unique_integer([positive])) ++
      "-" ++ Name).
