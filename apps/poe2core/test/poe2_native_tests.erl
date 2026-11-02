-module(poe2_native_tests).
-include_lib("eunit/include/eunit.hrl").

request_inventory_test_() ->
    Session = [{<<"get">>, 'GetSessionRequest', #{<<"dataset">> => <<"session">>}},
               {<<"subscribe">>, 'SubscribeSessionRequest', #{<<"dataset">> => <<"session">>}},
               {<<"unsubscribe">>, 'UnsubscribeSessionRequest', #{<<"subscription">> => 91}},
               {<<"publish">>, 'PublishSessionRequest', #{<<"dataset">> => <<"session">>,
                                                          <<"data">> => sample()}}],
    Trade = [{Op, Type, trade_args()} || {Op, Type} <-
        [{<<"trade_item">>, 'TradeItemRequest'}, {<<"trade_search">>, 'TradeSearchRequest'},
         {<<"trade_restore">>, 'TradeRestoreRequest'}, {<<"trade_predict">>, 'TradePredictRequest'}]],
    Other = [{<<"trade_leagues">>, 'TradeLeaguesRequest', #{}},
             {<<"trade_fetch">>, 'TradeFetchRequest', #{<<"search_id">> => <<"query">>, <<"offset">> => 0}},
             {<<"currency_asset">>, 'CurrencyAssetRequest', #{<<"currency">> => <<"exalted">>}}],
    [{binary_to_list(Op), fun() ->
        Data = Args#{<<"op">> => Op, <<"id">> => 16#ffffffffffffffff, <<"game">> => <<"poe2">>},
        {ok, Name, Bytes} = poe2_native:encode_request(Data),
        ?assertEqual(poe2_native_pb:msg_name_to_fqbin(Type), Name),
        ?assertEqual(Name, poe2_native:request(Op)),
        ?assertEqual({ok, Data}, poe2_native:decode_request(Name, Bytes))
    end} || {Op, Type, Args} <- Session ++ Trade ++ Other].

request_presence_test() ->
    Name = poe2_native:request(<<"trade_search">>),
    %% Stable wire vectors also cover Rust's transparent list/null transforms.
    ?assertEqual({ok, #{<<"op">> => <<"trade_search">>}}, poe2_native:decode_request(Name, <<>>)),
    ?assertEqual({ok, #{<<"op">> => <<"trade_search">>, <<"conditions">> => []}},
                 poe2_native:decode_request(Name, <<106, 0>>)),
    ?assertEqual({ok, #{<<"op">> => <<"trade_search">>, <<"filters">> => []}},
                 poe2_native:decode_request(Name, <<98, 0>>)),
    ?assertEqual({ok, #{<<"op">> => <<"trade_search">>, <<"rarity">> => null}},
                 poe2_native:decode_request(Name, <<50, 2, 16, 1>>)),
    ?assertEqual({ok, #{<<"op">> => <<"trade_search">>, <<"rarity">> => <<>>}},
                 poe2_native:decode_request(Name, <<50, 2, 10, 0>>)),
    ?assertEqual({ok, #{<<"op">> => <<"trade_search">>, <<"base">> => false}},
                 poe2_native:decode_request(Name, <<88, 0>>)).

checks_publication_test() ->
    Data = #{<<"context">> => #{<<"game_build">> => null, <<"session">> => <<"example">>},
        <<"checks">> => #{<<"native_state">> => #{<<"checked_at">> => 9007199254740993,
            <<"state">> => <<"fail">>, <<"error">> => <<"unreadable">>}},
        <<"diagnostics">> => [null, #{<<"address">> => 16#ffffffffffffffff}]},
    Request = #{<<"op">> => <<"publish">>, <<"dataset">> => <<"checks">>, <<"data">> => Data},
    {ok, Name, Bytes} = poe2_native:encode_request(Request),
    ?assertEqual(<<"retinue.poe2.v1.PublishChecksRequest">>, Name),
    ?assertEqual(Name, poe2_native:request(Request)),
    ?assertEqual(Name, poe2_native:request({<<"publish">>, <<"checks">>})),
    ?assertEqual({ok, Request}, poe2_native:decode_request(Name, Bytes)),
    ?assertEqual({ok, #{<<"op">> => <<"publish">>, <<"dataset">> => <<"checks">>}},
                 poe2_native:decode_request(Name, <<>>)).

uint64_wire_vector_test() ->
    Name = poe2_native:request(<<"trade_leagues">>),
    Bytes = <<16, 255, 255, 255, 255, 255, 255, 255, 255, 255, 1>>,
    ?assertEqual({ok, #{<<"op">> => <<"trade_leagues">>, <<"id">> => 16#ffffffffffffffff}},
                 poe2_native:decode_request(Name, Bytes)).

typed_session_defaults_test() ->
    lists:foreach(fun(Op) ->
        ?assertEqual({ok, #{<<"op">> => Op, <<"dataset">> => <<"session">>}},
            poe2_native:decode_request(poe2_native:request(Op), <<>>))
    end, [<<"get">>, <<"subscribe">>, <<"publish">>]).

request_rejection_test() ->
    Name = poe2_native:request(<<"trade_search">>),
    WrongOp = wire('TradeSearchRequest', #{<<"op">> => <<"publish">>}),
    ?assertEqual({error, invalid_poe2_request}, poe2_native:decode_request(Name, WrongOp)),
    ?assertEqual({error, invalid_poe2_request}, poe2_native:decode_request(Name, <<255>>)),
    ?assertEqual({error, unsupported_poe2_request}, poe2_native:decode_request(
        <<"retinue.poe2.v1.Session">>, <<>>)),
    ?assertEqual({error, unsupported_poe2_request}, poe2_native:decode_request(
        <<"retinue.poe2.v1.UntrustedName">>, <<>>)),
    ?assertEqual(undefined, poe2_native:request(<<"unknown">>)),
    ?assertEqual({error, invalid_poe2_request}, poe2_native:encode_request(
        #{<<"op">> => <<"get">>, <<"dataset">> => <<"other">>})),
    ?assertEqual({error, invalid_poe2_request}, poe2_native:encode_request(
        #{<<"op">> => <<"trade_search">>, <<"unknown_argument">> => true})).

invalid_wrappers_test() ->
    Name = poe2_native:request(<<"trade_search">>),
    lists:foreach(fun(Json) ->
        ?assertEqual({error, invalid_poe2_request}, poe2_native:decode_request(Name,
            wire('TradeSearchRequest', Json)))
    end, [#{<<"rarity">> => #{}}, #{<<"rarity">> => #{<<"null">> => false}},
          #{<<"filters">> => #{<<"values">> => [#{<<"min">> => #{}}]}}]).

session_presence_and_integers_test() ->
    Sample = sample(),
    Session = #{<<"schema">> => 2, <<"game">> => <<"poe2">>,
        <<"revision">> => 16#ffffffffffffffff, <<"updated_at">> => null,
        <<"stream">> => maps:get(<<"stream">>, Sample),
        <<"sequence">> => maps:get(<<"sequence">>, Sample),
        <<"observed_at">> => maps:get(<<"observed_at">>, Sample),
        <<"status">> => <<"observed">>, <<"process">> => maps:get(<<"process">>, Sample),
        <<"current">> => maps:get(<<"snapshot">>, Sample), <<"error">> => null,
        <<"events">> => [], <<"receipt">> => #{<<"connected">> => false, <<"received_at">> => 0}},
    roundtrip(<<"session">>, Session),
    lists:foreach(fun(Current) -> roundtrip(<<"session">>, Session#{<<"current">> => Current}) end,
        [null, #{}, maps:remove(<<"social">>, maps:get(<<"snapshot">>, Sample))]),
    roundtrip(<<"session_published">>, #{<<"revision">> => 0}),
    roundtrip(<<"session_published">>, #{<<"revision">> => 1, <<"unchanged">> => false}),
    roundtrip(<<"session_published">>, #{<<"revision">> => 1, <<"unchanged">> => true}).

session_events_test_() ->
    Area = #{<<"name">> => <<"Area">>, <<"in_town">> => false, <<"change_key">> => 7},
    Character = #{<<"name">> => <<"Character">>, <<"class">> => null},
    State = #{<<"scene">> => <<"in_game">>, <<"states">> => []},
    Cases = [{<<"state">>, State, #{}}, {<<"area">>, Area, #{}},
             {<<"character">>, Character, #{}},
             {<<"xp">>, 16#ffffffff, #{<<"character">> => <<"Character">>}},
             {<<"level">>, 100, #{<<"character">> => <<"Character">>}},
             {<<"gold">>, -16#80000000, #{<<"character">> => <<"Character">>}},
             {<<"panel">>, false, #{<<"panel">> => <<"inventory">>}},
             {<<"death">>, 0, #{<<"character">> => <<"Character">>}},
             {<<"unique_monster_death">>, 0, #{<<"id">> => 1, <<"address">> => 16#100000,
                 <<"name">> => <<"Example Unique">>, <<"tree">> => 16#110000, <<"area_key">> => 7}}],
    [{binary_to_list(Kind), fun() ->
        Base = Extra#{<<"kind">> => Kind, <<"stream">> => <<"stream">>,
            <<"sequence">> => 16#ffffffffffffffff, <<"observed_at">> => 9007199254740993,
            <<"process">> => #{<<"pid">> => 42, <<"started">> => 9007199254740993}},
        roundtrip(<<"session">>, #{<<"events">> => [Base#{<<"from">> => null, <<"to">> => Value},
            Base#{<<"from">> => Value, <<"to">> => Value}]})
    end} || {Kind, Value, Extra} <- Cases].

trade_reply_inventory_test_() ->
    Results = results(),
    Cases = [{<<"trade_item">>, #{<<"item">> => item(), <<"leagues">> => leagues(),
                  <<"league">> => <<"Standard">>, <<"search_options">> => options(),
                  <<"prediction_available">> => false, <<"rate_budget">> => budget()}},
             {<<"trade_leagues">>, #{<<"leagues">> => leagues(), <<"league">> => <<"Standard">>,
                                     <<"rate_budget">> => budget()}},
             {<<"trade_search">>, Results},
             {<<"trade_fetch">>, #{<<"search_id">> => <<"query">>, <<"offset">> => 0,
                 <<"next_offset">> => null, <<"listings">> => [listing()], <<"rate_budget">> => budget()}},
             {<<"trade_restore">>, #{<<"results">> => Results, <<"auto_search_allowed">> => false,
                                      <<"rate_budget">> => budget()}},
             {<<"trade_predict">>, #{<<"estimate">> => #{<<"amount">> => 1.0,
                 <<"currency">> => <<"exalted">>, <<"uncertainty">> => #{<<"low">> => 0,
                 <<"high">> => 1.5, <<"confidence">> => 95}}}},
             {<<"trade_error">>, #{<<"reason">> => <<"trade_busy">>,
                 <<"retry_after_seconds">> => 0, <<"rate_budget">> => budget()}},
             {<<"currency_asset">>, #{<<"id">> => <<"exalted">>, <<"text">> => <<"Exalted Orb">>,
                 <<"image">> => <<"/image/example.png">>, <<"tier">> => null,
                 <<"digest">> => binary:copy(<<"a">>, 64), <<"media_type">> => <<"image/png">>,
                 <<"byte_size">> => 100, <<"path">> => <<"/cache/example.png">>}}],
    [{binary_to_list(Dataset), fun() -> roundtrip(Dataset, Data) end} || {Dataset, Data} <- Cases].

trade_miss_and_nullable_choices_test() ->
    roundtrip(<<"trade_restore">>, #{<<"results">> => null, <<"auto_search_allowed">> => true}),
    roundtrip(<<"trade_predict">>, #{<<"estimate">> => null, <<"reason">> => <<"unsupported_item">>}),
    roundtrip(<<"trade_predict">>, #{<<"estimate">> => #{<<"amount">> => 0,
                                                       <<"currency">> => <<"exalted">>}}),
    roundtrip(<<"trade_item">>, #{<<"search_options">> => options()}),
    roundtrip(<<"trade_item">>, #{<<"item">> => #{<<"conditions">> => [
        #{<<"id">> => null, <<"text">> => <<"Unmodifiable">>, <<"filters">> => #{}}]}}).

number_representation_test_() ->
    [{integer_to_list(Index), fun() ->
        roundtrip(<<"trade_predict">>, #{<<"estimate">> => #{<<"amount">> => N}})
    end} || {Index, N} <- lists:enumerate([0, 0.0, 1, 1.0, -1, -1.25,
        9007199254740993, 16#7fffffffffffffff, 16#ffffffffffffffff, -16#8000000000000000])].

no_silent_field_loss_test() ->
    ?assertError({invalid_native_view, 'TradeEvaluation'}, poe2_native:encode(<<"trade_item">>,
        #{<<"item">> => #{<<"new_domain_field">> => true}})),
    ?assertEqual(dynamic, poe2_native:encode(<<"trade">>, results())),
    ?assertEqual(dynamic, poe2_native:encode(<<"unowned_dataset">>, #{})).

roundtrip(Dataset, Data) ->
    {ok, Name, Bytes} = poe2_native:encode(Dataset, Data),
    ?assertEqual({ok, Data}, poe2_native:decode(Name, Bytes)).

wire(Type, Json) -> poe2_native_pb:encode_msg(poe2_native_pb:from_json(Json, Type), Type).

trade_args() ->
    #{<<"text">> => <<"Rarity: Normal\nRing\n--------\nItem Level: 1">>,
      <<"league">> => <<"Standard">>, <<"status">> => <<"securable">>,
      <<"rarity">> => null, <<"currency">> => null, <<"indexed">> => <<"1day">>,
      <<"item_level">> => #{<<"min">> => 1, <<"max">> => null}, <<"sort">> => <<"asc">>,
      <<"base">> => false, <<"refresh">> => false, <<"conditions">> => [],
      <<"filters">> => [#{<<"id">> => <<"stat">>, <<"min">> => 0, <<"max">> => 1.0},
                        #{<<"id">> => <<"other">>, <<"max">> => null}]}.

sample() ->
    #{<<"stream">> => <<"stream">>, <<"sequence">> => 16#ffffffffffffffff,
      <<"observed_at">> => 9007199254740993, <<"status">> => <<"observed">>,
      <<"process">> => #{<<"pid">> => 16#ffffffff, <<"started">> => 16#ffffffffffffffff,
          <<"executable">> => <<"/game/example.exe">>, <<"image_base">> => 16#ffffffffffffffff},
      <<"error">> => null, <<"snapshot">> => #{<<"scene">> => <<"in_game">>, <<"states">> => [],
          <<"player">> => #{<<"name">> => unicode:characters_to_binary([16#03A9]),
              <<"class">> => null, <<"level">> => 1, <<"experience">> => 16#ffffffff,
              <<"gold">> => -16#80000000, <<"life">> => 0},
          <<"area">> => null, <<"panels">> => #{<<"stash">> => null, <<"inventory">> => false},
          <<"social">> => #{<<"local">> => #{<<"character">> => <<"Character">>,
              <<"account">> => <<"Example#1234">>, <<"league">> => <<"Standard">>},
              <<"party">> => [#{<<"character">> => <<"Other">>, <<"account">> => <<"Other#5678">>,
                  <<"league">> => null, <<"class_id">> => 0, <<"level">> => null}]},
          <<"entities">> => #{<<"tree">> => 16#100000, <<"area_key">> => 7, <<"count">> => 1,
              <<"unique_monsters">> => [#{<<"id">> => 1, <<"address">> => 16#110000,
                  <<"name">> => <<"Example Unique">>, <<"life">> => 0}]},
          <<"issues">> => [#{<<"field">> => <<"area">>, <<"reason">> => <<"unreadable">>}]}}.

item() ->
    #{<<"name">> => <<>>, <<"base">> => <<"Ring">>, <<"rarity">> => <<"Rare">>,
      <<"item_class">> => <<"Rings">>, <<"category">> => <<"accessory.ring">>,
      <<"ranking_status">> => <<"ready">>, <<"base_required">> => false, <<"auto_search">> => false,
      <<"profiles">> => [<<"allmods">>, <<"fuzzy">>], <<"default_profile">> => <<"allmods">>,
      <<"item_level">> => 77, <<"properties">> => [#{<<"text">> => <<"Quality: +20%">>}],
      <<"modifiers">> => [#{<<"key">> => <<"key">>, <<"id">> => <<"stat">>,
          <<"text">> => <<"+10 to Life">>, <<"section">> => <<"explicit">>,
          <<"min">> => 10, <<"max">> => 10.0, <<"supported">> => true,
          <<"resolution">> => <<"resolved">>, <<"selected">> => false,
          <<"quick_selected">> => true, <<"exact_selected">> => true, <<"base_selected">> => false,
          <<"ranges">> => #{<<"allmods">> => #{<<"min">> => 10}, <<"fuzzy">> => #{<<"min">> => 9}},
          <<"magnitudes">> => [#{<<"value">> => 10, <<"min">> => 8, <<"max">> => 12}],
          <<"decorator">> => #{<<"raw">> => <<"{Prefix Modifier}">>, <<"group">> => 0,
              <<"name">> => <<"Example">>, <<"affix">> => <<"Prefix">>, <<"tier">> => 2,
              <<"tags">> => [<<"Life">>]}, <<"display_template">> => <<"+# to Life">>,
          <<"display_values">> => [10, 1.0], <<"negated">> => false, <<"ranking_score">> => 2.5,
          <<"internal_ids">> => [<<"base_maximum_life">>],
          <<"tier_info">> => #{<<"current">> => 2, <<"count">> => 3, <<"max">> => 1}}],
      <<"conditions">> => [#{<<"id">> => <<"modifiable">>, <<"text">> => <<"Modifiable">>,
          <<"filters">> => #{<<"mirrored">> => false, <<"sanctified">> => false,
                            <<"corrupted">> => false, <<"twice_corrupted">> => false}}],
      <<"corrupted">> => false, <<"unidentified">> => false}.

listing() ->
    #{<<"id">> => <<"listing">>, <<"name">> => <<>>, <<"base">> => <<"Ring">>,
      <<"item_level">> => 77, <<"gem_level">> => null, <<"required_level">> => 0,
      <<"gem_sockets">> => null, <<"item">> => item(), <<"type">> => <<"securable">>,
      <<"quality">> => 0, <<"indexed_at">> => -1, <<"amount">> => 1,
      <<"currency">> => <<"exalted">>, <<"account">> => <<"Example#1234">>,
      <<"online">> => false, <<"indexed">> => <<"1969-12-31T23:59:59Z">>}.

results() ->
    #{<<"search_id">> => <<"query">>, <<"url">> => <<"https://example.invalid/query">>,
      <<"total">> => 9007199254740993, <<"returned">> => 1, <<"next_offset">> => null,
      <<"listings">> => [listing()], <<"rate_budget">> => budget()}.

leagues() -> [#{<<"id">> => <<"Standard">>, <<"text">> => <<"Standard">>}].
options() ->
    maps:from_list([{Key, [#{<<"id">> => null, <<"text">> => <<"Any">>},
        #{<<"id">> => <<"choice">>, <<"text">> => <<"Choice">>}]} ||
        Key <- [<<"status">>, <<"rarity">>, <<"currency">>, <<"indexed">>]]).
budget() -> #{<<"observed_at_ms">> => 9007199254740993, <<"windows">> => [
    #{<<"usage">> => 0.0, <<"expires_in_ms">> => 1000}], <<"retry_after_ms">> => 0}.
