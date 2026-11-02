-module(poe2_producer_contract_tests).
-include_lib("eunit/include/eunit.hrl").

%% Central umbrella EUnit supplies the daemon modules. These are pure producer
%% calls: no service startup, network, mutable cache, or live game reads.
clipboard_producers_test_() ->
    Files = ["poe2_advanced_item.txt", "poe2_magic_charm.txt", "poe2_rare_body_armour.txt",
             "poe2_unique_spear.txt", "poe2_property_weapon.txt", "poe2_quest_logbook.txt",
             "poe2_omen.txt", "poe2_tame_beast.txt"],
    [{File, fun() ->
        {ok, Catalog} = retinue_poe2_trade_item:catalog(json_fixture("poe2_modifier_stats.json")),
        {ok, Parsed} = retinue_poe2_trade_item:parse(fixture(File), Catalog),
        Item = retinue_poe2_trade_profiles:prepare(Parsed),
        roundtrip(<<"trade_item">>, #{<<"item">> => Item})
    end} || File <- Files].

filter_metadata_producer_test() ->
    {ok, Options} = retinue_poe2_trade_query:options(json_fixture("poe2_trade_filters.json")),
    roundtrip(<<"trade_item">>, #{<<"search_options">> => Options}).

listing_and_search_producers_test() ->
    Item = #{<<"name">> => <<"Example Ring">>, <<"typeLine">> => <<"Gold Ring">>,
        <<"frameType">> => 2, <<"ilvl">> => 84, <<"identified">> => true,
        <<"corrupted">> => true, <<"explicitMods">> => [<<"+90 to maximum Life">>],
        <<"implicitMods">> => [<<"10% increased Rarity of Items found">>],
        <<"requirements">> => [#{<<"name">> => <<"Level">>, <<"values">> => [[<<"65">>, 0]]}],
        <<"properties">> => [#{<<"name">> => <<"Quality">>, <<"values">> => [[<<"+20%">>, 0]]}]},
    Row = #{<<"id">> => <<"listing">>, <<"item">> => Item,
        <<"listing">> => #{<<"type">> => <<"secure">>, <<"indexed">> => <<"2025-02-17T09:15:53Z">>,
            <<"price">> => #{<<"amount">> => 40, <<"currency">> => <<"exalted">>},
            <<"account">> => #{<<"name">> => <<"Example">>}}},
    {ok, Listings} = retinue_poe2_trade_query:listings(#{<<"result">> => [Row]}),
    {ok, Search, _} = retinue_poe2_trade_query:search_result(
        #{<<"id">> => <<"search">>, <<"total">> => 1, <<"result">> => [<<"listing">>]}, <<"Standard">>),
    roundtrip(<<"trade_search">>, Search),
    roundtrip(<<"trade_fetch">>, #{<<"search_id">> => <<"search">>, <<"offset">> => 0,
        <<"next_offset">> => null, <<"listings">> => Listings}).

budget_producer_test() ->
    roundtrip(<<"trade_leagues">>, #{<<"rate_budget">> => retinue_poe2_trade_http:budget(#{}, 0)}).

session_producer_test() ->
    Empty = retinue_poe2_session:empty(),
    roundtrip(<<"session">>, Empty),
    Player = #{<<"name">> => <<"Example">>, <<"class">> => null, <<"level">> => 1,
               <<"experience">> => 0, <<"gold">> => null, <<"life">> => 100},
    Snapshot = #{<<"scene">> => <<"in_game">>, <<"states">> => [<<"InGameState">>],
        <<"player">> => Player, <<"area">> => #{<<"name">> => <<"Area">>,
            <<"in_town">> => false, <<"change_key">> => 1},
        <<"panels">> => #{<<"inventory">> => false, <<"stash">> => null},
        <<"issues">> => [], <<"social">> => null, <<"entities">> => null},
    Raw = #{<<"stream">> => <<"fixture">>, <<"sequence">> => 1, <<"observed_at">> => 1000,
        <<"status">> => <<"observed">>, <<"process">> => #{<<"pid">> => 42,
            <<"started">> => 1000, <<"executable">> => <<"/game/example.exe">>,
            <<"image_base">> => 16#140000000}, <<"snapshot">> => Snapshot, <<"error">> => null},
    {ok, Sample} = retinue_poe2_session:sample(Raw),
    {changed, First} = retinue_poe2_session:apply(Sample, Empty),
    Next = Raw#{<<"sequence">> => 2, <<"observed_at">> => 2000,
        <<"snapshot">> => Snapshot#{<<"player">> => Player#{<<"experience">> => 100,
            <<"level">> => 2, <<"gold">> => 0, <<"life">> => 0},
            <<"panels">> => #{<<"inventory">> => true}}},
    {ok, NextSample} = retinue_poe2_session:sample(Next),
    {changed, Second} = retinue_poe2_session:apply(NextSample, First),
    roundtrip(<<"session">>, First),
    roundtrip(<<"session">>, Second),
    {ok, Name, Bytes} = poe2_native:encode_event(<<"session">>, Second),
    ?assertEqual({ok, Second}, poe2_native:decode(Name, Bytes)),
    Publish = #{<<"op">> => <<"publish">>, <<"dataset">> => <<"session">>, <<"data">> => NextSample},
    {ok, RequestName, RequestBytes} = poe2_native:encode_request(Publish),
    {ok, Decoded} = poe2_native:decode_request(RequestName, RequestBytes),
    ?assertEqual({ok, NextSample}, retinue_poe2_session:sample(maps:get(<<"data">>, Decoded))).

roundtrip(Dataset, Data) ->
    {ok, Name, Bytes} = poe2_native:encode(Dataset, Data),
    ?assertEqual({ok, Data}, poe2_native:decode(Name, Bytes)).

json_fixture(Name) -> json:decode(fixture(Name)).
fixture(Name) ->
    {ok, Bytes} = file:read_file(filename:join("apps/retinue_cli/test/fixtures", Name)),
    Bytes.
