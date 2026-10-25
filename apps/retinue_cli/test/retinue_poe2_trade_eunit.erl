-module(retinue_poe2_trade_eunit).
-include_lib("eunit/include/eunit.hrl").
-export([fixture/0, item/0, http/4]).

parser_test() ->
    {ok, Item} = parse(item()),
    ?assertMatch(#{<<"name">> := <<"Doom Shelter">>, <<"base">> := <<"Felt Cap">>,
                   <<"rarity">> := <<"Rare">>, <<"item_level">> := 77,
                   <<"corrupted">> := true, <<"unidentified">> := false}, Item),
    [Life, Range, Negative, Unknown, Ambiguous, Implicit, Augment, Bonded] = maps:get(<<"modifiers">>, Item),
    ?assertMatch(#{<<"id">> := <<"explicit.stat_3299347043">>, <<"min">> := 82, <<"max">> := 82}, Life),
    ?assertMatch(#{<<"id">> := <<"explicit.stat_1940865751">>, <<"min">> := 15.0, <<"max">> := 15.0}, Range),
    ?assertMatch(#{<<"min">> := -12.5}, Negative),
    ?assertMatch(#{<<"supported">> := false, <<"id">> := null}, Unknown),
    ?assertMatch(#{<<"supported">> := true,
        <<"id">> := <<"explicit.stat_2144192055,explicit.stat_53045048">>}, Ambiguous),
    ?assertMatch(#{<<"id">> := <<"implicit.stat_3299347043">>, <<"section">> := <<"implicit">>}, Implicit),
    ?assertMatch(#{<<"id">> := <<"rune.stat_3299347043">>, <<"section">> := <<"augment">>}, Augment),
    ?assertMatch(#{<<"id">> := <<"rune.stat_2280525771">>, <<"min">> := 25}, Bonded),
    ?assertEqual({ok, Item}, parse(item())).

profile_ranges_test_() ->
    {ok, Bytes} = file:read_file("apps/retinue_cli/test/fixtures/poe2_trade_profiles.json"),
    [{binary_to_list(Name), ?_assertEqual(Expected,
        retinue_poe2_trade_profiles:ranges(Modifier#{<<"supported">> => true}))}
     || #{<<"name">> := Name, <<"modifier">> := Modifier, <<"expected">> := Expected}
         <- json:decode(Bytes)].

original_parser_cases_test_() ->
    #{<<"cases">> := Cases} = read_fixture("poe2_modifier_cases.json"),
    [{binary_to_list(maps:get(<<"name">>, Case)), fun() -> original_parser_case(Case) end} || Case <- Cases].

original_parser_case(Case) ->
    Name = maps:get(<<"name">>, Case),
    {ok, Text} = file:read_file(filename:join([filename:dirname(?FILE), "fixtures", "poe2_" ++ binary_to_list(Name) ++ ".txt"])),
    Http = fun
        (get, <<"/api/trade2/data/stats">>, _, _) ->
            {ok, 200, #{}, iolist_to_binary(json:encode(read_fixture("poe2_modifier_stats.json")))};
        (get, <<"/api/trade2/data/items">>, _, _) ->
            {ok, 200, #{}, <<"{\"result\":[{\"entries\":[{\"type\":\"Stone Charm\"}]}]}">>};
        (Method, Path, Headers, Body) -> http(Method, Path, Headers, Body)
    end,
    with_service(fun(Server, _, _) ->
        {ok, #{<<"item">> := Item}} = retinue_poe2_trade:request(Server,
            #{<<"op">> => <<"trade_item">>, <<"text">> => Text}),
        ?assertEqual(maps:get(<<"base">>, Case), maps:get(<<"base">>, Item)),
        ?assertEqual(maps:get(<<"category">>, Case), maps:get(<<"category">>, Item)),
        Rows = [R || R <- maps:get(<<"modifiers">>, Item), maps:get(<<"section">>, R) =/= <<"property">>],
        ?assert(lists:all(fun(R) -> maps:get(<<"supported">>, R) end, Rows)),
        maps:foreach(fun(Section, Hashes) ->
            Target = case Section of <<"rune">> -> <<"augment">>; _ -> Section end,
            ?assertEqual([Id || [Id, _] <- Hashes],
                [maps:get(<<"id">>, R) || R <- Rows, maps:get(<<"section">>, R) =:= Target])
        end, maps:get(<<"hashes">>, Case)),
        Filters = [maps:merge(#{<<"id">> => maps:get(<<"id">>, R)},
            maps:get(<<"allmods">>, maps:get(<<"ranges">>, R))) || R <- Rows],
        {ok, #{<<"query">> := Query}} = build((search_request())#{<<"filters">> => Filters}, Item, leagues()),
        Stats = [G#{<<"filters">> => [maps:remove(<<"disabled">>, F) || F <- maps:get(<<"filters">>, G)]}
                 || G <- maps:get(<<"stats">>, Query)],
        ?assertEqual(maps:get(<<"stats">>, Case), Stats),
        ?assertEqual(maps:get(<<"rarity">>, Item) =/= <<"Rare">>, maps:get(<<"auto_search">>, Item))
    end, Http).

read_fixture(Name) ->
    {ok, Bytes} = file:read_file(filename:join([filename:dirname(?FILE), "fixtures", Name])),
    json:decode(Bytes).

automatic_search_categories_test() ->
    Base = #{<<"name">> => <<>>, <<"rarity">> => <<"Rare">>, <<"category">> => <<"accessory.ring">>,
             <<"conditions">> => [], <<"properties">> => [], <<"modifiers">> => []},
    Cases = [
        {#{}, false}, {#{<<"rarity">> => <<"Unique">>}, true},
        {#{<<"rarity">> => <<"Unique">>, <<"unidentified">> => true}, false},
        {#{<<"rarity">> => <<"Normal">>}, true},
        {#{<<"rarity">> => <<"Normal">>, <<"conditions">> => [#{<<"text">> => <<"Corrupted">>}]}, false},
        {#{<<"category">> => <<"jewel">>}, false}, {#{<<"category">> => <<"map.waystone">>}, false},
        {#{<<"category">> => <<"weapon.bow">>}, false},
        {#{<<"category">> => <<"flask.charm">>}, true}, {#{<<"category">> => <<"sanctum.relic">>}, true},
        {#{<<"category">> => <<"gem.activegem">>, <<"rarity">> => <<"Gem">>}, true},
        {#{<<"rarity">> => <<"Currency">>}, true},
        {#{<<"rarity">> => <<"Currency">>, <<"trade_unavailable">> => <<"not_in_trade_catalog">>}, false},
        {#{<<"properties">> => [#{<<"text">> => <<"Fractured Item">>}]}, true},
        {#{<<"properties">> => [#{<<"text">> => <<"Quality: +21% (augmented)">>}]}, true},
        {#{<<"category">> => <<"armour.chest">>, <<"properties">> => [#{<<"text">> => <<"Sockets: S S">>}]}, false},
        {#{<<"category">> => <<"armour.chest">>, <<"properties">> => [#{<<"text">> => <<"Sockets: S S S">>}]}, true}],
    lists:foreach(fun({Changes, Expected}) ->
        ?assertEqual({Changes, Expected}, {Changes, maps:get(<<"auto_search">>,
            retinue_poe2_trade_profiles:prepare(maps:merge(Base, Changes)))})
    end, Cases).

parsed_profile_ranges_test() ->
    {ok, Item} = parse(equipment(<<"+35 to maximum Life\n",
        "+35(30-40) to maximum Life\n",
        "12.5(10-15)% reduced Movement Speed\n",
        "Adds 10(8-12) to 20(18-22) Physical Damage\n",
        "Unmapped modifier">>)),
    [Plain, Variable, Reduced, Damage, Unknown] = maps:get(<<"modifiers">>, Item),
    ?assertEqual(<<"+35 to maximum Life">>, maps:get(<<"text">>, Variable)),
    ?assertEqual(<<"12.5% reduced Movement Speed">>, maps:get(<<"text">>, Reduced)),
    ?assertEqual(#{<<"allmods">> => #{<<"min">> => 35}, <<"fuzzy">> => #{<<"min">> => 35}},
                 maps:get(<<"ranges">>, Plain)),
    ?assertMatch(#{<<"fuzzy">> := #{<<"min">> := 32}}, maps:get(<<"ranges">>, Variable)),
    ?assertMatch(#{<<"fuzzy">> := #{<<"min">> := -13.75}}, maps:get(<<"ranges">>, Reduced)),
    ?assertMatch(#{<<"fuzzy">> := #{<<"min">> := 14}}, maps:get(<<"ranges">>, Damage)),
    ?assertEqual(#{<<"allmods">> => #{}, <<"fuzzy">> => #{}}, maps:get(<<"ranges">>, Unknown)).

exact_property_filter_preserves_decimal_value_test() ->
    ?assertMatch(#{<<"allmods">> := #{<<"min">> := 1.13}, <<"fuzzy">> := #{<<"min">> := 1.01}},
        retinue_poe2_trade_profiles:ranges(#{<<"section">> => <<"property">>, <<"id">> => <<"property.aps">>,
            <<"supported">> => true, <<"min">> => 1.13})).

gem_parser_and_filters_test() ->
    {ok, Text} = file:read_file("apps/retinue_cli/test/fixtures/poe2_tame_beast.txt"),
    {ok, Item} = parse(Text),
    ?assertMatch(#{<<"base">> := <<"Tame Beast">>, <<"rarity">> := <<"Gem">>}, Item),
    ?assertNot(maps:is_key(<<"item_level">>, Item)),
    ?assertMatch([#{<<"id">> := <<"property.gem_level">>, <<"min">> := 13, <<"selected">> := true,
                   <<"ranges">> := #{<<"allmods">> := #{<<"min">> := 13}, <<"fuzzy">> := #{<<"min">> := 11}}},
                  #{<<"id">> := <<"property.gem_sockets">>, <<"min">> := 2,
                   <<"ranges">> := #{<<"allmods">> := #{<<"min">> := 2, <<"max">> := 2}}}],
                 maps:get(<<"modifiers">>, Item)),
    Request = (search_request())#{<<"base">> => false, <<"filters">> => [
        #{<<"id">> => <<"property.gem_level">>, <<"min">> => 13, <<"max">> => 15},
        #{<<"id">> => <<"property.gem_sockets">>, <<"min">> => 2}]},
    {ok, #{<<"query">> := Query}} = build(Request, Item, leagues()),
    ?assertMatch(#{<<"type">> := <<"Tame Beast">>, <<"stats">> := [#{<<"filters">> := []}],
        <<"filters">> := #{<<"type_filters">> := #{<<"filters">> := #{
            <<"category">> := #{<<"option">> := <<"gem">>}}},
            <<"misc_filters">> := #{<<"filters">> := #{
                <<"gem_level">> := #{<<"min">> := 13, <<"max">> := 15},
                <<"gem_sockets">> := #{<<"min">> := 2}}}}}, Query),
    ?assertNot(maps:is_key(<<"rarity">>, maps:get(<<"filters">>, maps:get(<<"type_filters">>, maps:get(<<"filters">>, Query))))),
    ?assertNot(maps:is_key(<<"identified">>, maps:get(<<"filters">>, maps:get(<<"misc_filters">>, maps:get(<<"filters">>, Query))))),
    {ok, #{<<"query">> := Broad}} = build(
        Request#{<<"filters">> => []}, Item, leagues()),
    ?assertNot(maps:is_key(<<"gem_level">>, maps:get(<<"filters">>, maps:get(<<"misc_filters">>, maps:get(<<"filters">>, Broad))))),
    {ok, Quality} = parse(binary:replace(Text, <<"Level: 13">>, <<"Level: 13\nQuality: +20% (augmented)">>)),
    QualityRequest = Request#{<<"filters">> => [#{<<"id">> => <<"property.quality">>, <<"min">> => 20}]},
    ?assertMatch({ok, #{<<"query">> := #{<<"filters">> := #{<<"type_filters">> := #{
        <<"filters">> := #{<<"quality">> := #{<<"min">> := 20}}}}}}},
        build(QualityRequest, Quality, leagues())),
    ?assertMatch({error, #{<<"reason">> := <<"unsupported_filter">>}},
        build(QualityRequest, Item, leagues())).

currency_and_quest_text_test() ->
    {ok, OmenText} = file:read_file("apps/retinue_cli/test/fixtures/poe2_omen.txt"),
    {ok, LogbookText} = file:read_file("apps/retinue_cli/test/fixtures/poe2_quest_logbook.txt"),
    {ok, Omen} = parse(OmenText),
    {ok, Logbook} = parse(LogbookText),
    ?assertMatch(#{<<"rarity">> := <<"Currency">>, <<"base">> := <<"Omen of Amelioration">>,
                   <<"modifiers">> := []}, Omen),
    ?assertMatch(#{<<"rarity">> := <<"Quest">>, <<"base">> := <<"Vorana's Logbook">>,
                   <<"modifiers">> := []}, Logbook),
    {ok, #{<<"query">> := Query}} = build(
        (search_request())#{<<"base">> => false}, Omen, leagues()),
    ?assertMatch(#{<<"type">> := <<"Omen of Amelioration">>}, Query),
    #{<<"type_filters">> := #{<<"filters">> := Types},
      <<"misc_filters">> := #{<<"filters">> := Misc}} = maps:get(<<"filters">>, Query),
    ?assertEqual(#{}, Types),
    ?assertEqual(#{}, Misc),
    with_service(fun(Server, Calls, _) ->
        {ok, #{<<"item">> := Resolved}} = retinue_poe2_trade:request(Server,
            #{<<"op">> => <<"trade_item">>, <<"text">> => LogbookText}),
        ?assertMatch(#{<<"trade_unavailable">> := <<"not_in_trade_catalog">>}, Resolved),
        Before = atomics:get(Calls, 1),
        ?assertMatch({error, #{<<"reason">> := <<"item_not_in_trade_catalog">>}},
            retinue_poe2_trade:request(Server, (search_request())#{<<"text">> => LogbookText})),
        ?assertEqual(Before, atomics:get(Calls, 1))
    end, fun http/4),
    with_service(fun(Server, _, _) ->
        {ok, #{<<"item">> := Resolved}} = retinue_poe2_trade:request(Server,
            #{<<"op">> => <<"trade_item">>, <<"text">> => OmenText}),
        ?assertNot(maps:is_key(<<"trade_unavailable">>, Resolved))
    end, fun http/4).

advanced_multiline_and_boolean_test() ->
    Text = equipment(<<"{ Implicit Modifier }\nCulling Strike\n--------\n",
        "{ Prefix Modifier \"Healthy\" (Tier: 1) }\n+82(70-89) to maximum Life\n",
        "10% increased Rarity of Items found\nYour other Modifiers to Rarity of Items found do not apply">>),
    {ok, Item} = parse(Text),
    [Boolean, Life, Ambiguous] = maps:get(<<"modifiers">>, Item),
    ?assertMatch(#{<<"id">> := <<"implicit.stat_1574531783">>, <<"min">> := null,
                   <<"supported">> := true}, Boolean),
    ?assertEqual(82, maps:get(<<"min">>, Life)),
    ?assertMatch(#{<<"id">> := <<"explicit.stat_1602191394,explicit.stat_2261942307">>,
                   <<"supported">> := true}, Ambiguous),
    ?assertMatch({_, _}, binary:match(maps:get(<<"text">>, Ambiguous), <<"\n">>)).

decorated_sections_do_not_promote_flavour_or_affix_names_test() ->
    {ok, Item} = parse(equipment(<<"{ Prefix Modifier \"Fractured Spirit\" (Tier: 1) }\n",
        "+82 to maximum Life\n--------\n+999 to maximum Life\n--------\n",
        "+10 to maximum Life (implicit)">>)),
    ?assertMatch([#{<<"section">> := <<"explicit">>, <<"min">> := 82},
                  #{<<"section">> := <<"implicit">>, <<"min">> := 10}], maps:get(<<"modifiers">>, Item)).

advanced_metadata_test() ->
    {ok, Text} = file:read_file("apps/retinue_cli/test/fixtures/poe2_advanced_item.txt"),
    {ok, Item} = parse(Text),
    ?assertEqual(<<"Rings">>, maps:get(<<"item_class">>, Item)),
    [Life, Fire, Physical, Implicit, Future] = maps:get(<<"modifiers">>, Item),
    ?assertMatch(#{<<"decorator">> := #{<<"name">> := <<"Healthy">>,
        <<"affix">> := <<"Prefix">>, <<"tier">> := 2, <<"tags">> := [<<"Life">>]},
        <<"magnitudes">> := [#{<<"value">> := 82, <<"min">> := 70, <<"max">> := 89}],
        <<"min">> := 82, <<"supported">> := true}, Life),
    ?assertMatch(#{<<"decorator">> := #{<<"tags">> := [<<"Elemental">>, <<"Fire">>, <<"Resistance">>]}}, Fire),
    ?assertMatch(#{<<"magnitudes">> := [#{<<"value">> := 10, <<"min">> := 8, <<"max">> := 12},
        #{<<"value">> := 20, <<"min">> := 18, <<"max">> := 22}], <<"min">> := 15.0}, Physical),
    ?assertNot(maps:is_key(<<"decorator">>, Implicit)),
    ?assertMatch(#{<<"section">> := <<"unknown">>, <<"supported">> := false,
        <<"decorator">> := #{<<"name">> := <<"Unmapped">>, <<"tier">> := 4,
                             <<"tags">> := [<<"FutureTag">>], <<"raw">> := _}}, Future).

advanced_metadata_scope_test() ->
    {ok, Item} = parse(equipment(<<"{ Prefix Modifier \"Hybrid\" (Tier: 2) }\n",
        "+82 to maximum Life\nAdds 10 to 20 Physical Damage\n",
        "{ Prefix Modifier \"Hybrid\" (Tier: 2) }\n+20 to maximum Life\n",
        "--------\n+10 to maximum Life\n",
        "{ Future Modifier }\n+15 to maximum Life\n",
        "--------\n+15 to maximum Life (implicit)">>)),
    [A, B, C, Plain, Unknown, Implicit] = maps:get(<<"modifiers">>, Item),
    Group = fun(Row) -> maps:get(<<"group">>, maps:get(<<"decorator">>, Row)) end,
    ?assertEqual(Group(A), Group(B)),
    ?assertNotEqual(Group(A), Group(C)),
    ?assertNot(maps:is_key(<<"decorator">>, Plain)),
    ?assertNot(maps:is_key(<<"decorator">>, Implicit)),
    ?assertMatch(#{<<"supported">> := false, <<"decorator">> := #{<<"raw">> := _}}, Unknown),
    ?assertMatch([#{<<"min">> := 10, <<"max">> := 10, <<"value">> := 10}],
                 maps:get(<<"magnitudes">>, Plain)).

invalid_roll_metadata_stays_untrusted_test() ->
    {ok, Item} = parse(equipment(<<"+82(90-70) to maximum Life\n",
        "{ Prefix Modifier (Tier: 0) }\n+82(1-3) to maximum Life\n",
        "999999999999999999999999 to maximum Life">>)),
    [Reversed, Outside, Huge] = maps:get(<<"modifiers">>, Item),
    ?assertMatch([#{<<"min">> := 70, <<"max">> := 90}], maps:get(<<"magnitudes">>, Reversed)),
    lists:foreach(fun(Row) -> ?assertEqual([], maps:get(<<"magnitudes">>, Row)) end,
                  [Outside, Huge]),
    ?assertNot(maps:is_key(<<"tier">>, maps:get(<<"decorator">>, Outside))),
    ?assertMatch(#{<<"supported">> := false}, Huge).

roll_metadata_follows_stat_captures_test() ->
    {ok, Item} = parse(equipment(<<"12.5(10-15)% reduced Movement Speed\n",
                                  "+82(82) to maximum Life">>)),
    [Reduced, Fixed] = maps:get(<<"modifiers">>, Item),
    ?assertMatch(#{<<"min">> := -12.5, <<"magnitudes">> := [
        #{<<"value">> := -12.5, <<"min">> := -15, <<"max">> := -10}]}, Reduced),
    ?assertMatch(#{<<"magnitudes">> := [#{<<"value">> := 82, <<"min">> := 82, <<"max">> := 82}]}, Fixed),
    {ok, Catalog} = retinue_poe2_trade_item:catalog(#{<<"result">> => [
        #{<<"id">> => <<"explicit">>, <<"entries">> => [
            #{<<"id">> => <<"explicit.test">>, <<"text">> => <<"Gain # Life every 5 seconds">>}]}]}),
    {ok, Literal} = retinue_poe2_trade_item:parse(
        equipment(<<"Gain 12(10-15) Life every 5 seconds">>), Catalog),
    ?assertMatch([#{<<"supported">> := true, <<"magnitudes">> := [
        #{<<"value">> := 12, <<"min">> := 10, <<"max">> := 15}]}], maps:get(<<"modifiers">>, Literal)).

signed_and_invalid_text_test() ->
    {ok, Item} = parse(equipment(<<"-15 to maximum Life\n12.5% increased Movement Speed">>)),
    [A, B] = maps:get(<<"modifiers">>, Item),
    ?assertEqual(-15, maps:get(<<"min">>, A)),
    ?assertEqual(12.5, maps:get(<<"min">>, B)),
    lists:foreach(fun(Text) -> ?assertMatch({error, invalid_item_text}, parse(Text)) end,
                  [<<>>, <<"password">>, <<255>>, binary:copy(<<"a">>, 32769),
                   <<"Rarity: Rare\nName\nBase\n--------\nItem Level: 999">>]),
    {ok, Huge} = parse(equipment(<<"999999999999999999999999 to maximum Life">>)),
    ?assertMatch([#{<<"supported">> := false}], maps:get(<<"modifiers">>, Huge)).

unknown_markers_and_multi_values_test() ->
    {ok, Item} = parse(equipment(<<"Alternating every 5 seconds:\nTake 10% more Damage from Hits\n",
        "Take 20% more Damage over time\n--------\n{ Future Modifier }\n+82 to maximum Life\n",
        "--------\nAdds 20 to 10 Physical Damage\n--------\n{ Augment Modifier }\n+15 to maximum Life">>)),
    [Multi, Unknown, Reversed, Augment] = maps:get(<<"modifiers">>, Item),
    ?assertMatch(#{<<"supported">> := true, <<"min">> := 15.0,
                   <<"id">> := <<"explicit.stat_258955603">>}, Multi),
    ?assertMatch(#{<<"supported">> := false, <<"id">> := null}, Unknown),
    ?assertMatch(#{<<"supported">> := false, <<"resolution">> := <<"unsupported_filter">>}, Reversed),
    ?assertEqual(<<"unknown">>, maps:get(<<"section">>, Unknown)),
    ?assertMatch(#{<<"supported">> := true, <<"id">> := <<"rune.stat_3299347043">>}, Augment).

query_identity_test() ->
    {ok, Rare} = parse(item()),
    {ok, #{<<"query">> := Query}} = build(search_request(), Rare, leagues()),
    ?assertNot(maps:is_key(<<"name">>, Query)),
    ?assertEqual(<<"Felt Cap">>, maps:get(<<"type">>, Query)),
    ?assertMatch(#{<<"stats">> := [#{<<"filters">> := []}],
                   <<"filters">> := #{<<"type_filters">> := #{<<"filters">> := #{
                       <<"rarity">> := #{<<"option">> := <<"rare">>}}},
                       <<"misc_filters">> := #{<<"filters">> := #{
                       <<"corrupted">> := #{<<"option">> := <<"true">>}}}}}, Query),
    {ok, Unique} = parse(unique()),
    {ok, #{<<"query">> := UniqueQuery}} = build(
        (search_request())#{<<"base">> => false, <<"status">> => <<"any">>}, Unique, leagues()),
    ?assertMatch(#{<<"name">> := <<"Goldrim">>, <<"type">> := <<"Felt Cap">>,
                   <<"status">> := #{<<"option">> := <<"any">>}}, UniqueQuery),
    Unidentified = Unique#{<<"name">> => <<>>, <<"unidentified">> => true},
    ?assertMatch({error, #{<<"reason">> := <<"unidentified_unique">>}},
        build(search_request(), Unidentified, leagues())),
    {ok, #{<<"query">> := Broad}} = build(
        (search_request())#{<<"base">> => false}, Rare, leagues()),
    ?assertNot(maps:is_key(<<"type">>, Broad)).

filter_validation_test() ->
    {ok, Item} = parse(item()),
    Filter = #{<<"id">> => <<"explicit.stat_3299347043">>, <<"min">> => -5, <<"max">> => 100.5},
    Build = fun(F) -> build(
        (search_request())#{<<"filters">> => F}, Item, leagues()) end,
    ?assertMatch({ok, #{<<"query">> := #{<<"stats">> := [#{<<"filters">> := [#{
        <<"value">> := #{<<"min">> := -5, <<"max">> := 100.5}}]}]}}}, Build([Filter])),
    lists:foreach(fun(F) -> ?assertMatch({error, _}, Build([F])) end,
        [Filter#{<<"min">> => <<"5">>}, Filter#{<<"min">> => true},
         Filter#{<<"min">> => 1.0e200}, Filter#{<<"max">> => -10},
         Filter#{<<"id">> => <<"explicit.stat_53045048">>},
         Filter#{<<"id">> => <<"explicit.stat_3917489142">>},
         Filter#{<<"id">> => <<"invented">>}, Filter#{<<"disabled">> => true}]),
    ?assertMatch({error, #{<<"reason">> := <<"duplicate_filter">>}}, Build([Filter, Filter])),
    ?assertMatch({error, _}, Build(lists:duplicate(33, Filter))),
    ?assertMatch({error, _}, Build(<<"bad">>)),
    ?assertMatch({error, #{<<"reason">> := <<"invalid_league">>}},
        build((search_request())#{<<"league">> => <<"old league">>}, Item, leagues())).

metadata_backed_search_options_test() ->
    {ok, Options} = retinue_poe2_trade_query:options(filter_fixture()),
    ?assertEqual([<<"currency">>, <<"indexed">>, <<"rarity">>, <<"status">>], lists:sort(maps:keys(Options))),
    {ok, Item} = parse(item()),
    {ok, #{<<"query">> := Default}} = build(search_request(), Item, leagues()),
    ?assertEqual(#{<<"option">> => <<"securable">>}, maps:get(<<"status">>, Default)),
    lists:foreach(fun(Status) ->
        {ok, #{<<"query">> := Query}} = build((search_request())#{<<"status">> => Status}, Item, leagues()),
        ?assertEqual(#{<<"option">> => Status}, maps:get(<<"status">>, Query))
    end, [maps:get(<<"id">>, Choice) || Choice <- maps:get(<<"status">>, Options)]),
    Request = (search_request())#{<<"rarity">> => null, <<"currency">> => <<"divine">>,
        <<"indexed">> => <<"1day">>, <<"sort">> => <<"desc">>,
        <<"item_level">> => #{<<"min">> => 75, <<"max">> => 85}},
    {ok, #{<<"sort">> := #{<<"price">> := <<"desc">>}, <<"query">> := Query}} = build(Request, Item, leagues()),
    #{<<"type_filters">> := #{<<"filters">> := Types},
      <<"trade_filters">> := #{<<"filters">> := Trade},
      <<"misc_filters">> := #{<<"filters">> := Misc}} = maps:get(<<"filters">>, Query),
    ?assertNot(maps:is_key(<<"rarity">>, Types)),
    ?assertEqual(#{<<"min">> => 0, <<"option">> => <<"divine">>}, maps:get(<<"price">>, Trade)),
    ?assertEqual(#{<<"option">> => <<"true">>}, maps:get(<<"collapse">>, Trade)),
    ?assertEqual(#{<<"option">> => <<"1day">>}, maps:get(<<"indexed">>, Trade)),
    ?assertEqual(#{<<"min">> => 75, <<"max">> => 85}, maps:get(<<"ilvl">>, Misc)),
    lists:foreach(fun({Key, Value}) ->
        ?assertMatch({error, _}, build((search_request())#{Key => Value}, Item, leagues()))
    end, [{<<"status">>, <<"made-up">>}, {<<"rarity">>, <<"future">>},
          {<<"currency">>, <<"dollars">>}, {<<"indexed">>, <<"tomorrow">>},
          {<<"sort">>, <<"wrong">>}, {<<"item_level">>, #{<<"min">> => 101}},
          {<<"item_level">>, #{<<"min">> => 12.5}},
          {<<"item_level">>, #{<<"min">> => 80, <<"max">> => 70}}]),
    {ok, Unique} = parse(unique()),
    ?assertMatch({error, _}, build(Request, Unique, leagues())),
    ?assertEqual({error, invalid_filter_metadata}, retinue_poe2_trade_query:options(#{<<"result">> => []})).

listing_metadata_test() ->
    {ok, 200, _, Bytes} = http(get, <<"/api/trade2/fetch/1,2,3,4,5,6,7,8,9,10?query=test">>, [], <<>>),
    #{<<"result">> := [Row]} = json:decode(Bytes),
    Item = (maps:get(<<"item">>, Row))#{<<"properties">> => [#{<<"name">> => <<"Quality">>,
        <<"values">> => [[<<"+20%">>, 0]]}]},
    {ok, [Listing]} = retinue_poe2_trade_query:listings(#{<<"result">> => [Row#{<<"item">> => Item}]}),
    {ok, Time} = retinue_time:parse(<<"2025-02-17T09:15:53Z">>),
    ?assertEqual(Time div 1000, maps:get(<<"indexed_at">>, Listing)),
    ?assertEqual(20, maps:get(<<"quality">>, Listing)).

listing_item_preview_test() ->
    Item = #{<<"name">> => <<"Beast Circle">>, <<"typeLine">> => <<"Gold Ring">>,
        <<"frameType">> => 2, <<"ilvl">> => 84, <<"identified">> => true,
        <<"corrupted">> => true, <<"explicitMods">> => [<<"+90 to maximum Life">>],
        <<"implicitMods">> => [<<"10% increased Rarity of Items found">>],
        <<"requirements">> => [#{<<"name">> => <<"Level">>, <<"values">> => [[<<"65">>, 0]]}],
        <<"properties">> => [#{<<"name">> => <<"Quality">>, <<"values">> => [[<<"+20%">>, 0]]}]},
    Row = #{<<"id">> => <<"listing">>, <<"item">> => Item,
        <<"listing">> => #{<<"type">> => <<"secure">>,
            <<"price">> => #{<<"amount">> => 40, <<"currency">> => <<"exalted">>},
            <<"account">> => #{<<"name">> => <<"Example">>}}},
    {ok, [Listing]} = retinue_poe2_trade_query:listings(#{<<"result">> => [Row]}),
    ?assertEqual(<<"secure">>, maps:get(<<"type">>, Listing)),
    ?assertEqual(65, maps:get(<<"required_level">>, Listing)),
    Preview = maps:get(<<"item">>, Listing),
    ?assertEqual(<<"Rare">>, maps:get(<<"rarity">>, Preview)),
    ?assertEqual([#{<<"text">> => <<"Quality: +20%">>}, #{<<"text">> => <<"Requires Level: 65">>}],
                 maps:get(<<"properties">>, Preview)),
    ?assertMatch([#{<<"section">> := <<"implicit">>}, #{<<"section">> := <<"explicit">>,
        <<"text">> := <<"+90 to maximum Life">>, <<"supported">> := false}], maps:get(<<"modifiers">>, Preview)),
    ?assertMatch([#{<<"text">> := <<"Corrupted">>}], maps:get(<<"conditions">>, Preview)),
    BadFields = Item#{<<"requirements">> => null, <<"properties">> => null, <<"explicitMods">> => null},
    ?assertMatch({ok, [_]}, retinue_poe2_trade_query:listings(#{<<"result">> => [Row#{<<"item">> => BadFields}]})).

listing_property_formats_test() ->
    Item = #{<<"properties">> => [
        #{<<"name">> => <<"{0} to {1} Damage">>, <<"displayMode">> => 3,
          <<"values">> => [[<<"10">>, 0], [<<"20">>, 0]]},
        #{<<"name">> => <<"Armour">>, <<"displayMode">> => 1, <<"values">> => [[<<"100">>, 0]]}]},
    ?assertMatch(#{<<"properties">> := [#{<<"text">> := <<"10 to 20 Damage">>},
                                        #{<<"text">> := <<"100 Armour">>}]},
                 retinue_poe2_trade_item:from_listing(Item)),
    ?assertMatch(#{<<"rarity">> := <<"Unknown">>, <<"item_level">> := null},
                 retinue_poe2_trade_item:from_listing(#{<<"frameType">> => 99, <<"ilvl">> => -1})).

returned_listing_count_test() ->
    ?assertMatch({ok, #{<<"total">> := 5000, <<"returned">> := 2}, [<<"one">>, <<"two">>]},
        retinue_poe2_trade_query:search_result(#{<<"id">> => <<"search">>, <<"total">> => 5000,
                                              <<"result">> => [<<"one">>, <<"two">>]}, <<"Standard">>)).

item_condition_filters_test() ->
    Plain = binary:replace(equipment(<<"+82 to maximum Life">>), <<"\n--------\nCorrupted">>, <<>>),
    {ok, Modifiable} = parse(Plain),
    ?assertMatch([#{<<"id">> := <<"modifiable">>, <<"text">> := <<"Modifiable">>}],
                 maps:get(<<"conditions">>, Modifiable)),
    {ok, #{<<"query">> := Query}} = build(search_request(), Modifiable, leagues()),
    Misc = maps:get(<<"filters">>, maps:get(<<"misc_filters">>, maps:get(<<"filters">>, Query))),
    ?assertEqual(#{<<"corrupted">> => #{<<"option">> => <<"false">>},
                   <<"twice_corrupted">> => #{<<"option">> => <<"false">>},
                   <<"sanctified">> => #{<<"option">> => <<"false">>},
                   <<"mirrored">> => #{<<"option">> => <<"false">>}}, Misc),
    {ok, #{<<"query">> := Any}} = build((search_request())#{<<"conditions">> => []}, Modifiable, leagues()),
    ?assertEqual(#{}, maps:get(<<"filters">>, maps:get(<<"misc_filters">>, maps:get(<<"filters">>, Any)))),
    lists:foreach(fun(State) ->
        {ok, Item} = parse(<<Plain/binary, "\n--------\n", State/binary>>),
        ?assertNot(lists:any(fun(C) -> maps:get(<<"id">>, C) =:= <<"modifiable">> end,
                            maps:get(<<"conditions">>, Item))),
        ?assertEqual(1, length(maps:get(<<"modifiers">>, Item)))
    end, [<<"Mirrored">>, <<"Unmodifiable">>, <<"Sanctified">>, <<"Corrupted">>, <<"Twice Corrupted">>]),
    ?assertMatch({error, _}, build((search_request())#{<<"conditions">> => [<<"mirrored">>]}, Modifiable, leagues())),
    ?assertMatch({error, _}, build((search_request())#{<<"conditions">> => [<<"modifiable">>, <<"modifiable">>]}, Modifiable, leagues())).

freshness_test() ->
    ?assertEqual(#{store => true, expires => 1030}, retinue_poe2_trade_http:freshness(
        #{<<"cache-control">> => <<"public, max-age=60">>, <<"age">> => <<"30">>}, 1000)),
    ?assertEqual(#{store => false, expires => 1000}, retinue_poe2_trade_http:freshness(
        #{<<"cache-control">> => <<"no-store, max-age=999">>}, 1000)),
    ?assertEqual(#{store => true, expires => 1000}, retinue_poe2_trade_http:freshness(
        #{<<"cache-control">> => <<"no-cache, max-age=999">>}, 1000)),
    Headers = retinue_poe2_trade_http:headers([{<<"ETag">>, <<"tag">>},
        {<<"Set-Cookie">>, <<"must-not-be-retained">>}, {<<"Last-Modified">>, <<"date">>}]),
    ?assertNot(maps:is_key(<<"set-cookie">>, Headers)),
    ?assertEqual([{<<"if-none-match">>, <<"tag">>}, {<<"if-modified-since">>, <<"date">>}],
                  retinue_poe2_trade_http:validators(Headers)).

rate_limits_test() ->
    Headers = #{<<"x-rate-limit-policy">> => <<"trade-search">>,
                <<"x-rate-limit-rules">> => <<"Ip, Account">>,
                <<"x-rate-limit-ip">> => <<"5:10:60,15:60:120,600:21600:3600">>,
                <<"x-rate-limit-ip-state">> => <<"5:10:0,15:60:0,600:21600:0">>,
                <<"x-rate-limit-account">> => <<"20:60:120">>,
                <<"x-rate-limit-account-state">> => <<"2:60:0">>},
    Limits = retinue_poe2_trade_http:observe(search, 200, Headers, #{}, 1000),
    ?assertEqual(21600, retinue_poe2_trade_http:cooldown(search, Limits, 1000)),
    ?assertEqual(0, retinue_poe2_trade_http:cooldown(fetch, Limits, 1000)),
    ?assertEqual(0, retinue_poe2_trade_http:cooldown(search, Limits, 22600)),
    Limited = retinue_poe2_trade_http:observe(fetch, 429, #{<<"retry-after">> => <<"12">>}, #{}, 1000),
    ?assertEqual(12, retinue_poe2_trade_http:cooldown(search, Limited, 1000)),
    ?assertEqual(7, retinue_poe2_trade_http:cooldown(fetch, Limited, 1005)),
    ?assertEqual(60, retinue_poe2_trade_http:cooldown(fetch,
        retinue_poe2_trade_http:observe(fetch, 429, #{}, #{}, 1000), 1000)),
    ?assertEqual(12, retinue_poe2_trade_http:retry_after(<<"Thu, 01 Jan 1970 00:16:52 GMT">>, 1000)),
    ?assertEqual(0, retinue_poe2_trade_http:retry_after(<<"nonsense">>, 1000)).

automatic_search_rate_budget_test() ->
    ?assert(retinue_poe2_trade_http:automatic_allowed(#{}, 1000)),
    Headers = #{<<"x-rate-limit-policy">> => <<"shared">>, <<"x-rate-limit-rules">> => <<"Ip">>,
        <<"x-rate-limit-ip">> => <<"4:10:60">>, <<"x-rate-limit-ip-state">> => <<"2:10:0">>},
    Two = retinue_poe2_trade_http:observe(search, 200, Headers, #{}, 1000),
    ?assert(retinue_poe2_trade_http:automatic_allowed(Two, 1000)),
    Three = retinue_poe2_trade_http:spend(search, Two, 1000),
    ?assertEqual(0, retinue_poe2_trade_http:cooldown(search, Three, 1000)),
    ?assertNot(retinue_poe2_trade_http:automatic_allowed(Three, 1000)),
    ?assert(retinue_poe2_trade_http:automatic_allowed(Three, 1010)),
    Shared = retinue_poe2_trade_http:observe(fetch, 200,
        Headers#{<<"x-rate-limit-ip-state">> => <<"3:10:0">>}, Two, 1000),
    ?assertNot(retinue_poe2_trade_http:automatic_allowed(Shared, 1000)),
    Blocked = retinue_poe2_trade_http:observe(fetch, 429, #{<<"retry-after">> => <<"12">>}, #{}, 1000),
    ?assertNot(retinue_poe2_trade_http:automatic_allowed(Blocked, 1000)),
    ?assert(retinue_poe2_trade_http:automatic_allowed(Blocked, 1012)).

rate_budget_projects_shared_windows_and_remaining_cooldown_test() ->
    Headers = #{<<"x-rate-limit-policy">> => <<"shared">>, <<"x-rate-limit-rules">> => <<"ip">>,
        <<"x-rate-limit-ip">> => <<"10:10:60,20:60:120">>,
        <<"x-rate-limit-ip-state">> => <<"5:10:0,8:60:0">>},
    Search = retinue_poe2_trade_http:observe(search, 200, Headers, #{}, 1000),
    Shared = retinue_poe2_trade_http:observe(fetch, 200, Headers, Search, 1000),
    ?assertMatch(#{<<"retry_after_ms">> := 0, <<"windows">> := [
        #{<<"usage">> := 0.5, <<"expires_in_ms">> := 10000},
        #{<<"usage">> := 0.4, <<"expires_in_ms">> := 60000}]},
        retinue_poe2_trade_http:budget(Shared, 1000)),
    ?assertMatch(#{<<"windows">> := [#{<<"usage">> := 0.4, <<"expires_in_ms">> := 50000}]},
        retinue_poe2_trade_http:budget(Shared, 1010)),
    Blocked = retinue_poe2_trade_http:observe(fetch, 429,
        #{<<"retry-after">> => <<"12">>}, #{}, 1000),
    ?assertMatch(#{<<"retry_after_ms">> := 7000, <<"windows">> := []},
        retinue_poe2_trade_http:budget(Blocked, 1005)),
    ?assertMatch(#{<<"retry_after_ms">> := 0, <<"windows">> := []},
        retinue_poe2_trade_http:budget(#{}, 1000)).

league_default_preserves_api_order_test() ->
    Http = fun(get, <<"/api/trade2/data/leagues">>, _, _) ->
        Body = json:encode(#{<<"result">> => [
            #{<<"id">> => <<"Z New League">>, <<"text">> => <<"Z New League">>},
            #{<<"id">> => <<"A Old League">>, <<"text">> => <<"A Old League">>}]}),
        {ok, 200, #{}, iolist_to_binary(Body)}
    end,
    {ok, Server} = retinue_poe2_trade:start_link(#{name => undefined, http => Http}),
    try
        ?assertMatch({ok, #{<<"league">> := <<"Z New League">>}},
                     retinue_poe2_trade:request(Server, #{<<"op">> => <<"trade_leagues">>}))
    after gen_server:stop(Server) end.

service_cache_test() ->
    with_service(fun(Server, Calls, Clock) ->
        Read = fun(Request) ->
            {ok, Data} = retinue_poe2_trade:request(Server, Request),
            ?assertMatch(#{<<"observed_at_ms">> := _, <<"windows">> := _, <<"retry_after_ms">> := _},
                maps:get(<<"rate_budget">>, Data)),
            maps:remove(<<"rate_budget">>, Data)
        end,
        First = Read(item_request()),
        ?assertMatch(#{<<"league">> := <<"Standard">>, <<"leagues">> := [#{<<"id">> := <<"Standard">>}]}, First),
        ?assert(maps:is_key(<<"search_options">>, First)),
        ?assertEqual(3, atomics:get(Calls, 1)),
        ?assertEqual(First, Read(item_request())),
        ?assertEqual(3, atomics:get(Calls, 1)),
        atomics:add(Clock, 1, 61),
        ?assertEqual(First, Read(item_request())),
        ?assertEqual(6, atomics:get(Calls, 1)),
        Result = Read(search_request()),
        ?assertMatch(#{<<"search_id">> := <<"search-id">>, <<"total">> := 25,
                       <<"listings">> := [], <<"next_offset">> := 0}, Result),
        ?assertEqual(7, atomics:get(Calls, 1)),
        ?assertEqual(Result, Read(search_request())),
        ?assertEqual(7, atomics:get(Calls, 1)),
        Page = Read(fetch_request(0)),
        ?assertMatch(#{<<"offset">> := 0, <<"next_offset">> := 10,
                       <<"listings">> := [#{<<"amount">> := 20, <<"currency">> := <<"exalted">>,
                       <<"online">> := true}]}, Page),
        ?assertEqual(Page, Read(fetch_request(0))),
        ?assertEqual(8, atomics:get(Calls, 1))
    end, fun http/4).

reload_does_not_restore_old_compiled_stats_from_active_worker_test() ->
    Parent = self(),
    Http = fun(Method, Path, Headers, Body) ->
        case Path of
            <<"/api/trade2/data/stats">> -> Parent ! {stats, self()}, receive release -> ok end;
            _ -> ok
        end,
        http(Method, Path, Headers, Body)
    end,
    with_service(fun(Server, _, _) ->
        spawn(fun() -> Parent ! {item, retinue_poe2_trade:request(Server, item_request())} end),
        Worker = receive {stats, Pid} -> Pid after 1000 -> error(no_worker) end,
        ok = sys:suspend(Server),
        ok = sys:change_code(Server, retinue_poe2_trade, old, hot_update),
        ok = sys:resume(Server),
        Worker ! release,
        receive {item, Reply} -> ?assertMatch({ok, _}, Reply) after 1000 -> error(no_reply) end,
        Cache = maps:get(cache, sys:get_state(Server)),
        ?assertNot(maps:is_key(stats, Cache)),
        ?assert(maps:is_key(leagues, Cache))
    end, Http).

no_search_on_parse_or_invalid_filter_test() ->
    with_service(fun(Server, Calls, _) ->
        ?assertMatch({error, _}, retinue_poe2_trade:request(Server, #{<<"op">> => <<"trade_item">>, <<"text">> => <<"bad">>})),
        ?assertEqual(0, atomics:get(Calls, 1)),
        ?assertMatch({ok, _}, retinue_poe2_trade:request(Server, item_request())),
        ?assertMatch({error, _}, retinue_poe2_trade:request(Server,
            (search_request())#{<<"filters">> => [#{<<"id">> => <<"invented">>, <<"min">> => 5}]})),
        ?assertEqual(3, atomics:get(Calls, 1))
    end, fun http/4).

pending_fetch_retry_test() ->
    Attempts = atomics:new(1, []),
    HTTP = fun(Method, Path, Headers, Body) ->
        case Path of
            <<"/api/trade2/fetch/", _/binary>> ->
                case atomics:add_get(Attempts, 1, 1) of
                    1 -> {ok, 429, #{<<"retry-after">> => <<"12">>}, <<>>};
                    _ -> http(Method, Path, Headers, Body)
                end;
            _ -> http(Method, Path, Headers, Body)
        end
    end,
    with_service(fun(Server, Calls, Clock) ->
        ?assertMatch({ok, #{<<"listings">> := []}},
            retinue_poe2_trade:request(Server, search_request())),
        ?assertMatch({error, #{<<"reason">> := <<"rate_limited">>, <<"retry_after_seconds">> := 12}},
            retinue_poe2_trade:request(Server, fetch_request(0))),
        ?assertEqual(5, atomics:get(Calls, 1)),
        ?assertMatch({error, #{<<"retry_after_seconds">> := 12}},
            retinue_poe2_trade:request(Server, fetch_request(0))),
        ?assertEqual(5, atomics:get(Calls, 1)),
        atomics:add(Clock, 1, 12),
        ?assertMatch({ok, _}, retinue_poe2_trade:request(Server, fetch_request(0))),
        ?assertEqual(6, atomics:get(Calls, 1))
    end, HTTP).

pagination_refresh_and_retention_test() ->
    with_service(fun(Server, Calls, Clock) ->
        ?assertMatch({error, #{<<"reason">> := <<"search_expired">>}},
            retinue_poe2_trade:request(Server, fetch_request(0))),
        ?assertEqual(0, atomics:get(Calls, 1)),
        {ok, _} = retinue_poe2_trade:request(Server, search_request()),
        lists:foreach(fun({Offset, Next, Id}) ->
            ?assertMatch({ok, #{<<"offset">> := Offset, <<"next_offset">> := Next,
                               <<"listings">> := [#{<<"id">> := Id}]}},
                retinue_poe2_trade:request(Server, fetch_request(Offset)))
        end, [{0, 10, <<"1">>}, {10, 20, <<"11">>}, {20, null, <<"21">>}]),
        ?assertEqual(7, atomics:get(Calls, 1)),
        lists:foreach(fun(Offset) ->
            ?assertMatch({error, _}, retinue_poe2_trade:request(Server, fetch_request(Offset)))
        end, [-1, 1, 30, 10000, <<"0">>]),
        ?assertEqual(7, atomics:get(Calls, 1)),
        {ok, _} = retinue_poe2_trade:request(Server, (search_request())#{<<"refresh">> => true}),
        {ok, _} = retinue_poe2_trade:request(Server, fetch_request(0)),
        ?assertEqual(9, atomics:get(Calls, 1)),
        atomics:add(Clock, 1, 601),
        ?assertMatch({ok, #{<<"offset">> := 0}},
            retinue_poe2_trade:request(Server, fetch_request(0))),
        ?assertEqual(9, atomics:get(Calls, 1))
    end, fun http/4).

cache_restore_never_fetches_and_keeps_completed_pages_test() ->
    with_service(fun(Server, Calls, Clock) ->
        Restore = (search_request())#{<<"op">> => <<"trade_restore">>},
        ?assertMatch({ok, #{<<"results">> := null}}, retinue_poe2_trade:request(Server, Restore)),
        ?assertEqual(0, atomics:get(Calls, 1)),
        {ok, _} = retinue_poe2_trade:request(Server, search_request()),
        {ok, _} = retinue_poe2_trade:request(Server, fetch_request(0)),
        {ok, _} = retinue_poe2_trade:request(Server, fetch_request(10)),
        atomics:add(Clock, 1, 86400),
        {ok, #{<<"results">> := Result}} = retinue_poe2_trade:request(Server, Restore),
        ?assertMatch(#{<<"next_offset">> := 20, <<"listings">> := [#{<<"id">> := <<"1">>}, #{<<"id">> := <<"11">>}]}, Result),
        SameQuery = Restore#{<<"text">> => binary:replace(item(), <<"Doom Shelter">>, <<"Other Name">>)},
        ?assertMatch({ok, #{<<"results">> := Result}}, retinue_poe2_trade:request(Server, SameQuery)),
        ?assertMatch({ok, #{<<"results">> := null}},
            retinue_poe2_trade:request(Server, Restore#{<<"sort">> => <<"desc">>})),
        ?assertEqual(6, atomics:get(Calls, 1)),
        {ok, _} = retinue_poe2_trade:request(Server, (search_request())#{<<"refresh">> => true}),
        ?assertMatch({ok, #{<<"results">> := #{<<"listings">> := [], <<"next_offset">> := 0}}},
            retinue_poe2_trade:request(Server, Restore)),
        ?assertEqual(10, atomics:get(Calls, 1))
    end, fun http/4).

cache_evicts_oldest_completion_not_last_access_test() ->
    with_service(fun(Server, _, _) ->
        Request = fun(N) -> (search_request())#{<<"item_level">> => #{<<"min">> => N}} end,
        Restore = fun(N) -> retinue_poe2_trade:request(Server,
            (Request(N))#{<<"op">> => <<"trade_restore">>}) end,
        {ok, _} = retinue_poe2_trade:request(Server, Request(1)),
        {ok, _} = retinue_poe2_trade:request(Server, fetch_request(0)),
        lists:foreach(fun(N) -> {ok, _} = retinue_poe2_trade:request(Server, Request(N)) end, lists:seq(2, 10)),
        ?assertMatch({ok, #{<<"results">> := #{<<"listings">> := [_]}}}, Restore(1)),
        {ok, _} = retinue_poe2_trade:request(Server, Request(11)),
        ?assertMatch({ok, #{<<"results">> := null}}, Restore(1)),
        ?assertMatch({ok, #{<<"results">> := #{}}}, Restore(2)),
        {ok, _} = retinue_poe2_trade:request(Server, (Request(2))#{<<"refresh">> => true}),
        {ok, _} = retinue_poe2_trade:request(Server, Request(12)),
        ?assertMatch({ok, #{<<"results">> := #{}}}, Restore(2)),
        ?assertMatch({ok, #{<<"results">> := null}}, Restore(3)),
        ?assertEqual(10, map_size(maps:get(results, sys:get_state(Server))))
    end, fun http/4).

cache_restore_does_not_wait_for_an_upstream_request_test() ->
    Parent = self(),
    HTTP = fun(Method, Path, Headers, Body) ->
        case Method =:= post andalso maps:get(<<"sort">>, json:decode(Body), #{}) =:= #{<<"price">> => <<"desc">>} of
            true -> Parent ! {searching, self()}, receive finish -> ok after 2000 -> error(not_released) end;
            false -> ok
        end,
        http(Method, Path, Headers, Body)
    end,
    with_service(fun(Server, Calls, _) ->
        {ok, _} = retinue_poe2_trade:request(Server, search_request()),
        spawn(fun() -> Parent ! {search_done, retinue_poe2_trade:request(Server,
            (search_request())#{<<"sort">> => <<"desc">>})} end),
        Worker = receive {searching, Pid} -> Pid after 1000 -> error(no_worker) end,
        try
            Restore = (search_request())#{<<"op">> => <<"trade_restore">>},
            ?assertMatch({ok, #{<<"results">> := #{}}}, gen_server:call(Server, {request, Restore}, 1000)),
            ?assertEqual(5, atomics:get(Calls, 1))
        after Worker ! finish end,
        receive {search_done, Reply} -> ?assertMatch({ok, _}, Reply) after 1000 -> error(no_reply) end
    end, HTTP).

cache_keeps_first_page_when_pagination_reaches_its_bound_test() ->
    HTTP = fun
        (post, _, _, _) -> {ok, 200, #{}, iolist_to_binary(json:encode(#{
            <<"id">> => <<"search-id">>, <<"total">> => 150,
            <<"result">> => [integer_to_binary(N) || N <- lists:seq(1, 150)]}))};
        (Method, Path, Headers, Body) -> http(Method, Path, Headers, Body)
    end,
    with_service(fun(Server, Calls, _) ->
        {ok, _} = retinue_poe2_trade:request(Server, search_request()),
        lists:foreach(fun(Offset) -> {ok, _} = retinue_poe2_trade:request(Server, fetch_request(Offset)) end,
            lists:seq(0, 80, 10)),
        ?assertEqual(13, atomics:get(Calls, 1)),
        {ok, #{<<"results">> := Restored}} = retinue_poe2_trade:request(Server,
            (search_request())#{<<"op">> => <<"trade_restore">>}),
        ?assertMatch(#{<<"listings">> := [#{<<"id">> := <<"1">>}], <<"next_offset">> := 10}, Restored),
        [#{value := #{pages := Pages}}] = maps:values(maps:get(results, sys:get_state(Server))),
        ?assertEqual(8, map_size(Pages)),
        ?assertEqual(13, atomics:get(Calls, 1))
    end, HTTP).

cache_restore_succeeds_during_network_cooldown_test() ->
    HTTP = fun(Method, Path, Headers, Body) ->
        case Method =:= post andalso maps:get(<<"sort">>, json:decode(Body), #{}) =:= #{<<"price">> => <<"desc">>} of
            true -> {ok, 429, #{<<"retry-after">> => <<"60">>}, <<>>};
            false -> http(Method, Path, Headers, Body)
        end
    end,
    with_service(fun(Server, Calls, _) ->
        {ok, _} = retinue_poe2_trade:request(Server, search_request()),
        {ok, _} = retinue_poe2_trade:request(Server, fetch_request(0)),
        ?assertMatch({error, #{<<"reason">> := <<"rate_limited">>,
                               <<"rate_budget">> := #{<<"retry_after_ms">> := 60000}}},
            retinue_poe2_trade:request(Server, (search_request())#{<<"sort">> => <<"desc">>})),
        ?assertMatch({ok, #{<<"auto_search_allowed">> := false,
                           <<"results">> := #{<<"listings">> := [_]}}},
            retinue_poe2_trade:request(Server, (search_request())#{<<"op">> => <<"trade_restore">>})),
        ?assertEqual(6, atomics:get(Calls, 1))
    end, HTTP).

deleted_page_still_advances_test() ->
    Http = fun(Method, Path, Headers, Body) ->
        case Path of
            <<"/api/trade2/fetch/", _/binary>> -> {ok, 200, #{}, <<"{\"result\":[null]}">>};
            _ -> http(Method, Path, Headers, Body)
        end
    end,
    with_service(fun(Server, _, _) ->
        {ok, _} = retinue_poe2_trade:request(Server, search_request()),
        ?assertMatch({ok, #{<<"next_offset">> := 10, <<"listings">> := []}},
            retinue_poe2_trade:request(Server, fetch_request(0)))
    end, Http).

upstream_errors_test() ->
    lists:foreach(fun({Status, Reason}) ->
        with_service(fun(Server, _, _) ->
            ?assertMatch({error, #{<<"reason">> := Reason, <<"rate_budget">> := #{}}},
                retinue_poe2_trade:request(Server, item_request()))
        end, fun(_, _, _, _) -> {ok, Status, #{}, <<"not JSON">>} end)
    end, [{403, <<"anonymous_trade_unavailable">>}, {500, <<"upstream_unavailable">>},
           {200, <<"invalid_metadata_response">>}, {302, <<"trade_request_rejected">>}]).

busy_and_dedup_test() ->
    Parent = self(),
    HTTP = fun(Method, Path, Headers, Body) ->
        Parent ! {http_waiting, self()},
        receive continue -> http(Method, Path, Headers, Body) after 2000 -> {error, test_timeout} end
    end,
    with_service(fun(Server, Calls, _) ->
        Req = #{<<"op">> => <<"trade_leagues">>},
        spawn_link(fun() -> Parent ! {one, retinue_poe2_trade:request(Server, Req)} end),
        Worker = receive {http_waiting, Pid} -> Pid after 1000 -> error(timeout) end,
        spawn_link(fun() -> Parent ! {two, retinue_poe2_trade:request(Server, Req)} end),
        ?assertMatch({error, #{<<"reason">> := <<"trade_busy">>}},
            retinue_poe2_trade:request(Server, item_request())),
        Worker ! continue,
        receive {one, R1} -> ?assertMatch({ok, _}, R1) after 2000 -> error(timeout) end,
        receive {two, R2} -> ?assertMatch({ok, _}, R2) after 2000 -> error(timeout) end,
        ?assertEqual(1, atomics:get(Calls, 1))
    end, HTTP).

metadata_must_revalidate_test() ->
    Attempts = atomics:new(1, []),
    HTTP = fun(Method, Path, Headers, Body) ->
        case atomics:add_get(Attempts, 1, 1) of
            N when N > 3 -> {ok, 503, #{<<"retry-after">> => <<"30">>}, <<>>};
            _ -> http(Method, Path, Headers, Body)
        end
    end,
    with_service(fun(Server, Calls, Clock) ->
        ?assertMatch({ok, _}, retinue_poe2_trade:request(Server, item_request())),
        atomics:add(Clock, 1, 61),
        ?assertMatch({error, #{<<"reason">> := <<"upstream_unavailable">>, <<"retry_after_seconds">> := 30}},
            retinue_poe2_trade:request(Server, search_request())),
        ?assertEqual(4, atomics:get(Calls, 1)),
        ?assertMatch({error, #{<<"reason">> := <<"rate_limited">>}},
            retinue_poe2_trade:request(Server, search_request())),
        ?assertEqual(4, atomics:get(Calls, 1))
    end, HTTP).

no_store_metadata_test() ->
    HTTP = fun(Method, Path, Headers, Body) ->
        {ok, Status, ResponseHeaders, Data} = http(Method, Path, Headers, Body),
        {ok, Status, ResponseHeaders#{<<"cache-control">> => <<"no-store">>}, Data}
    end,
    with_service(fun(Server, Calls, _) ->
        ?assertMatch({ok, _}, retinue_poe2_trade:request(Server, item_request())),
        ?assertMatch({ok, _}, retinue_poe2_trade:request(Server, item_request())),
        ?assertEqual(6, atomics:get(Calls, 1))
    end, HTTP).

magic_base_and_unidentified_test() ->
    with_service(fun(Server, _, _) ->
        Text = <<"Item Class: Helmets\nRarity: Magic\nHealthy Felt Cap of the Fox\n--------\n",
                 "Item Level: 40\n--------\n+82 to maximum Life">>,
        ?assertMatch({ok, #{<<"item">> := #{<<"base">> := <<"Felt Cap">>, <<"name">> := <<>>}}},
            retinue_poe2_trade:request(Server, (item_request())#{<<"text">> => Text}))
    end, fun http/4),
    {ok, Item} = parse(<<"Rarity: Rare\nFelt Cap\n--------\nItem Level: 40\n--------\nUnidentified">>),
    ?assertMatch(#{<<"name">> := <<>>, <<"unidentified">> := true, <<"modifiers">> := []}, Item).

gem_listing_level_test() ->
    Row = #{<<"id">> => <<"gem">>, <<"item">> => #{<<"frameType">> => 4, <<"ilvl">> => 0,
        <<"baseType">> => <<"Tame Beast">>, <<"properties">> => [#{<<"type">> => 5,
            <<"values">> => [[<<"14">>, 0]]}]},
        <<"listing">> => #{<<"price">> => #{<<"amount">> => 1, <<"currency">> => <<"exalted">>},
            <<"account">> => #{<<"name">> => <<"example">>}}},
    ?assertMatch({ok, [#{<<"gem_level">> := 14, <<"item_level">> := 0}]},
        retinue_poe2_trade_query:listings(#{<<"result">> => [Row]})),
    ?assertMatch({ok, [#{<<"gem_level">> := null}]},
        retinue_poe2_trade_query:listings(#{<<"result">> => [Row#{<<"item">> => #{}}]})).

empty_and_deleted_listings_test() ->
    ?assertMatch({ok, #{<<"total">> := 0}, []}, retinue_poe2_trade_query:search_result(
        #{<<"id">> => <<"search-id">>, <<"total">> => 0, <<"result">> => []}, <<"HC League">>)),
    ?assertMatch({ok, []}, retinue_poe2_trade_query:listings(#{<<"result">> => [null, #{}]})),
    ?assertMatch({error, _}, retinue_poe2_trade_query:search_result(
        #{<<"id">> => <<"../escape">>, <<"total">> => 0, <<"result">> => []}, <<"Standard">>)),
    ?assertMatch({error, _}, retinue_poe2_trade_query:listings(#{<<"result">> => lists:duplicate(11, null)})).

with_service(Test, HTTP) ->
    Calls = atomics:new(1, []),
    Clock = atomics:new(1, []),
    atomics:put(Clock, 1, 1700000000),
    Counted = fun(Method, Path, Headers, Body) ->
        atomics:add(Calls, 1, 1), HTTP(Method, Path, Headers, Body)
    end,
    {ok, Server} = retinue_poe2_trade:start_link(#{name => undefined, http => Counted,
        enrich => fun retinue_poe2_trade_profiles:prepare/1,
        clock => fun() -> atomics:get(Clock, 1) end}),
    try Test(Server, Calls, Clock) after gen_server:stop(Server) end.

http(get, <<"/api/trade2/data/", Name/binary>>, Headers, <<>>) ->
    CacheHeaders = #{<<"cache-control">> => <<"public, max-age=60">>, <<"etag">> => <<"v1">>},
    case proplists:get_value(<<"if-none-match">>, Headers) of
        <<"v1">> -> {ok, 304, CacheHeaders, <<>>};
        undefined ->
            Data = case Name of
                <<"leagues">> -> #{<<"result">> => leagues()};
                <<"stats">> -> fixture();
                <<"filters">> -> filter_fixture();
                <<"items">> -> #{<<"result">> => [#{<<"entries">> => [#{<<"type">> => <<"Felt Cap">>},
                    #{<<"type">> => <<"Omen of Amelioration">>}]}]}
            end,
            {ok, 200, CacheHeaders, iolist_to_binary(json:encode(Data))}
    end;
http(post, <<"/api/trade2/search/poe2/Standard">>, [], Body) ->
    #{<<"query">> := _} = json:decode(Body),
    {ok, 200, #{}, iolist_to_binary(json:encode(#{<<"id">> => <<"search-id">>, <<"total">> => 25,
        <<"result">> => [integer_to_binary(N) || N <- lists:seq(1, 25)]}))};
http(get, <<"/api/trade2/fetch/", Tail/binary>>, [], <<>>) ->
    [Ids, _] = binary:split(Tail, <<"?">>),
    [First | _] = Split = binary:split(Ids, <<",">>, [global]),
    true = length(Split) =< 10,
    {ok, 200, #{}, iolist_to_binary(json:encode(#{<<"result">> => [#{<<"id">> => First,
        <<"item">> => #{<<"name">> => <<"Goldrim">>, <<"baseType">> => <<"Felt Cap">>, <<"ilvl">> => 77},
        <<"listing">> => #{<<"indexed">> => <<"2025-02-17T09:15:53Z">>,
            <<"price">> => #{<<"amount">> => 20, <<"currency">> => <<"exalted">>},
            <<"account">> => #{<<"name">> => <<"fixture-account">>, <<"online">> => #{}}}}]}))}.

fixture() ->
    {ok, Bytes} = file:read_file(filename:join([filename:dirname(?FILE), "fixtures", "poe2_trade_stats.json"])),
    json:decode(Bytes).
parse(Text) ->
    {ok, Catalog} = retinue_poe2_trade_item:catalog(fixture()),
    retinue_poe2_trade_item:parse(Text, Catalog).
leagues() -> [#{<<"id">> => <<"Standard">>, <<"text">> => <<"Standard">>}].
item_request() -> #{<<"op">> => <<"trade_item">>, <<"text">> => item()}.
search_request() -> #{<<"op">> => <<"trade_search">>, <<"text">> => item(),
                      <<"league">> => <<"Standard">>, <<"status">> => <<"securable">>,
                      <<"base">> => true, <<"filters">> => []}.
fetch_request(Offset) -> #{<<"op">> => <<"trade_fetch">>, <<"search_id">> => <<"search-id">>,
                          <<"offset">> => Offset}.
item() -> equipment(<<"+82 to maximum Life\nAdds 10 to 20 Physical Damage\n",
    "12.5% reduced Movement Speed\nUnknown future modifier\n+46 to Evasion Rating\n",
    "+10 to maximum Life (implicit)\n+15 to maximum Life (rune)\nBonded: +25 to maximum Life (augment)">>).
equipment(Mods) -> <<"Item Class: Helmets\nRarity: Rare\nDoom Shelter\nFelt Cap\n--------\n",
    "Quality: +20% (augmented)\nEvasion Rating: 125\n--------\nItem Level: 77\n--------\n",
    Mods/binary, "\n--------\nCorrupted">>.
unique() -> <<"Item Class: Helmets\nRarity: Unique\nGoldrim\nFelt Cap\n--------\n",
    "Item Level: 77\n--------\n+31% to all Elemental Resistances">>.

filter_fixture() ->
    {ok, Bytes} = file:read_file(filename:join([filename:dirname(?FILE), "fixtures", "poe2_trade_filters.json"])),
    json:decode(Bytes).

build(Request, Item, Leagues) ->
    {ok, Options} = retinue_poe2_trade_query:options(filter_fixture()),
    retinue_poe2_trade_query:build(Request, Item, Leagues, Options).
