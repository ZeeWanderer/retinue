%%%-------------------------------------------------------------------
%% EUnit coverage for OCR-oriented Market label resolution.
%%%-------------------------------------------------------------------
-module(wfcli_market_resolver_eunit).

-include_lib("eunit/include/eunit.hrl").

ranked_fuzzy_matches_test() ->
    Items = [item(<<"saryn_prime_set">>, <<"Saryn Prime Set">>, 100),
             item(<<"saryn_prime_chassis_blueprint">>,
                  <<"Saryn Prime Chassis Blueprint">>, 100),
             item(<<"soma_prime_set">>, <<"Soma Prime Set">>, 100)],
    [Resolution] = wfcli_market_resolver:resolve(
                     [<<"Saryn Prme Set">>], Items, 2),
    ?assertEqual(<<"Saryn Prme Set">>, maps:get(label, Resolution)),
    [Best, Second] = maps:get(matches, Resolution),
    ?assertEqual(#{name => <<"Saryn Prime Set">>, slug => <<"saryn_prime_set">>,
                   ducats => 100, distance => 1, confidence => 0.9333}, Best),
    ?assertEqual(<<"Soma Prime Set">>, maps:get(name, Second)),
    ?assert(maps:get(confidence, Best) > maps:get(confidence, Second)).

normalization_and_label_order_test() ->
    Items = [item(<<"braton_prime_barrel">>, <<"Braton Prime Barrel">>, 15),
             item(<<"braton_prime_blueprint">>, <<"Braton Prime Blueprint">>, 25)],
    Resolutions = wfcli_market_resolver:resolve(
                    [<<"BRATON--PRIME barrel">>, <<"Braton Prime Blueprnt">>], Items, 1),
    [Barrel, Blueprint] = Resolutions,
    [BarrelMatch] = maps:get(matches, Barrel),
    [BlueprintMatch] = maps:get(matches, Blueprint),
    ?assertEqual(0, maps:get(distance, BarrelMatch)),
    ?assertEqual(1.0, maps:get(confidence, BarrelMatch)),
    ?assertEqual(15, maps:get(ducats, BarrelMatch)),
    ?assertEqual(<<"braton_prime_blueprint">>, maps:get(slug, BlueprintMatch)),
    ?assertEqual(1, maps:get(distance, BlueprintMatch)).

partial_query_prefers_name_matches_test() ->
    Items = [item(<<"saryn_prime_set">>, <<"Saryn Prime Set">>, 100),
             item(<<"saryn_prime_chassis_blueprint">>,
                  <<"Saryn Prime Chassis Blueprint">>, 100),
             item(<<"da_ren">>, <<"Da-Ren">>, undefined),
             item(<<"karkina">>, <<"Karkina">>, undefined)],
    [Prefix, Tokens] = wfcli_market_resolver:resolve(
                         [<<"saryn">>, <<"saryn set">>], Items, 2),
    PrefixNames = [maps:get(name, Match) || Match <- maps:get(matches, Prefix)],
    ?assertEqual([<<"Saryn Prime Set">>, <<"Saryn Prime Chassis Blueprint">>],
                 PrefixNames),
    [TokenMatch | _] = maps:get(matches, Tokens),
    ?assertEqual(<<"Saryn Prime Set">>, maps:get(name, TokenMatch)).

exact_descriptors_test() ->
    Item = (item(<<"arcane_energize">>, <<"Arcane Energize">>, undefined))#{
        <<"id">> => <<"arcane-id">>, <<"bulkTradable">> => true,
        <<"maxRank">> => 5, <<"subtypes">> => [<<"ranked">>],
        <<"i18n">> => #{<<"en">> => #{<<"name">> => <<"Arcane Energize">>,
                                         <<"thumb">> => <<"items/thumb.png">>}}},
    {[ById, ByName], [<<"missing">>]} = wfcli_market_resolver:describe(
                                        [<<"arcane-id">>, <<"ARCANE ENERGIZE">>,
                                         <<"missing">>], [Item]),
    ?assertEqual(ById, ByName),
    ?assertEqual(<<"arcane-id">>, maps:get(id, ById)),
    ?assertEqual(true, maps:get(bulk_tradable, ById)),
    ?assertEqual(5, maps:get(max_rank, ById)),
    ?assertEqual(true, maps:get(tradable, ById)),
    ?assertEqual(#{id => <<"market-item:arcane-id">>, source => <<"market">>,
                   image_name => <<"items/thumb.png">>},
                 maps:get(asset, ById)),
    ?assertNot(maps:is_key(ducats, ById)).

unicode_exact_lookup_test() ->
    Unicode = item(<<"albrechts_archive_scene">>,
                   <<"Albrecht\x{2019}s Archive Scene"/utf8>>, undefined),
    Hush = (item(<<"hush">>, <<"Hush">>, undefined))#{
        <<"id">> => <<"54a74454e779892d5e515621">>},
    {[Descriptor], []} = wfcli_market_resolver:describe(
                           [<<"54a74454e779892d5e515621">>], [Unicode, Hush]),
    ?assertEqual(<<"Hush">>, maps:get(name, Descriptor)).

item(Slug, Name, Ducats) ->
    #{<<"slug">> => Slug,
      <<"ducats">> => Ducats,
      <<"i18n">> => #{<<"en">> => #{<<"name">> => Name}}}.
