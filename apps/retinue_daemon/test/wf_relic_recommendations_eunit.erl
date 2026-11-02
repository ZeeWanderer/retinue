%%%-------------------------------------------------------------------
%% EUnit coverage for owned relic catalog parsing and recommendation scoring.
%%%-------------------------------------------------------------------
-module(wf_relic_recommendations_eunit).

-include_lib("eunit/include/eunit.hrl").

empty_upstream_rewards_use_current_drop_feed_test() ->
    Market = [(item(<<"cheap_prime_part">>, 15))#{
        <<"i18n">> => #{<<"en">> => #{<<"name">> => <<"Cheap Prime Part">>,
                                       <<"thumb">> => <<"cheap.png">>}}}],
    with_catalog_http(empty_rewards(), drop_fixture(), fun() ->
        {ok, Catalog} = wf_relic_recommendations:fetch(Market, #{}),
        Result = wf_relic_recommendations:build(
            <<"axi">>, Catalog, Market, quotes(), player(), planner_options()),
        [Relic] = maps:get(<<"items">>, Result),
        [Reward] = maps:get(<<"rewards">>, Relic),
        ?assertEqual(<<"cheap_prime_part">>, maps:get(<<"slug">>, Reward)),
        ?assertEqual(<<"Cheap Prime Part">>, maps:get(<<"name">>, Reward)),
        ?assertEqual(100, maps:get(<<"chance">>, Reward)),
        ?assertEqual(<<"cheap.png">>, maps:get(<<"image_name">>, maps:get(<<"asset">>, Reward))),
        ?assertEqual(10, maps:get(<<"expected_platinum">>, Relic)),
        ?assertEqual(15, maps:get(<<"expected_ducats">>, Relic)),
        ?assertEqual([<<"cheap_prime_part">>], wf_relic_recommendations:price_slugs(
            <<"axi">>, Catalog, Market, player(), 1))
    end).

unavailable_reward_sources_cannot_publish_empty_success_test() ->
    with_catalog_http(empty_rewards(), jsone:encode(#{<<"relics">> => []}), fun() ->
        ?assertEqual({error, empty_relic_rewards}, wf_relic_recommendations:fetch([], #{}))
    end).

unavailable_rewards_keep_last_good_relic_test() ->
    {ok, Previous} = wf_relic_recommendations:parse(fixture()),
    with_catalog_http(empty_rewards(), {error, timeout}, fun() ->
        {ok, Catalog} = wf_relic_recommendations:fetch([], Previous),
        ?assertEqual(maps:get(<<"relic-a">>, Previous), maps:get(<<"relic-a">>, Catalog))
    end).

valid_primary_does_not_request_fallback_test() ->
    with_catalog_http(fixture(), unexpected_fallback, fun() ->
        {ok, Expected} = wf_relic_recommendations:parse(fixture()),
        ?assertEqual({ok, Expected}, wf_relic_recommendations:fetch([], #{})),
        ?assertEqual(false, get(relic_fallback_called))
    end).

partial_invalid_reward_arrays_are_not_accepted_as_complete_test() ->
    [Relic] = jsone:decode(empty_rewards()),
    Broken = Relic#{<<"rewards">> => [
        #{<<"chance">> => 100, <<"item">> => #{<<"name">> => <<"Forma Blueprint">>}},
        #{<<"item">> => #{<<"name">> => <<"Missing chance">>}}]},
    {ok, Catalog} = wf_relic_recommendations:parse(jsone:encode([Broken])),
    ?assertNot(wf_relic_recommendations:usable(Catalog)),
    with_catalog_http(jsone:encode([Broken]), drop_fixture(), fun() ->
        {ok, Repaired} = wf_relic_recommendations:fetch([], #{}),
        [Reward] = maps:get(rewards, maps:get(<<"relic-a">>, Repaired)),
        ?assertEqual(<<"Cheap Prime Part">>, maps:get(name, Reward))
    end).

generic_era_templates_are_not_playable_relics_test() ->
    Values = jsone:decode(empty_rewards()),
    Template = #{<<"name">> => <<"Axi Relic">>, <<"uniqueName">> => <<"template">>,
                 <<"rewards">> => []},
    {ok, Catalog} = wf_relic_recommendations:parse(jsone:encode([Template | Values])),
    ?assertEqual([<<"relic-a">>], maps:keys(Catalog)).

malformed_drop_rows_do_not_publish_partial_probabilities_test() ->
    Drops = jsone:encode(#{<<"relics">> => [#{<<"tier">> => <<"Axi">>,
        <<"relicName">> => <<"A1">>, <<"state">> => <<"Intact">>,
        <<"rewards">> => [#{<<"itemName">> => <<"Part">>, <<"chance">> => 50},
                          #{<<"itemName">> => <<"Other">>, <<"chance">> => <<"50">>}]}]}),
    with_catalog_http(empty_rewards(), Drops, fun() ->
        ?assertEqual({error, empty_relic_rewards}, wf_relic_recommendations:fetch([], #{}))
    end).

empty_rewards() ->
    jsone:encode([#{<<"uniqueName">> => <<"relic-a">>,
        <<"name">> => <<"Axi A1 Intact">>, <<"rewards">> => []}]).

drop_fixture() ->
    jsone:encode(#{<<"relics">> => [#{<<"tier">> => <<"Axi">>,
        <<"relicName">> => <<"A1">>, <<"state">> => <<"Intact">>,
        <<"rewards">> => [#{<<"itemName">> => <<"Cheap Prime Part">>,
                            <<"chance">> => 100, <<"rarity">> => <<"Common">>}]}]}).

with_catalog_http(Primary, Fallback, Fun) ->
    Previous = application:get_env(retinue_daemon, relic_catalog_http_fun),
    put(relic_fallback_called, false),
    application:set_env(retinue_daemon, relic_catalog_http_fun, fun(Url, _) ->
        case lists:suffix("Relics.json", Url) of
            true -> {ok, 200, Primary};
            false ->
                put(relic_fallback_called, true),
                case Fallback of
                    {error, _} -> Fallback;
                    _ -> {ok, 200, Fallback}
                end
        end
    end),
    try Fun()
    after
        erase(relic_fallback_called),
        case Previous of
            {ok, Old} -> application:set_env(retinue_daemon, relic_catalog_http_fun, Old);
            undefined -> application:unset_env(retinue_daemon, relic_catalog_http_fun)
        end
    end.

parses_and_ranks_owned_relics_test() ->
    {ok, Catalog} = wf_relic_recommendations:parse(fixture()),
    Result = wf_relic_recommendations:build(
               <<"axi">>, Catalog, items(), quotes(), player(), planner_options()),
    [First, Second] = maps:get(<<"items">>, Result),
    ?assertEqual(<<"Axi A1 Relic">>, maps:get(<<"name">>, First)),
    ?assertEqual(19, maps:get(<<"expected_platinum">>, First)),
    ?assertEqual(95, maps:get(<<"expected_ducats">>, First)),
    ?assertEqual(true, maps:get(<<"price_complete">>, First)),
    ?assertEqual(<<"axi">>, maps:get(<<"era">>, First)),
    ?assertEqual(<<"Intact">>, maps:get(<<"refinement">>, First)),
    ?assertEqual(<<"RelicAxiD.png">>,
                 maps:get(<<"image_name">>, maps:get(<<"asset">>, First))),
    [FirstReward | _] = maps:get(<<"rewards">>, First),
    ?assertEqual(<<"Cheap prime part">>, maps:get(<<"name">>, FirstReward)),
    ?assertEqual(3, maps:get(<<"amount_owned">>, First)),
    ?assertEqual(2, length(maps:get(<<"refinements">>, First))),
    ?assertEqual(<<"Axi B1 Relic">>, maps:get(<<"name">>, Second)),
    ?assertEqual(<<"RelicAxiD.png">>,
                 maps:get(<<"image_name">>, maps:get(<<"asset">>, Second))),
    ?assertEqual(1842, maps:get(<<"trace_count">>, Result)),
    ?assertEqual(0, maps:get(<<"revision">>, Result)).

missing_quotes_keep_ducat_ranking_and_null_price_test() ->
    {ok, Catalog} = wf_relic_recommendations:parse(fixture()),
    Result = wf_relic_recommendations:build(
               <<"axi">>, Catalog, items(), #{}, player(), planner_options()),
    [First | _] = maps:get(<<"items">>, Result),
    ?assertEqual(<<"Axi A1 Relic">>, maps:get(<<"name">>, First)),
    ?assertEqual(null, maps:get(<<"expected_platinum">>, First)).

planner_can_include_unowned_catalog_relics_test() ->
    {ok, Catalog0} = wf_relic_recommendations:parse(fixture()),
    Variants = [{<<"relic-ci">>, <<"Intact">>},
                {<<"relic-ce">>, <<"Exceptional">>},
                {<<"relic-cf">>, <<"Flawless">>},
                {<<"relic-cr">>, <<"Radiant">>}],
    Catalog = lists:foldl(
                fun({Id, Refinement}, Acc) ->
                    Acc#{Id => #{name => <<"Axi C1 ", Refinement/binary>>,
                                 era => <<"axi">>, vaulted => false, rewards => []}}
                end,
                Catalog0,
                Variants),
    Result = wf_relic_recommendations:build(
               <<"axi">>, Catalog, items(), quotes(), player(),
               (planner_options())#{only_owned => false}),
    [Unowned] = [Item || Item <- maps:get(<<"items">>, Result),
                          maps:get(<<"name">>, Item) =:= <<"Axi C1 Relic">>],
    ?assertEqual(0, maps:get(<<"amount_owned">>, Unowned)),
    ?assertEqual([{<<"Intact">>, 0}, {<<"Exceptional">>, 0},
                  {<<"Flawless">>, 0}, {<<"Radiant">>, 0}],
                 [{maps:get(<<"refinement">>, Row),
                   maps:get(<<"amount_owned">>, Row)}
                  || Row <- maps:get(<<"refinements">>, Unowned)]).

partial_quotes_return_marked_estimate_test() ->
    {ok, Catalog} = wf_relic_recommendations:parse(fixture()),
    Result = wf_relic_recommendations:build(
               <<"axi">>, Catalog, items(),
               #{<<"expensive_prime_part">> => #{lowest_sell => 20}}, player(),
               planner_options()),
    [First | _] = maps:get(<<"items">>, Result),
    ?assert(is_integer(maps:get(<<"expected_platinum">>, First))),
    ?assertEqual(false, maps:get(<<"price_complete">>, First)),
    ?assertEqual(1, maps:get(<<"priced_rewards">>, First)),
    ?assertEqual(2, maps:get(<<"priceable_rewards">>, First)).

legacy_catalog_rewards_still_use_cached_quotes_test() ->
    {ok, Catalog} = wf_relic_recommendations:parse(fixture()),
    LegacyCatalog = maps:map(
                      fun(_Id, Relic) ->
                          Relic#{rewards => [maps:remove(name, Reward)
                                            || Reward <- maps:get(rewards, Relic)]}
                      end,
                      Catalog),
    Result = wf_relic_recommendations:build(
               <<"axi">>, LegacyCatalog, items(), quotes(), player(), planner_options()),
    [First | _] = maps:get(<<"items">>, Result),
    ?assertEqual(19, maps:get(<<"expected_platinum">>, First)).

selects_visible_candidate_reward_slugs_test() ->
    {ok, Catalog} = wf_relic_recommendations:parse(fixture()),
    ?assertEqual(
       [<<"cheap_prime_part">>, <<"expensive_prime_part">>],
       wf_relic_recommendations:price_slugs(
         <<"axi">>, Catalog, items(), player(), 1)).

all_includes_requiem_relics_test() ->
    {ok, Catalog} = wf_relic_recommendations:parse(fixture()),
    Result = wf_relic_recommendations:build(
               <<"all">>, Catalog, items(), quotes(), player(), planner_options()),
    Requiem = [Item || Item <- maps:get(<<"items">>, Result),
                       maps:get(<<"era">>, Item) =:= <<"requiem">>],
    [Relic] = Requiem,
    ?assertEqual(<<"Requiem I1 Relic">>, maps:get(<<"name">>, Relic)).

requiem_can_be_selected_explicitly_test() ->
    {ok, Catalog} = wf_relic_recommendations:parse(fixture()),
    Result = wf_relic_recommendations:build(
               <<"requiem">>, Catalog, items(), quotes(), player(),
               recommendation_options(all)),
    [Relic] = maps:get(<<"items">>, Result),
    ?assertEqual(<<"Requiem I1 Intact">>, maps:get(<<"name">>, Relic)),
    ?assertEqual(<<"RelicImmortalD.png">>,
                 maps:get(<<"image_name">>, maps:get(<<"asset">>, Relic))).

vanguard_uses_projection_tier_test() ->
    Body = jsone:encode([
        #{<<"uniqueName">> => <<"/Lotus/Types/Game/Projections/T4VanguardTestABronze">>,
          <<"name">> => <<"Vanguard V1 Intact">>, <<"vaulted">> => true,
          <<"imageName">> => <<"RelicAxiD.png">>, <<"rewards">> => []}
    ]),
    {ok, Catalog} = wf_relic_recommendations:parse(Body),
    Player = #{data => #{<<"inventory">> => #{<<"index">> => #{<<"stacks">> => [
        #{<<"item_type">> => <<"/Lotus/Types/Game/Projections/T4VanguardTestABronze">>,
          <<"count">> => 1}
    ]}}}},
    Axi = wf_relic_recommendations:build(
            <<"axi">>, Catalog, [], #{}, Player, planner_options()),
    [Relic] = maps:get(<<"items">>, Axi),
    ?assertEqual(<<"Vanguard V1 Relic">>, maps:get(<<"name">>, Relic)),
    ?assertEqual(<<"axi">>, maps:get(<<"era">>, Relic)),
    All = wf_relic_recommendations:build(
            <<"all">>, Catalog, [], #{}, Player, planner_options()),
    ?assertEqual(1, length(maps:get(<<"items">>, All))).

unknown_vanguard_tier_remains_visible_in_all_test() ->
    Body = jsone:encode([
        #{<<"uniqueName">> => <<"/Lotus/Types/Game/Projections/VanguardTestABronze">>,
          <<"name">> => <<"Vanguard V2 Intact">>, <<"rewards">> => []}
    ]),
    {ok, Catalog} = wf_relic_recommendations:parse(Body),
    Player = #{data => #{<<"inventory">> => #{<<"index">> => #{<<"stacks">> => [
        #{<<"item_type">> => <<"/Lotus/Types/Game/Projections/VanguardTestABronze">>,
          <<"count">> => 1}
    ]}}}},
    All = wf_relic_recommendations:build(
            <<"all">>, Catalog, [], #{}, Player, planner_options()),
    [Relic] = maps:get(<<"items">>, All),
    ?assertEqual(<<"vanguard">>, maps:get(<<"era">>, Relic)),
    Axi = wf_relic_recommendations:build(
            <<"axi">>, Catalog, [], #{}, Player, planner_options()),
    ?assertEqual([], maps:get(<<"items">>, Axi)).

recommendation_limit_is_request_scoped_test() ->
    Entries = lists:seq(1, 60),
    Catalog = maps:from_list(
                [{relic_id(Index),
                  #{name => iolist_to_binary(io_lib:format("Axi ~2..0B Intact", [Index])),
                    era => <<"axi">>, vaulted => false, rewards => []}}
                 || Index <- Entries]),
    Stacks = [#{<<"item_type">> => relic_id(Index), <<"count">> => 1}
              || Index <- Entries],
    Player = #{data => #{<<"inventory">> => #{<<"index">> => #{<<"stacks">> => Stacks}}}},
    Full = wf_relic_recommendations:build(
             <<"axi">>, Catalog, [], #{}, Player, recommendation_options(all)),
    Limited = wf_relic_recommendations:build(
                <<"axi">>, Catalog, [], #{}, Player, recommendation_options(32)),
    ?assertEqual(60, length(maps:get(<<"items">>, Full))),
    ?assertEqual(32, length(maps:get(<<"items">>, Limited))).

recommendations_keep_refinement_variants_test() ->
    {ok, Catalog} = wf_relic_recommendations:parse(fixture()),
    Result = wf_relic_recommendations:build(
               <<"axi">>, Catalog, items(), quotes(), player(),
               recommendation_options(all)),
    Names = [maps:get(<<"name">>, Item) || Item <- maps:get(<<"items">>, Result)],
    ?assert(lists:member(<<"Axi A1 Intact">>, Names)),
    ?assert(lists:member(<<"Axi A1 Radiant">>, Names)),
    ?assertEqual(false, lists:any(fun(Item) -> maps:is_key(<<"refinements">>, Item) end,
                                 maps:get(<<"items">>, Result))).

forma_reward_uses_embedded_asset_and_local_value_test() ->
    Body = jsone:encode([
        #{<<"uniqueName">> => <<"relic-forma">>, <<"name">> => <<"Axi F1 Intact">>,
          <<"rewards">> => [
              #{<<"chance">> => 100, <<"rarity">> => <<"Common">>,
                <<"item">> => #{<<"name">> => <<"Forma Blueprint">>,
                                 <<"uniqueName">> => <<"projection-forma">>}}
          ]}
    ]),
    {ok, Catalog} = wf_relic_recommendations:parse(Body),
    Player = #{data => #{<<"inventory">> => #{<<"index">> => #{<<"stacks">> => [
        #{<<"item_type">> => <<"relic-forma">>, <<"count">> => 1}
    ]}}}},
    Result = wf_relic_recommendations:build(
               <<"axi">>, Catalog, [], #{}, Player, planner_options()),
    [Relic] = maps:get(<<"items">>, Result),
    [Reward] = maps:get(<<"rewards">>, Relic),
    ?assertEqual(2, maps:get(<<"expected_platinum">>, Relic)),
    ?assertEqual(2, maps:get(<<"platinum">>, Reward)),
    ?assertEqual(0, maps:get(<<"ducats">>, Reward)),
    Asset = maps:get(<<"asset">>, Reward),
    ?assertEqual(<<"builtin:forma">>, maps:get(<<"id">>, Asset)),
    ?assertEqual(<<"builtin">>, maps:get(<<"source">>, Asset)).

fixture() ->
    jsone:encode([
        #{<<"uniqueName">> => <<"relic-a">>,
          <<"name">> => <<"Axi A1 Intact">>, <<"vaulted">> => false,
          <<"imageName">> => <<"RelicAxiD.png">>,
          <<"rewards">> => [
              reward(50, <<"cheap_prime_part">>),
              reward(50, <<"expensive_prime_part">>)
          ]},
        #{<<"uniqueName">> => <<"relic-ar">>,
          <<"name">> => <<"Axi A1 Radiant">>, <<"vaulted">> => false,
          <<"imageName">> => <<"RelicAxiC.png">>,
          <<"rewards">> => [
              reward(50, <<"cheap_prime_part">>),
              reward(50, <<"expensive_prime_part">>)
          ]},
        #{<<"uniqueName">> => <<"relic-b">>,
          <<"name">> => <<"Axi B1 Intact">>, <<"vaulted">> => true,
          <<"rewards">> => [reward(100, <<"medium_prime_part">>)]},
        #{<<"uniqueName">> => <<"relic-r">>,
          <<"name">> => <<"Requiem I1 Intact">>, <<"vaulted">> => false,
          <<"rewards">> => [reward(100, <<"medium_prime_part">>)]},
        #{<<"uniqueName">> => <<"void">>, <<"name">> => <<"Void Relic">>,
          <<"rewards">> => []}
    ]).

relic_id(Index) -> iolist_to_binary(io_lib:format("relic-~B", [Index])).

reward(Chance, Slug) ->
    Name = iolist_to_binary(
             [string:titlecase(string:replace(binary_to_list(Slug), "_", " ", all))]),
    #{<<"chance">> => Chance, <<"rarity">> => <<"Common">>,
      <<"item">> => #{<<"name">> => Name,
                       <<"warframeMarket">> => #{<<"urlName">> => Slug}}}.

items() ->
    [item(<<"cheap_prime_part">>, 15),
     item(<<"expensive_prime_part">>, 100),
     item(<<"medium_prime_part">>, 45)].

item(Slug, Ducats) ->
    #{<<"slug">> => Slug, <<"ducats">> => Ducats,
      <<"gameRef">> => <<"game:", Slug/binary>>,
      <<"i18n">> => #{<<"en">> => #{<<"thumb">> => <<"items/images/", Slug/binary,
                                                        ".png">>}}}.

quotes() ->
    #{<<"cheap_prime_part">> => #{lowest_sell => 10},
      <<"expensive_prime_part">> => #{lowest_sell => 20},
      <<"medium_prime_part">> => #{lowest_sell => 12}}.

player() ->
    #{data => #{<<"inventory">> => #{<<"index">> => #{<<"stacks">> => [
        #{<<"item_type">> => <<"relic-a">>, <<"count">> => 2},
        #{<<"item_type">> => <<"relic-ar">>, <<"count">> => 1},
        #{<<"item_type">> => <<"relic-b">>, <<"count">> => 1},
        #{<<"item_type">> => <<"relic-r">>, <<"count">> => 1},
        #{<<"item_type">> =>
              <<"/Lotus/Types/Items/MiscItems/VoidTearDrop">>,
          <<"count">> => 1842}
    ]}}}}.

planner_options() -> #{view => planner, limit => all}.

recommendation_options(Limit) -> #{view => recommendations, limit => Limit}.
