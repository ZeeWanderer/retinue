-module(wf_ducat_inventory_eunit).

-include_lib("eunit/include/eunit.hrl").

blueprint_and_built_component_keep_distinct_market_identities_test() ->
    Quote = #{lowest_sell => 12, quoted_at => 123},
    Result = wf_ducat_inventory:build(snapshot(), catalog(), [market()],
                                      #{<<"saryn_prime_chassis_blueprint">> => Quote}),
    Items = maps:from_list([{maps:get(<<"id">>, I), I} || I <- maps:get(<<"items">>, Result)]),
    ?assertEqual(2, map_size(Items)),
    Built = maps:get(component(), Items),
    Blueprint = maps:get(blueprint(), Items),
    ?assertEqual(null, maps:get(<<"slug">>, Built)),
    ?assertEqual(null, maps:get(<<"quote">>, Built)),
    ?assertEqual(<<"Saryn Prime Chassis">>, maps:get(<<"name">>, Built)),
    ?assertEqual(<<"Saryn Prime Chassis Blueprint">>, maps:get(<<"name">>, Blueprint)),
    ?assertEqual(<<"saryn_prime_chassis_blueprint">>, maps:get(<<"slug">>, Blueprint)),
    ?assertEqual(Quote, maps:get(<<"quote">>, Blueprint)),
    Parent = maps:get(<<"parent">>, Built),
    ?assertEqual(Parent, maps:get(<<"parent">>, Blueprint)),
    ?assertEqual(#{<<"name">> => <<"Saryn Prime">>, <<"owned">> => true,
                   <<"mastered">> => true, <<"subsumed">> => false}, Parent).

metadata_does_not_require_quotes_or_a_market_listing_test() ->
    #{<<"items">> := Items} = wf_ducat_inventory:build(snapshot(), catalog(), [], #{}),
    ?assertEqual(2, length(Items)),
    lists:foreach(fun(I) ->
        ?assertEqual(null, maps:get(<<"quote">>, I)),
        ?assertEqual(null, maps:get(<<"slug">>, I)),
        ?assert(maps:get(<<"mastered">>, maps:get(<<"parent">>, I)))
    end, Items),
    ?assertEqual(#{<<"items">> => []}, wf_ducat_inventory:build(#{}, catalog(), [market()], #{})).

ambiguous_market_identity_is_not_guessed_test() ->
    #{<<"items">> := Items} = wf_ducat_inventory:build(snapshot(), catalog(),
        [market(), (market())#{<<"slug">> := <<"different">>}], #{}),
    ?assert(lists:all(fun(I) -> maps:get(<<"slug">>, I) =:= null end, Items)).

parent_ownership_is_not_inferred_from_parts_or_mastery_test() ->
    #{data := #{<<"inventory">> := Inventory}} = Snapshot = snapshot(),
    Index = maps:get(<<"index">>, Inventory),
    WithoutFrame = Snapshot#{data := #{<<"inventory">> => Inventory#{
        <<"index">> := Index#{<<"equipment">> := []}}}},
    #{<<"items">> := Items} = wf_ducat_inventory:build(WithoutFrame, catalog(), [market()], #{}),
    lists:foreach(fun(#{<<"parent">> := Parent}) ->
        ?assertNot(maps:get(<<"owned">>, Parent)),
        ?assert(maps:get(<<"mastered">>, Parent))
    end, Items).

empty_order_book_is_null_and_books_are_not_copied_into_metadata_test() ->
    #{<<"items">> := Items} = wf_ducat_inventory:build(snapshot(), catalog(), [market()],
        #{<<"saryn_prime_chassis_blueprint">> => #{lowest_sell => undefined,
          quoted_at => 123, sell_orders => [], buy_orders => []}}),
    [Quote] = [Q || #{<<"quote">> := Q} <- Items, is_map(Q)],
    ?assertEqual(#{lowest_sell => null, quoted_at => 123}, Quote).

root() -> <<"/Lotus/Powersuits/Saryn/SarynPrime">>.
component() -> <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeChassisComponent">>.
blueprint() -> <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeChassisBlueprint">>.

snapshot() ->
    #{data => #{<<"inventory">> => #{<<"index">> => #{
        <<"equipment">> => [#{<<"item_type">> => root(), <<"count">> => 1}],
        <<"mastery">> => [#{<<"item_type">> => root(), <<"xp">> => 900000}],
        <<"stacks">> => [#{<<"item_type">> => Ref, <<"count">> => 1}
                           || Ref <- [component(), blueprint()]]}}}}.

catalog() ->
    [#{<<"uniqueName">> => root(), <<"name">> => <<"Saryn Prime">>,
       <<"category">> => <<"Warframes">>, <<"type">> => <<"Warframe">>,
       <<"isPrime">> => true, <<"masterable">> => true,
       <<"components">> => [#{<<"uniqueName">> => component(), <<"name">> => <<"Chassis">>,
           <<"itemCount">> => 1, <<"primeSellingPrice">> => 100, <<"tradable">> => true,
           <<"recipeAliases">> => [blueprint()]}]}].

market() ->
    #{<<"gameRef">> => blueprint(), <<"slug">> => <<"saryn_prime_chassis_blueprint">>,
      <<"tags">> => [<<"component">>, <<"blueprint">>],
      <<"i18n">> => #{<<"en">> => #{<<"name">> => <<"Saryn Prime Chassis Blueprint">>}}}.
