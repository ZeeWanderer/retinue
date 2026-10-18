%%%-------------------------------------------------------------------
%% Owned relic recommendations from WFCD rewards, player data, and Market quotes.
%%%-------------------------------------------------------------------
-module(wf_relic_recommendations).

-export([fetch/0, parse/1, build/6, price_slugs/5, price_slugs/6,
         catalog_version/0]).

-define(URL,
        "https://raw.githubusercontent.com/WFCD/warframe-items/master/data/json/Relics.json").
-define(VOID_TRACES, <<"/Lotus/Types/Items/MiscItems/VoidTearDrop">>).

-doc "Return compact relic-catalog schema version.".
-spec catalog_version() -> pos_integer().
catalog_version() -> 3.

-doc "Fetch and compact the WFCD relic catalog.".
-spec fetch() -> {ok, map()} | {error, term()}.
fetch() ->
    Headers = [{"user-agent", "retinue (+https://github.com/ZeeWanderer/retinue)"},
               {"accept", "application/json"}],
    Result = try
        case application:get_env(retinue_daemon, relic_catalog_http_fun) of
            {ok, Fun} when is_function(Fun, 2) -> Fun(?URL, Headers);
            _ -> real_http_get(?URL, Headers)
        end
    catch HttpClass:HttpReason ->
        {error, {relic_catalog_http_crash, HttpClass, HttpReason}}
    end,
    case Result of
        {ok, 200, Body} -> parse(iolist_to_binary(Body));
        {ok, Status, _Body} -> {error, {relic_catalog_http_status, Status}};
        {error, Reason} -> {error, {relic_catalog_http_failed, Reason}};
        Other -> {error, {invalid_relic_catalog_http_result, Other}}
    end.

-doc "Parse WFCD Relics.json into the fields needed by recommendation scoring.".
-spec parse(binary()) -> {ok, map()} | {error, term()}.
parse(Body) ->
    try jsone:decode(Body, [{object_format, map}]) of
        Values when is_list(Values) ->
            Relics = lists:filtermap(fun compact/1, Values),
            case Relics of
                [] -> {error, empty_relic_catalog};
                _ -> {ok, maps:from_list(Relics)}
            end;
        Other -> {error, {invalid_relic_catalog_root, Other}}
    catch error:Reason -> {error, {invalid_relic_catalog_json, Reason}}
    end.

-type view() :: planner | recommendations.
-type build_options() :: #{view := view(), limit := all | pos_integer()}.

-doc "Rank owned relics for either compact planner or per-refinement recommendations.".
-spec build(binary(), map(), [map()], map(), map(), build_options()) -> map().
build(Era, Catalog, Items, Quotes, PlayerSnapshot, Options) ->
    Owned = owned_counts(PlayerSnapshot),
    ItemBySlug = maps:from_list(
                   [{maps:get(<<"slug">>, Item), Item}
                    || Item <- Items, maps:is_key(<<"slug">>, Item)]),
    Suggestions = ranked(Era, Catalog, Owned, ItemBySlug, Quotes,
                         maps:get(view, Options),
                         maps:get(only_owned, Options, true)),
    #{<<"revision">> => maps:get(revision, PlayerSnapshot, 0),
      <<"trace_count">> => maps:get(?VOID_TRACES, Owned, 0),
      <<"items">> => limit(Suggestions, maps:get(limit, Options))}.

limit(Items, all) -> Items;
limit(Items, Count) -> lists:sublist(Items, Count).

-doc "Return reward slugs worth warming for the highest-ducat owned relics.".
-spec price_slugs(binary(), map(), [map()], map(), non_neg_integer()) -> [binary()].
price_slugs(Era, Catalog, Items, PlayerSnapshot, CandidateLimit) ->
    price_slugs(Era, Catalog, Items, PlayerSnapshot, CandidateLimit, true).

-doc "Return reward slugs for the highest-ducat relics in one ownership scope.".
-spec price_slugs(binary(), map(), [map()], map(), non_neg_integer(), boolean()) -> [binary()].
price_slugs(Era, Catalog, Items, PlayerSnapshot, CandidateLimit, OnlyOwned) ->
    Owned = owned_counts(PlayerSnapshot),
    ItemBySlug = maps:from_list(
                   [{maps:get(<<"slug">>, Item), Item}
                    || Item <- Items, maps:is_key(<<"slug">>, Item)]),
    Candidates = ranked_candidates(Era, Catalog, Owned, ItemBySlug, #{},
                                   recommendations, OnlyOwned),
    Selected = lists:sublist(Candidates, CandidateLimit),
    unique(lists:append(
             [maps:get(price_slugs, Suggestion, []) || Suggestion <- Selected]), []).

real_http_get(Url, Headers) ->
    case httpc:request(get, {Url, Headers}, [{timeout, 30000}],
                       [{body_format, binary}]) of
        {ok, {{_Version, Status, _Reason}, _ResponseHeaders, Body}} ->
            {ok, Status, Body};
        {error, Reason} -> {error, Reason}
    end.

compact(#{<<"uniqueName">> := Unique, <<"name">> := Name,
          <<"rewards">> := Rewards} = Relic)
  when is_binary(Unique), is_binary(Name), is_list(Rewards) ->
    case era(Name, Unique) of
        undefined -> false;
        Era ->
            CompactRewards = lists:filtermap(fun compact_reward/1, Rewards),
            {true, {Unique, #{name => Name, era => Era,
                              image_name => maps:get(<<"imageName">>, Relic, undefined),
                              vaulted => maps:get(<<"vaulted">>, Relic, false) =:= true,
                              rewards => CompactRewards}}}
    end;
compact(_Relic) -> false.

compact_reward(#{<<"chance">> := Chance, <<"item">> := Item} = Reward)
  when (is_integer(Chance) orelse is_float(Chance)), is_map(Item) ->
    Market = maps:get(<<"warframeMarket">>, Item, #{}),
    Slug = maps:get(<<"urlName">>, Market, undefined),
    {true, #{chance => Chance, slug => Slug,
             name => maps:get(<<"name">>, Item, Slug),
             rarity => maps:get(<<"rarity">>, Reward, undefined)}};
compact_reward(_Reward) -> false.

era(Name, Unique) ->
    case binary:split(Name, <<" ">>, [global, trim_all]) of
        [Era | _] when Era =:= <<"Lith">>; Era =:= <<"Meso">>;
                       Era =:= <<"Neo">>; Era =:= <<"Axi">>;
                       Era =:= <<"Requiem">> ->
            string:lowercase(Era);
        [<<"Vanguard">> | _] -> projection_era(Unique);
        _ -> undefined
    end.

projection_era(Unique) when is_binary(Unique) ->
    case re:run(Unique, <<"/T([1-4])">>, [{capture, [1], binary}]) of
        {match, [<<"1">>]} -> <<"lith">>;
        {match, [<<"2">>]} -> <<"meso">>;
        {match, [<<"3">>]} -> <<"neo">>;
        {match, [<<"4">>]} -> <<"axi">>;
        nomatch -> <<"vanguard">>
    end;
projection_era(_Unique) -> <<"vanguard">>.

ranked(Era, Catalog, Owned, ItemBySlug, Quotes, View, OnlyOwned) ->
    Candidates = ranked_candidates(Era, Catalog, Owned, ItemBySlug, Quotes,
                                   View, OnlyOwned),
    [maps:without([price_slugs, price_ready, sort_platinum, sort_ducats], Candidate)
     || Candidate <- Candidates].

ranked_candidates(Era, Catalog, Owned, ItemBySlug, Quotes, View, OnlyOwned) ->
    Variants = lists:filtermap(
      fun({Unique, Relic}) ->
          Count = maps:get(Unique, Owned, 0),
          case (not OnlyOwned orelse Count > 0) andalso
               era_matches(Era, maps:get(era, Relic)) of
              true ->
                  Rewards = maps:get(rewards, Relic),
                  {PricedRewards, PriceableRewards} = price_coverage(Rewards, Quotes),
                  PriceReady = PriceableRewards > 0 andalso
                               PricedRewards =:= PriceableRewards,
                  Platinum = expected_max(
                               Rewards,
                               fun(Reward) -> reward_platinum(Reward, Quotes) end),
                  Ducats = expected_max(
                             Rewards,
                             fun(Reward) -> reward_ducats(Reward, ItemBySlug) end),
                  {_Family, Refinement} = relic_name_parts(maps:get(name, Relic)),
                  Public = #{<<"id">> => Unique,
                             <<"name">> => maps:get(name, Relic),
                             <<"era">> => maps:get(era, Relic),
                             <<"refinement">> => Refinement,
                             <<"amount_owned">> => Count,
                             <<"vaulted">> => maps:get(vaulted, Relic),
                             <<"favorite">> => false,
                             <<"asset">> => relic_asset(Unique, Relic),
                             <<"rewards">> =>
                                 [public_reward(Reward, ItemBySlug, Quotes, Owned)
                                  || Reward <- Rewards],
                             <<"expected_platinum">> =>
                                 case PricedRewards > 0 of true -> Platinum; false -> null end,
                             <<"price_complete">> => PriceReady,
                             <<"priced_rewards">> => PricedRewards,
                             <<"priceable_rewards">> => PriceableRewards,
                             <<"expected_ducats">> => Ducats},
                  Slugs = [Slug || #{slug := Slug} <- Rewards, is_binary(Slug)],
                  {true, Public#{price_slugs => Slugs,
                                 price_ready => PriceReady,
                                 sort_platinum => Platinum,
                                 sort_ducats => Ducats}};
              false -> false
          end
      end,
      maps:to_list(Catalog)),
    Candidates = case View of
        planner -> group_candidates(Variants);
        recommendations -> Variants
    end,
    lists:sort(fun recommendation_before/2, Candidates).

group_candidates(Variants) ->
    Groups = lists:foldl(
      fun(Variant, Acc) ->
          Name = maps:get(<<"name">>, Variant),
          GroupName = relic_group_name(Name),
          case maps:get(GroupName, Acc, undefined) of
              undefined -> Acc#{GroupName => new_group(GroupName, Variant)};
              Group -> Acc#{GroupName => merge_group(Group, Variant)}
          end
      end,
      #{},
      lists:sort(fun(A, B) -> maps:get(<<"name">>, A) < maps:get(<<"name">>, B) end,
                 Variants)),
    [finalize_group(Group) || Group <- maps:values(Groups)].

new_group(GroupName, Variant) ->
    Variant#{<<"id">> => <<"relic-group:", GroupName/binary>>,
             <<"name">> => GroupName,
             <<"refinements">> => [refinement_row(Variant)]}.

merge_group(Group, Variant) ->
    PreferVariant = maps:get(<<"refinement">>, Variant) =:= <<"Intact">>,
    Group#{<<"amount_owned">> => maps:get(<<"amount_owned">>, Group) +
                                  maps:get(<<"amount_owned">>, Variant),
           <<"vaulted">> => maps:get(<<"vaulted">>, Group) andalso
                             maps:get(<<"vaulted">>, Variant),
           <<"asset">> => choose(PreferVariant, maps:get(<<"asset">>, Variant),
                                  maps:get(<<"asset">>, Group)),
           <<"rewards">> => choose(PreferVariant, maps:get(<<"rewards">>, Variant),
                                    maps:get(<<"rewards">>, Group)),
           <<"expected_platinum">> => max_nullable(
                                         maps:get(<<"expected_platinum">>, Group),
                                         maps:get(<<"expected_platinum">>, Variant)),
           <<"expected_ducats">> => max(maps:get(<<"expected_ducats">>, Group),
                                        maps:get(<<"expected_ducats">>, Variant)),
           <<"price_complete">> => maps:get(<<"price_complete">>, Group) andalso
                                    maps:get(<<"price_complete">>, Variant),
           <<"priced_rewards">> => max(maps:get(<<"priced_rewards">>, Group),
                                      maps:get(<<"priced_rewards">>, Variant)),
           <<"priceable_rewards">> => max(maps:get(<<"priceable_rewards">>, Group),
                                         maps:get(<<"priceable_rewards">>, Variant)),
           <<"refinements">> => [refinement_row(Variant) |
                                  maps:get(<<"refinements">>, Group)],
           price_slugs => unique(maps:get(price_slugs, Variant) ++
                                 maps:get(price_slugs, Group), []),
           price_ready => maps:get(price_ready, Group) andalso
                          maps:get(price_ready, Variant),
           sort_platinum => max(maps:get(sort_platinum, Group),
                                maps:get(sort_platinum, Variant)),
           sort_ducats => max(maps:get(sort_ducats, Group),
                              maps:get(sort_ducats, Variant))}.

finalize_group(Group) ->
    Refinements = lists:sort(
                    fun(A, B) -> refinement_rank(maps:get(<<"refinement">>, A)) <
                                 refinement_rank(maps:get(<<"refinement">>, B)) end,
                    maps:get(<<"refinements">>, Group)),
    Group#{<<"refinements">> => Refinements}.

refinement_row(Variant) ->
    maps:with([<<"refinement">>, <<"amount_owned">>, <<"expected_platinum">>,
               <<"expected_ducats">>, <<"price_complete">>], Variant).

relic_group_name(Name) ->
    case binary:split(Name, <<" ">>, [global, trim_all]) of
        [Era, Code | _] -> <<Era/binary, " ", Code/binary, " Relic">>;
        _ -> Name
    end.

refinement_rank(<<"Intact">>) -> 0;
refinement_rank(<<"Exceptional">>) -> 1;
refinement_rank(<<"Flawless">>) -> 2;
refinement_rank(<<"Radiant">>) -> 3;
refinement_rank(_Other) -> 4.

choose(true, Value, _Previous) -> Value;
choose(false, _Value, Previous) -> Previous.

max_nullable(null, null) -> null;
max_nullable(null, Right) -> Right;
max_nullable(Left, null) -> Left;
max_nullable(Left, Right) -> max(Left, Right).

recommendation_before(A, B) ->
    KeyA = {maps:get(price_ready, A), maps:get(sort_platinum, A),
            maps:get(sort_ducats, A), maps:get(<<"name">>, A)},
    KeyB = {maps:get(price_ready, B), maps:get(sort_platinum, B),
            maps:get(sort_ducats, B), maps:get(<<"name">>, B)},
    KeyA > KeyB.

era_matches(<<"all">>, Era) ->
    lists:member(Era, [<<"lith">>, <<"meso">>, <<"neo">>, <<"axi">>,
                       <<"requiem">>, <<"vanguard">>]);
era_matches(Era, Era) -> true;
era_matches(_Wanted, _Actual) -> false.

price_coverage(Rewards, Quotes) ->
    lists:foldl(
      fun(Reward, {Priced, Total}) when is_map(Reward) ->
              case is_forma(maps:get(name, Reward, undefined)) of
                  true -> {Priced + 1, Total + 1};
                  false -> price_coverage_market(Reward, Quotes, Priced, Total)
              end;
         (_Reward, Coverage) -> Coverage
      end,
      {0, 0},
      Rewards).

price_coverage_market(#{slug := Slug}, Quotes, Priced, Total) when is_binary(Slug) ->
    Quote = maps:get(Slug, Quotes, #{}),
    case is_number(maps:get(lowest_sell, Quote, undefined)) of
        true -> {Priced + 1, Total + 1};
        false -> {Priced, Total + 1}
    end;
price_coverage_market(_Reward, _Quotes, Priced, Total) -> {Priced, Total}.

relic_name_parts(Name) ->
    case binary:split(Name, <<" ">>, [global, trim_all]) of
        [Era, _Code | Refinement] ->
            {string:lowercase(Era), iolist_to_binary(lists:join($\s, Refinement))};
        [Era | _] -> {string:lowercase(Era), <<>>};
        [] -> {<<>>, <<>>}
    end.

relic_asset(Unique, Relic) ->
    ImageName = case maps:get(image_name, Relic, undefined) of
        Existing when is_binary(Existing) -> Existing;
        _ -> relic_image_name_from_relic(maps:get(name, Relic, undefined), Unique)
    end,
    case ImageName of
        Name when is_binary(Name) ->
            #{<<"id">> => <<"relic:", Unique/binary>>,
              <<"source">> => <<"wfcd">>, <<"image_name">> => Name};
        _ -> null
    end.

relic_image_name_from_relic(Name, Unique) when is_binary(Name) ->
    case binary:split(Name, <<" ">>, [global, trim_all]) of
        [_Family, _Code | Parts] when Parts =/= [] ->
            relic_image_name(era(Name, Unique), lists:last(Parts));
        _ -> undefined
    end;
relic_image_name_from_relic(_Name, _Unique) -> undefined.

relic_image_name(Era, Refinement) ->
    Prefix = case Era of
        <<"lith">> -> <<"Lith">>;
        <<"meso">> -> <<"Meso">>;
        <<"neo">> -> <<"Neo">>;
        <<"axi">> -> <<"Axi">>;
        <<"requiem">> -> <<"Immortal">>;
        _ -> undefined
    end,
    Suffix = case Refinement of
        <<"Exceptional">> -> <<"A">>;
        <<"Flawless">> -> <<"B">>;
        <<"Radiant">> -> <<"C">>;
        <<"Intact">> -> <<"D">>;
        _ -> undefined
    end,
    case {Prefix, Suffix} of
        {P, S} when is_binary(P), is_binary(S) ->
            <<"Relic", P/binary, S/binary, ".png">>;
        _ -> undefined
    end.

public_reward(Reward, Items, Quotes, Owned) ->
    Slug = maps:get(slug, Reward, undefined),
    Name = maps:get(name, Reward, Slug),
    Item = maps:get(Slug, Items, #{}),
    Quote = maps:get(Slug, Quotes, #{}),
    GameRef = maps:get(<<"gameRef">>, Item, undefined),
    #{<<"name">> => nullable(Name),
      <<"slug">> => nullable(Slug),
      <<"rarity">> => nullable(maps:get(rarity, Reward, undefined)),
      <<"chance">> => maps:get(chance, Reward),
      <<"platinum">> => public_reward_platinum(Name, Quote),
      <<"ducats">> => value(maps:get(<<"ducats">>, Item, 0)),
      <<"owned">> => maps:get(GameRef, Owned, 0),
      <<"asset">> => reward_asset(Name, Slug, Item)}.

public_reward_platinum(Name, Quote) ->
    case wf_builtin_metadata:reward(Name) of
        #{platinum := Platinum} -> Platinum;
        undefined -> nullable_number(maps:get(lowest_sell, Quote, undefined))
    end.

reward_asset(Name, Slug, Item) ->
    case wf_builtin_metadata:reward(Name) of
        #{asset := Asset} -> Asset;
        undefined -> market_asset(Slug, Item)
    end.

market_asset(Slug, Item) when is_binary(Slug) ->
    English = maps:get(<<"en">>, maps:get(<<"i18n">>, Item, #{}), #{}),
    case maps:get(<<"subIcon">>, English,
                  maps:get(<<"thumb">>, English,
                           maps:get(<<"icon">>, English, undefined))) of
        Path when is_binary(Path) ->
            #{<<"id">> => <<"market:", Slug/binary>>,
              <<"source">> => <<"market">>, <<"image_name">> => Path};
        _ -> null
    end;
market_asset(_Slug, _Item) -> null.

nullable(undefined) -> null;
nullable(Value) -> Value.

nullable_number(Value) when is_integer(Value); is_float(Value) -> Value;
nullable_number(_Value) -> null.

reward_platinum(#{slug := undefined, name := Name}, _Quotes) ->
    case wf_builtin_metadata:reward(Name) of
        #{platinum := Platinum} -> Platinum;
        undefined -> 0
    end;
reward_platinum(#{slug := Slug}, Quotes) ->
    value(maps:get(lowest_sell, maps:get(Slug, Quotes, #{}), 0)).

reward_ducats(#{slug := undefined}, _Items) -> 0;
reward_ducats(#{slug := Slug}, Items) ->
    value(maps:get(<<"ducats">>, maps:get(Slug, Items, #{}), 0)).

is_forma(Name) when is_binary(Name) ->
    wf_builtin_metadata:reward(Name) =/= undefined;
is_forma(_Name) -> false.

value(Number) when is_integer(Number); is_float(Number) -> Number;
value(_Other) -> 0.

expected_max([], _ValueFun) -> 0;
expected_max(Rewards, ValueFun) ->
    Distribution = lists:foldl(
      fun(#{chance := Chance} = Reward, Acc) ->
          Value = ValueFun(Reward),
          Acc#{Value => maps:get(Value, Acc, 0.0) + Chance}
      end,
      #{},
      Rewards),
    Total = lists:sum(maps:values(Distribution)),
    case Total > 0 of
        false -> 0;
        true ->
            {_Cumulative, Expected} = lists:foldl(
              fun({Value, Chance}, {Cumulative, Acc}) ->
                  Next = Cumulative + Chance / Total,
                  Probability = math:pow(Next, 4) - math:pow(Cumulative, 4),
                  {Next, Acc + Value * Probability}
              end,
              {0.0, 0.0},
              lists:sort(maps:to_list(Distribution))),
            round(Expected)
    end.

owned_counts(PlayerSnapshot) ->
    Player = wf_player_projection:build(PlayerSnapshot),
    Entries = maps:get(<<"equipment">>, Player, []) ++
              maps:get(<<"items">>, Player, []) ++
              maps:get(<<"stacks">>, Player, []),
    lists:foldl(fun owned_entry/2, #{}, Entries).

owned_entry(#{<<"item_type">> := ItemType, <<"count">> := Count}, Acc)
  when is_binary(ItemType), is_integer(Count), Count > 0 ->
    Acc#{ItemType => maps:get(ItemType, Acc, 0) + Count};
owned_entry(_Entry, Acc) -> Acc.

unique([], Acc) -> lists:reverse(Acc);
unique([Value | Rest], Acc) ->
    case lists:member(Value, Acc) of
        true -> unique(Rest, Acc);
        false -> unique(Rest, [Value | Acc])
    end.
