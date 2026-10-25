-module(retinue_poe2_catalog_eunit).
-include_lib("eunit/include/eunit.hrl").

source_ranking_test_() ->
    {ok, Bytes} = file:read_file("apps/retinue_cli/test/fixtures/poe2_trade_ranking.json"),
    [{binary_to_list(maps:get(<<"name">>, Case)), fun() ->
        Rows = [maps:merge((row())#{<<"id">> => <<"explicit.", (maps:get(<<"key">>, R))/binary>>}, R)
                || R <- maps:get(<<"rows">>, Case)],
        Item = (item())#{<<"category">> => maps:get(<<"category">>, Case),
                         <<"rarity">> => maps:get(<<"rarity">>, Case, <<"Rare">>),
                         <<"modifiers">> => Rows},
        Prepared = retinue_poe2_trade_profiles:prepare(Item),
        Actual = maps:get(<<"modifiers">>, Prepared),
        Scores = maps:from_list([{K, S} || #{<<"key">> := K, <<"ranking_score">> := S} <- Actual]),
        Expected = maps:get(<<"scores">>, Case),
        ?assertEqual(lists:sort(maps:keys(Expected)), lists:sort(maps:keys(Scores))),
        maps:foreach(fun(Key, Value) -> ?assert(abs(Value - maps:get(Key, Scores)) < 1.0e-9) end, Expected),
        ?assertEqual(maps:get(<<"selected">>, Case),
            [K || #{<<"key">> := K, <<"quick_selected">> := true} <- Actual])
    end} || Case <- json:decode(Bytes)].

category_defaults_test() ->
    Prepare = fun(Fields) -> retinue_poe2_trade_profiles:prepare(maps:merge(item(), Fields)) end,
    ?assertMatch(#{<<"category">> := <<"accessory.ring">>, <<"profiles">> := [<<"nomods">>, <<"fuzzy">>],
                   <<"default_profile">> := <<"nomods">>, <<"base_required">> := false}, Prepare(#{})),
    ?assertMatch(#{<<"profiles">> := [<<"bases">>, <<"nomods">>, <<"fuzzy">>],
                   <<"default_profile">> := <<"bases">>}, Prepare(#{<<"rarity">> => <<"Normal">>})),
    Fractured = (row())#{<<"section">> => <<"fractured">>},
    ?assertMatch(#{<<"default_profile">> := <<"bases">>, <<"modifiers">> := [#{<<"base_selected">> := true}]},
                 Prepare(#{<<"modifiers">> => [Fractured]})),
    ?assertMatch(#{<<"default_profile">> := <<"bases">>},
                 Prepare(#{<<"properties">> => [#{<<"text">> => <<"Fractured Item">>}]})),
    ?assertMatch(#{<<"default_profile">> := <<"bases">>},
                 Prepare(#{<<"properties">> => [#{<<"text">> => <<"Quality: +21%">>}]})),
    ?assertMatch(#{<<"default_profile">> := <<"nomods">>},
                 Prepare(#{<<"properties">> => [#{<<"text">> => <<"Quality: +20%">>}]})),
    ?assertMatch(#{<<"default_profile">> := <<"nomods">>}, Prepare(#{
        <<"properties">> => [#{<<"text">> => <<"Quality: +30%">>}],
        <<"modifiers">> => [(row())#{<<"section">> => <<"implicit">>,
            <<"internal_ids">> => [<<"local_maximum_quality_is_%">>], <<"min">> => 30}]})),
    lists:foreach(fun(Text) ->
        ?assertMatch(#{<<"profiles">> := [<<"allmods">>, <<"fuzzy">>], <<"default_profile">> := <<"allmods">>},
            Prepare(#{<<"conditions">> => [#{<<"text">> => Text}], <<"modifiers">> => [Fractured]}))
    end, [<<"Corrupted">>, <<"Twice Corrupted">>, <<"Mirrored">>, <<"Sanctified">>, <<"Unmodifiable">>]),
    ?assertMatch(#{<<"default_profile">> := <<"allmods">>, <<"base_required">> := true},
                 Prepare(#{<<"rarity">> => <<"Unique">>})),
    ?assertMatch(#{<<"default_profile">> := <<"allmods">>, <<"base_required">> := true},
                 Prepare(#{<<"item_class">> => <<"Mana Flasks">>})),
    ?assertMatch(#{<<"default_profile">> := <<"allmods">>}, Prepare(#{<<"item_class">> => <<"Relics">>})),
    ?assertMatch(#{<<"default_profile">> := <<"nomods">>, <<"base_required">> := true},
                 Prepare(#{<<"item_class">> => <<"Waystones">>,
                           <<"conditions">> => [#{<<"text">> => <<"Corrupted">>}]})).

ambiguous_group_is_not_partly_ranked_test() ->
    A = (row())#{<<"decorator">> => #{<<"group">> => 1, <<"tags">> => [<<"Life">>]}},
    B = maps:remove(<<"internal_ids">>, A#{<<"key">> => <<"unknown">>, <<"supported">> => false}),
    #{<<"modifiers">> := Rows} = retinue_poe2_trade_profiles:prepare((item())#{<<"modifiers">> => [A, B]}),
    lists:foreach(fun(R) ->
        ?assertNot(maps:is_key(<<"ranking_score">>, R)),
        ?assertNot(maps:get(<<"quick_selected">>, R))
    end, Rows).

missing_internal_id_preserves_clipboard_ranking_test() ->
    Row = maps:remove(<<"internal_ids">>, (row())#{<<"decorator">> => #{<<"tags">> => [<<"Life">>]}}),
    #{<<"modifiers">> := [Prepared]} = retinue_poe2_trade_profiles:prepare((item())#{<<"modifiers">> => [Row]}),
    ?assertMatch(#{<<"ranking_score">> := 4.0, <<"quick_selected">> := true}, Prepared).

catalog_join_and_tier_test() ->
    {ok, Index} = retinue_poe2_catalog:index(catalog()),
    #{<<"modifiers">> := [Row]} = retinue_poe2_catalog:annotate(item(), Index),
    ?assertMatch(#{<<"internal_ids">> := [<<"base_maximum_life">>],
        <<"tier_info">> := #{<<"current">> := 2, <<"count">> := 3, <<"max">> := 2},
        <<"quick_selected">> := true}, Row),
    Unknown = (item())#{<<"base">> => <<"Unknown Ring">>},
    #{<<"modifiers">> := [MissingBase]} = retinue_poe2_catalog:annotate(Unknown, Index),
    ?assertEqual(#{<<"current">> => 2}, maps:get(<<"tier_info">>, MissingBase)),
    Tier1 = (raw_row())#{<<"decorator">> => #{<<"name">> => <<"Absent">>, <<"tier">> => 1}},
    #{<<"modifiers">> := [Fallback]} = retinue_poe2_catalog:annotate((item())#{<<"modifiers">> => [Tier1]}, Index),
    ?assertEqual(#{<<"current">> => 1, <<"count">> => 1, <<"max">> => 1}, maps:get(<<"tier_info">>, Fallback)).

catalog_ambiguity_test() ->
    Catalog = catalog(),
    [Stat] = maps:get(<<"stats">>, Catalog),
    {ok, Index} = retinue_poe2_catalog:index(Catalog#{<<"stats">> => [Stat, Stat#{<<"ids">> => [<<"other_stat">>]}]}),
    #{<<"modifiers">> := [Row]} = retinue_poe2_catalog:annotate(item(), Index),
    ?assertNot(maps:is_key(<<"internal_ids">>, Row)),
    ?assertMatch(#{<<"quick_selected">> := true, <<"tier_info">> := #{<<"count">> := 3}}, Row),
    ?assertEqual({error, invalid_ranking_catalog}, retinue_poe2_catalog:index(#{})),
    ?assertEqual({error, invalid_ranking_catalog}, retinue_poe2_catalog:index(Catalog#{<<"mods">> => []})).

clipboard_category_is_not_replaced_by_generic_catalog_class_test() ->
    Catalog = (catalog())#{<<"bases">> => #{<<"charm">> => #{<<"name">> => <<"Stone Charm">>,
        <<"item_class">> => <<"UtilityFlask">>, <<"tags">> => [<<"utility_flask">>]}}},
    {ok, Index} = retinue_poe2_catalog:index(Catalog),
    Item = (item())#{<<"base">> => <<"Stone Charm">>, <<"item_class">> => <<"Charms">>},
    ?assertMatch(#{<<"category">> := <<"flask.charm">>}, retinue_poe2_catalog:annotate(Item, Index)),
    ?assertMatch(#{<<"category">> := <<"flask">>},
        retinue_poe2_catalog:annotate(Item#{<<"item_class">> => null}, Index)).

spawn_weight_order_test() ->
    Catalog = catalog(),
    Mods = maps:get(<<"mods">>, Catalog),
    High = maps:get(<<"high">>, Mods),
    Blocked = High#{<<"spawn_weights">> => [#{<<"tag">> => <<"ring">>, <<"weight">> => 0},
                                              #{<<"tag">> => <<"default">>, <<"weight">> => 1}]},
    {ok, Index} = retinue_poe2_catalog:index(Catalog#{<<"mods">> => Mods#{<<"high">> => Blocked}}),
    #{<<"modifiers">> := [Row]} = retinue_poe2_catalog:annotate(item(), Index),
    ?assertEqual(#{<<"current">> => 2, <<"count">> => 2, <<"max">> => 1}, maps:get(<<"tier_info">>, Row)).

cache_refresh_and_coalescing_test() ->
    Parent = self(),
    {ok, Data} = retinue_poe2_catalog:index(catalog()),
    Clock = atomics:new(1, []),
    Loader = fun(_) -> Parent ! {loading, self()}, receive {result, Result} -> Result end end,
    {ok, Server} = retinue_poe2_catalog:start_link(#{name => undefined, loader => Loader,
        clock => fun() -> atomics:get(Clock, 1) end}),
    try
        ?assertMatch(#{<<"ranking_status">> := <<"unavailable">>},
            gen_server:call(Server, {enrich_cached, item()})),
        receive {loading, _} -> error(cache_lookup_loaded) after 0 -> ok end,
        spawn(fun() -> Parent ! {first, retinue_poe2_catalog:enrich(Server, item())} end),
        Worker = receive {loading, Pid} -> Pid after 1000 -> error(no_loader) end,
        Server ! {catalog_cached, Worker, Data},
        receive {first, Item} -> ?assertEqual(<<"ready">>, maps:get(<<"ranking_status">>, Item))
        after 1000 -> error(no_cached_reply) end,
        ?assertEqual(<<"ready">>, maps:get(<<"ranking_status">>, retinue_poe2_catalog:enrich(Server, item()))),
        receive {loading, _} -> error(duplicate_loader) after 0 -> ok end,
        Worker ! {result, {ok, Data, cached}},
        await_idle(Server),
        atomics:put(Clock, 1, 4000),
        ?assertMatch(#{<<"ranking_status">> := <<"ready">>},
            gen_server:call(Server, {enrich_cached, item()})),
        receive {loading, _} -> error(cache_lookup_refreshed) after 0 -> ok end,
        ?assertEqual(<<"ready">>, maps:get(<<"ranking_status">>, retinue_poe2_catalog:enrich(Server, item()))),
        Refresh = receive {loading, Next} -> Next after 1000 -> error(no_refresh) end,
        Refresh ! {result, {error, offline}},
        await_idle(Server),
        ?assertEqual(<<"ready">>, maps:get(<<"ranking_status">>, retinue_poe2_catalog:enrich(Server, item()))),
        receive {loading, _} -> error(no_backoff) after 0 -> ok end
    after gen_server:stop(Server) end.

worker_timeout_and_shutdown_test() ->
    Parent = self(),
    Loader = fun(_) -> Parent ! {loading, self()}, receive never -> ok end end,
    {ok, Server} = retinue_poe2_catalog:start_link(#{name => undefined, loader => Loader, timeout => 30}),
    try
        ?assertMatch(#{<<"ranking_status">> := <<"unavailable">>}, retinue_poe2_catalog:enrich(Server, item())),
        Worker = receive {loading, Pid} -> Pid after 1000 -> error(no_loader) end,
        ?assertNot(is_process_alive(Worker))
    after gen_server:stop(Server) end,
    {ok, Second} = retinue_poe2_catalog:start_link(#{name => undefined, loader => Loader}),
    spawn(fun() -> retinue_poe2_catalog:enrich(Second, item()) end),
    Busy = receive {loading, Pid2} -> Pid2 after 1000 -> error(no_loader) end,
    Ref = monitor(process, Busy),
    gen_server:stop(Second),
    receive {'DOWN', Ref, process, Busy, _} -> ok after 1000 -> error(orphan_loader) end.

category_query_test() ->
    Item = retinue_poe2_trade_profiles:prepare(item()),
    {ok, Fixture} = file:read_file("apps/retinue_cli/test/fixtures/poe2_trade_filters.json"),
    {ok, Options} = retinue_poe2_trade_query:options(json:decode(Fixture)),
    Request = #{<<"league">> => <<"Standard">>, <<"base">> => false},
    {ok, #{<<"query">> := Query}} = retinue_poe2_trade_query:build(Request, Item,
        [#{<<"id">> => <<"Standard">>}], Options),
    ?assertNot(maps:is_key(<<"type">>, Query)),
    ?assertMatch(#{<<"type_filters">> := #{<<"filters">> := #{
        <<"category">> := #{<<"option">> := <<"accessory.ring">>}}}}, maps:get(<<"filters">>, Query)).

reload_safe_callbacks_test() ->
    lists:foreach(fun({Module, Keys}) ->
        {ok, Server} = Module:start_link(#{name => undefined}),
        try
            State = sys:get_state(Server),
            lists:foreach(fun(Key) ->
                ?assertEqual({type, external}, erlang:fun_info(maps:get(Key, State), type))
            end, Keys),
            ok = sys:suspend(Server),
            ok = sys:change_code(Server, Module, old, hot_update),
            ok = sys:resume(Server),
            ?assertEqual(State, sys:get_state(Server))
        after gen_server:stop(Server) end
    end, [{retinue_poe2_catalog, [loader, clock]}, {retinue_poe2_trade, [http, enrich, enrich_cached, clock]}]),
    Clock = fun() -> 42 end,
    State = #{clock => Clock, loader => fun retinue_poe2_catalog:load/1},
    ?assertMatch({ok, #{clock := Clock}}, retinue_poe2_catalog:code_change(old, State, hot_update)).

await_idle(Server) -> await_idle(Server, 100).
await_idle(_, 0) -> error(loader_did_not_finish);
await_idle(Server, Attempts) ->
    case maps:get(worker, sys:get_state(Server)) of
        undefined -> ok;
        _ -> timer:sleep(10), await_idle(Server, Attempts - 1)
    end.

row() -> #{<<"key">> => <<"life">>, <<"section">> => <<"explicit">>, <<"supported">> => true,
    <<"id">> => <<"explicit.life">>, <<"min">> => 10, <<"max">> => 10,
    <<"internal_ids">> => [<<"base_maximum_life">>],
    <<"magnitudes">> => [#{<<"value">> => 10, <<"min">> => 10, <<"max">> => 10}]}.

raw_row() -> (maps:remove(<<"internal_ids">>, row()))#{<<"decorator">> => #{
    <<"name">> => <<"Healthy">>, <<"affix">> => <<"Prefix">>, <<"tier">> => 2, <<"tags">> => [<<"Life">>]}}.

item() -> #{<<"name">> => <<"Dread Loop">>, <<"base">> => <<"Test Ring">>,
    <<"item_class">> => <<"Rings">>, <<"rarity">> => <<"Rare">>, <<"item_level">> => 30,
    <<"modifiers">> => [raw_row()], <<"conditions">> => [], <<"properties">> => []}.

catalog() ->
    Mod = fun(Name, Level) -> #{<<"name">> => Name, <<"required_level">> => Level,
        <<"generation_type">> => <<"prefix">>, <<"type">> => <<"Life">>,
        <<"stats">> => [#{<<"id">> => <<"base_maximum_life">>}],
        <<"spawn_weights">> => [#{<<"tag">> => <<"ring">>, <<"weight">> => 1}]} end,
    #{<<"bases">> => #{<<"ring">> => #{<<"name">> => <<"Test Ring">>, <<"item_class">> => <<"Ring">>,
        <<"tags">> => [<<"ring">>, <<"default">>]}},
      <<"stats">> => [#{<<"ids">> => [<<"base_maximum_life">>],
                        <<"trade_stats">> => [#{<<"id">> => <<"explicit.life">>}]}],
      <<"mods">> => #{<<"low">> => Mod(<<"Low">>, 1), <<"middle">> => Mod(<<"Healthy">>, 20),
                       <<"high">> => Mod(<<"High">>, 60)}}.
