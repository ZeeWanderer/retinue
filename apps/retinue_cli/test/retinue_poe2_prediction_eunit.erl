-module(retinue_poe2_prediction_eunit).
-include_lib("eunit/include/eunit.hrl").

payload_is_allowlisted_and_preserves_item_stats_test() ->
    Payload = payload(),
    ?assertMatch(#{<<"category">> := <<"accessory.ring">>, <<"frameType">> := 2,
        <<"explicitMods">> := [<<"+80 to maximum Life">>],
        <<"extended">> := #{<<"mods">> := #{<<"explicit">> := [#{<<"magnitudes">> :=
            [#{<<"hash">> := <<"explicit.stat_3299347043">>, <<"value">> := 80}]}]}}}, Payload),
    Enriched = (item())#{<<"raw">> => <<"secret clipboard">>, <<"account">> => <<"private">>,
        <<"inventoryId">> => <<"bag">>, <<"properties">> => [#{<<"text">> => <<"Quality: +10% (augmented)">>},
            #{<<"text">> => <<"Physical Damage: 10-20">>}, #{<<"text">> => <<"Requires Level 10">>}]},
    Safe = retinue_poe2_prediction_item:build(Enriched),
    lists:foreach(fun(Key) -> ?assertNot(maps:is_key(Key, Safe)) end,
        [<<"raw">>, <<"account">>, <<"inventoryId">>]),
    ?assertMatch([#{<<"type">> := 6, <<"values">> := [[<<"+10%">>, 1]]},
                  #{<<"type">> := 9, <<"values">> := [[<<"10-20">>, 0]]}],
        maps:get(<<"properties">>, Safe)),
    Partial = retinue_poe2_prediction_item:build((item())#{<<"modifiers">> =>
        [#{<<"key">> => <<"unknown">>, <<"section">> => <<"explicit">>,
           <<"supported">> => false, <<"text">> => <<"unknown affix">>}]}),
    ?assertMatch(#{<<"explicitMods">> := [<<"unknown affix">>],
        <<"extended">> := #{<<"mods">> := #{<<"explicit">> := []},
                            <<"hashes">> := #{<<"explicit">> := []}}}, Partial),
    ?assertEqual(undefined, retinue_poe2_prediction_item:build((item())#{<<"rarity">> => <<"Quest">>})).

zero_error_code_is_success_test() ->
    with_server(fun(_) -> {ok, 200, #{}, <<"{\"results\":[{\"error_code\":0,\"predicted_price\":40}]}">>} end,
        fun(Server, _, _) -> ?assertMatch({ok, #{<<"estimate">> := #{<<"amount">> := 40}}},
            predict(Server, <<"A">>)) end).

payload_preserves_hybrid_affixes_and_reduced_values_test() ->
    {ok, Bytes} = file:read_file("apps/retinue_cli/test/fixtures/poe2_modifier_stats.json"),
    {ok, Catalog} = retinue_poe2_trade_item:catalog(json:decode(Bytes)),
    {ok, ArmourText} = file:read_file("apps/retinue_cli/test/fixtures/poe2_rare_body_armour.txt"),
    {ok, Armour} = retinue_poe2_trade_item:parse(ArmourText, Catalog),
    Payload = retinue_poe2_prediction_item:build(Armour),
    Id = <<"explicit.stat_124859000,explicit.stat_2106365538">>,
    ?assertMatch([<<"104% increased Evasion Rating">> | _], maps:get(<<"explicitMods">>, Payload)),
    #{<<"mods">> := Mods, <<"hashes">> := Hashes} = maps:get(<<"extended">>, Payload),
    ?assertMatch([[Id, [0, 2]] | _], maps:get(<<"explicit">>, Hashes)),
    ?assertMatch([#{<<"magnitudes">> := [#{<<"value">> := 86}]}, _,
        #{<<"magnitudes">> := [#{<<"value">> := 15}, #{<<"value">> := 18}]} | _],
        maps:get(<<"explicit">>, Mods)),
    {ok, SpearText} = file:read_file("apps/retinue_cli/test/fixtures/poe2_unique_spear.txt"),
    {ok, Spear} = retinue_poe2_trade_item:parse(SpearText, Catalog),
    ?assertMatch(#{<<"extended">> := #{<<"mods">> := #{<<"explicit">> := [
        #{<<"magnitudes">> := [#{<<"value">> := 10, <<"mod">> := <<"negate">>}]} | _]}}},
        retinue_poe2_prediction_item:build(Spear)),
    Bad = Spear#{<<"modifiers">> => [#{<<"section">> => <<"explicit">>,
        <<"display_values">> => [1000000001]}]},
    ?assertEqual(undefined, retinue_poe2_prediction_item:build(Bad)).

cache_expiry_and_league_identity_test() ->
    with_server(fun(_) -> success() end, fun(Server, Calls, Clock) ->
        First = predict(Server, <<"League A">>),
        ?assertMatch({ok, #{<<"estimate">> := #{<<"amount">> := 40,
            <<"currency">> := <<"exalted">>, <<"uncertainty">> := #{<<"confidence">> := 75}}}}, First),
        ?assertEqual(First, predict(Server, <<"League A">>)),
        ?assertEqual(1, atomics:get(Calls, 1)),
        predict(Server, <<"League B">>),
        ?assertEqual(2, atomics:get(Calls, 1)),
        atomics:put(Clock, 1, 1601),
        predict(Server, <<"League A">>),
        ?assertEqual(3, atomics:get(Calls, 1))
    end).

coalescing_and_bounded_concurrency_test() ->
    Parent = self(),
    with_server(fun(_) -> Parent ! {started, self()}, receive release -> success() end end,
      fun(Server, Calls, _) ->
        spawn(fun() -> Parent ! {result, predict(Server, <<"A">>)} end),
        Worker = receive {started, W} -> W after 1000 -> error(no_worker) end,
        spawn(fun() -> Parent ! {result, predict(Server, <<"A">>)} end),
        ?assertMatch({error, #{<<"reason">> := <<"prediction_busy">>}}, predict(Server, <<"B">>)),
        Worker ! release,
        lists:foreach(fun(_) -> receive {result, Reply} -> ?assertMatch({ok, _}, Reply)
            after 1000 -> error(no_reply) end end, [1, 2]),
        ?assertEqual(1, atomics:get(Calls, 1))
    end).

model_and_rate_backoff_test() ->
    with_server(fun(_) -> {ok, 404, #{}, <<>>} end, fun(Server, Calls, Clock) ->
        ?assertMatch({ok, #{<<"estimate">> := null, <<"reason">> := <<"model_unavailable">>}},
            predict(Server, <<"A">>)),
        retinue_poe2_prediction:request(Server, <<"A">>, (payload())#{<<"name">> => <<"Other">>}),
        ?assertEqual(1, atomics:get(Calls, 1)),
        predict(Server, <<"B">>),
        ?assertEqual(2, atomics:get(Calls, 1)),
        atomics:put(Clock, 1, 1901), predict(Server, <<"A">>),
        ?assertEqual(3, atomics:get(Calls, 1))
    end),
    with_server(fun(_) -> {ok, 429, #{<<"retry-after">> => <<"120">>}, <<>>} end,
      fun(Server, Calls, Clock) ->
        predict(Server, <<"A">>),
        ?assertMatch({ok, #{<<"reason">> := <<"prediction_cooldown">>}}, predict(Server, <<"B">>)),
        ?assertEqual(1, atomics:get(Calls, 1)),
        atomics:put(Clock, 1, 1121), predict(Server, <<"B">>),
        ?assertEqual(2, atomics:get(Calls, 1))
    end).

invalid_responses_do_not_become_estimates_test() ->
    lists:foreach(fun(Body) ->
        with_server(fun(_) -> {ok, 200, #{}, Body} end, fun(Server, _, _) ->
            ?assertMatch({ok, #{<<"estimate">> := null,
                <<"reason">> := <<"invalid_prediction_response">>}}, predict(Server, <<"A">>))
        end)
    end, [<<"not json">>, <<"{\"results\":[]}">>,
        <<"{\"results\":[{\"predicted_price\":-5}]}">>,
        <<"{\"results\":[{\"predicted_price\":40,\"predicted_price_low\":50,\"predicted_price_high\":30}]}">>]),
    with_server(fun(_) -> {ok, 200, #{}, <<"{\"results\":[{\"error_code\":\"unsupported\"}]}">>} end,
      fun(Server, _, _) ->
        ?assertMatch({ok, #{<<"estimate">> := null, <<"reason">> := <<"item_not_supported">>}},
            predict(Server, <<"A">>))
    end).

trade_hands_off_without_blocking_search_test() ->
    Parent = self(),
    {ok, Prediction} = retinue_poe2_prediction:start_link(#{http => fun(_) ->
        Parent ! {started, self()}, receive release -> success() end end}),
    {ok, Trade} = retinue_poe2_trade:start_link(#{name => undefined,
        enrich => fun retinue_poe2_trade_profiles:prepare/1,
        http => fun retinue_poe2_trade_eunit:http/4}),
    try
        Text = text(),
        {ok, Evaluation} = retinue_poe2_trade:request(Trade, #{<<"op">> => <<"trade_item">>, <<"text">> => Text}),
        ?assertEqual(true, maps:get(<<"prediction_available">>, Evaluation)),
        ?assertNot(maps:is_key(prediction_item, maps:get(<<"item">>, Evaluation))),
        spawn(fun() -> Parent ! {prediction_result, retinue_poe2_trade:request(Trade,
            #{<<"op">> => <<"trade_predict">>, <<"text">> => Text, <<"league">> => <<"Standard">>})} end),
        Worker = receive {started, W} -> W after 1000 -> error(no_worker) end,
        ?assertMatch({ok, _}, retinue_poe2_trade:request(Trade, #{<<"op">> => <<"trade_leagues">>})),
        ?assertMatch({error, #{<<"reason">> := <<"invalid_league">>}},
            retinue_poe2_trade:request(Trade, #{<<"op">> => <<"trade_predict">>,
                <<"text">> => Text, <<"league">> => <<"injected">>})),
        Worker ! release,
        receive {prediction_result, Reply} -> ?assertMatch({ok, #{<<"estimate">> := #{}}}, Reply)
            after 1000 -> error(no_prediction) end
    after gen_server:stop(Trade), gen_server:stop(Prediction) end.

predict(Server, League) -> retinue_poe2_prediction:request(Server, League, payload()).
payload() -> retinue_poe2_prediction_item:build(item()).
text() -> <<"Item Class: Rings\nRarity: Rare\nGloom Finger\nGold Ring\n--------\n",
    "Item Level: 82\n--------\n+80(70-89) to maximum Life">>.
item() ->
    {ok, Catalog} = retinue_poe2_trade_item:catalog(retinue_poe2_trade_eunit:fixture()),
    {ok, Item} = retinue_poe2_trade_item:parse(text(), Catalog), Item.
success() -> {ok, 200, #{}, <<"{\"results\":[{\"predicted_price\":40,\"predicted_price_low\":30,",
    "\"predicted_price_high\":50,\"predicted_price_confidence\":75}]}">>}.
with_server(Http, Test) ->
    Calls = atomics:new(1, []), Clock = atomics:new(1, []), atomics:put(Clock, 1, 1000),
    {ok, Server} = retinue_poe2_prediction:start_link(#{name => undefined,
        http => fun(Body) -> atomics:add(Calls, 1, 1),
            ?assertMatch(#{<<"gameId">> := <<"poe2">>, <<"items">> := [_]}, json:decode(Body)), Http(Body) end,
        clock => fun() -> atomics:get(Clock, 1) end}),
    try Test(Server, Calls, Clock) after gen_server:stop(Server) end.
