-module(retinue_sup_eunit).

-include_lib("eunit/include/eunit.hrl").

restart_policy_tolerates_short_bursts_test() ->
    {ok, {Flags, _Children}} = retinue_sup:init([]),
    ?assertEqual(rest_for_one, maps:get(strategy, Flags)),
    ?assertEqual(3, maps:get(intensity, Flags)),
    ?assertEqual(10, maps:get(period, Flags)).

platform_children_follow_activity_dependencies_test() ->
    ?assertMatch([#{id := retinue_activity, type := worker},
                  #{id := retinue_games_sup, type := supervisor, shutdown := infinity},
                  #{id := retinue_local_api, type := worker},
                  #{id := retinue_daemon, type := worker}], retinue_sup:child_specs()).

game_supervisors_restart_independently_test_() ->
    [fun() ->
         {ok, {Flags, _Children}} = Module:init([]),
         ?assertEqual(one_for_one, maps:get(strategy, Flags))
     end || Module <- [retinue_games_sup, wf_sup, retinue_poe2_sup]].

games_are_separate_subtrees_test() ->
    ?assertMatch([#{id := wf_sup, type := supervisor, shutdown := infinity},
                  #{id := retinue_poe2_sup, type := supervisor, shutdown := infinity}],
                 retinue_games_sup:child_specs()).

worker_owners_match_supervisors_test() ->
    Workers = [{maps:get(id, Spec), Supervisor}
               || Supervisor <- [retinue_sup, wf_sup, retinue_poe2_sup],
                  Spec = #{type := worker} <- Supervisor:child_specs()],
    ?assertEqual(lists:sort([Name || {Name, _} <- Workers]),
                 lists:sort([maps:get(id, Spec) || Spec <- retinue_sup:worker_specs()])),
    lists:foreach(fun({Name, Supervisor}) ->
        ?assertEqual(Supervisor, retinue_sup:owner(Name))
    end, Workers).
