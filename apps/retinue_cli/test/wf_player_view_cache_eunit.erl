-module(wf_player_view_cache_eunit).
-include_lib("eunit/include/eunit.hrl").

retirement_test() ->
    Table = ets:new(wf_player_view_cache, [named_table, public, set]),
    try
        ?assertEqual(one, wf_player_view_cache:get({foundry, 1}, fun() -> one end)),
        ?assertEqual(one, wf_player_view_cache:get({foundry, 1}, fun() -> error(rebuilt) end)),
        [wf_player_view_cache:get({foundry, N}, fun() -> N end) || N <- lists:seq(2, 20)],
        ?assertEqual(1, ets:info(Table, size)),
        Parent = self(),
        Worker = spawn(fun() ->
            Result = wf_player_view_cache:get({foundry, old}, fun() ->
                Parent ! started,
                receive finish -> old end
            end),
            Parent ! {finished, Result}
        end),
        receive started -> ok after 1000 -> error(no_worker) end,
        ?assertEqual(current, wf_player_view_cache:get({foundry, current}, fun() -> current end)),
        Worker ! finish,
        receive {finished, old} -> ok after 1000 -> error(no_result) end,
        ?assertEqual(current, wf_player_view_cache:get({foundry, current}, fun() -> error(stale_overwrite) end)),
        wf_player_view_cache:get({mastery, 1}, fun() -> other end),
        ?assertEqual(2, ets:info(Table, size)),
        Invalidated = spawn(fun() ->
            wf_player_view_cache:get({inventory, 1}, fun() ->
                Parent ! pending,
                receive finish -> retired end
            end),
            Parent ! completed
        end),
        receive pending -> ok after 1000 -> error(no_reservation) end,
        ets:delete_all_objects(Table),
        Invalidated ! finish,
        receive completed -> ok after 1000 -> error(no_completion) end,
        ?assertEqual(0, ets:info(Table, size))
    after ets:delete(Table) end,
    ?assertEqual(uncached, wf_player_view_cache:get({foundry, 1}, fun() -> uncached end)).
