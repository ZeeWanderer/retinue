-module(retinue_poe2_trade_query).
-compile({no_auto_import, [error/1, error/2]}).
-export([build/4, options/1, search_result/2, listings/1, error/1, error/2]).

build(Request, Item, Leagues, Options) ->
    try
        require(not maps:is_key(<<"trade_unavailable">>, Item), item_not_in_trade_catalog),
        League = maps:get(<<"league">>, Request, <<>>),
        require(lists:any(fun(#{<<"id">> := Id}) -> Id =:= League end, Leagues), invalid_league),
        Status = option(<<"status">>, Request, Options, <<"securable">>),
        Base = maps:get(<<"base">>, Request, true),
        require(is_boolean(Base), invalid_search_options),
        Sort = maps:get(<<"sort">>, Request, <<"asc">>),
        require(lists:member(Sort, [<<"asc">>, <<"desc">>]), invalid_search_options),
        Filters = maps:get(<<"filters">>, Request, []),
        require(is_list(Filters) andalso length(Filters) =< 32, invalid_filters),
        Rows = maps:get(<<"modifiers">>, Item),
        Resolved = [filter(F, Rows) || F <- Filters],
        Ids = [maps:get(<<"id">>, F) || F <- Filters],
        require(length(lists:usort(Ids)) =:= length(Ids), duplicate_filter),
        Stats = [F || {stat, F} <- Resolved],
        Rarity = maps:get(<<"rarity">>, Item),
        Name = maps:get(<<"name">>, Item),
        require(Rarity =/= <<"Unique">> orelse Name =/= <<>>, unidentified_unique),
        Misc = conditions(Request, Item),
        Types0 = type_filters(Rarity),
        Types = case maps:get(<<"category">>, Item, null) of
            null -> Types0;
            Category -> Types0#{<<"category">> => #{<<"option">> => Category}}
        end,
        SelectedRarity = case lists:member(Rarity, [<<"Normal">>, <<"Magic">>, <<"Rare">>]) of
            true -> option(<<"rarity">>, Request, Options, string:lowercase(Rarity));
            false -> require(not maps:is_key(<<"rarity">>, Request), invalid_search_options), undefined
        end,
        TypeFilters = case SelectedRarity of
            undefined -> Types;
            null -> maps:remove(<<"rarity">>, Types);
            _ -> Types#{<<"rarity">> => #{<<"option">> => SelectedRarity}}
        end,
        Price = optional_option(<<"currency">>, Request, Options, #{<<"min">> => 0}),
        Trade0 = #{<<"price">> => Price, <<"collapse">> => #{<<"option">> => <<"true">>}},
        Trade = case option(<<"indexed">>, Request, Options, null) of
            null -> Trade0;
            Age -> Trade0#{<<"indexed">> => #{<<"option">> => Age}}
        end,
        MiscWithLevel = case maps:get(<<"item_level">>, Request, null) of
            null -> Misc;
            Range when is_map(Range) ->
                require(maps:is_key(<<"item_level">>, Item), unsupported_filter),
                require(maps:keys(Range) -- [<<"min">>, <<"max">>] =:= [], invalid_bounds),
                MinLevel = maps:get(<<"min">>, Range, null),
                MaxLevel = maps:get(<<"max">>, Range, null),
                require(level_bound(MinLevel) andalso level_bound(MaxLevel), invalid_bounds),
                require(MinLevel =:= null orelse MaxLevel =:= null orelse MinLevel =< MaxLevel, invalid_bounds),
                Misc#{<<"ilvl">> => Range};
            _ -> throw(invalid_bounds)
        end,
        Query0 = #{<<"status">> => #{<<"option">> => Status},
                   <<"stats">> => stat_groups(Stats),
                   <<"filters">> => #{
                       <<"type_filters">> => #{<<"filters">> => TypeFilters},
                       <<"trade_filters">> => #{<<"filters">> => Trade},
                       <<"misc_filters">> => #{<<"filters">> => MiscWithLevel}}},
        QueryWithProperties = lists:foldl(fun
            ({property, Group, Field, Value}, Q) ->
                Sections = maps:get(<<"filters">>, Q),
                Section = maps:get(Group, Sections, #{<<"filters">> => #{}}),
                Values = maps:get(<<"filters">>, Section),
                Q#{<<"filters">> => Sections#{Group => Section#{<<"filters">> => Values#{Field => Value}}}};
            ({stat, _}, Q) -> Q
        end, Query0, Resolved),
        Query1 = case Base orelse retinue_poe2_trade_profiles:base_required(Item) of
            true -> QueryWithProperties#{<<"type">> => maps:get(<<"base">>, Item)};
            false -> QueryWithProperties
        end,
        Query = case Rarity of <<"Unique">> -> Query1#{<<"name">> => Name}; _ -> Query1 end,
        {ok, #{<<"query">> => Query, <<"sort">> => #{<<"price">> => Sort}}}
    catch
        throw:Reason -> error(Reason);
        _:_ -> error(invalid_filters)
    end.

stat_groups(Stats) ->
    {Singles, Alternatives} = lists:partition(fun(#{<<"id">> := Id}) ->
        binary:match(Id, <<",">>) =:= nomatch
    end, Stats),
    [#{<<"type">> => <<"and">>, <<"filters">> => Singles} |
     [#{<<"type">> => <<"count">>, <<"value">> => #{<<"min">> => 1},
        <<"filters">> => [Filter#{<<"id">> => Id}
            || Id <- binary:split(maps:get(<<"id">>, Filter), <<",">>, [global])]} || Filter <- Alternatives]].

options(#{<<"result">> := Groups}) when is_list(Groups) ->
    try
        Pairs = [{<<"status">>, <<"status_filters">>, <<"status">>},
                 {<<"rarity">>, <<"type_filters">>, <<"rarity">>},
                 {<<"currency">>, <<"trade_filters">>, <<"price">>},
                 {<<"indexed">>, <<"trade_filters">>, <<"indexed">>}],
        {ok, maps:from_list([begin
            [Options] = [Rows || #{<<"id">> := G, <<"filters">> := Filters} <- Groups, G =:= Group,
                #{<<"id">> := F, <<"option">> := #{<<"options">> := Rows}} <- Filters, F =:= Field],
            true = is_list(Options) andalso Options =/= [] andalso length(Options) =< 256,
            true = lists:all(fun
                (#{<<"id">> := Id, <<"text">> := Text}) ->
                    (Id =:= null orelse (is_binary(Id) andalso byte_size(Id) =< 128))
                    andalso is_binary(Text) andalso byte_size(Text) =< 256;
                (_) -> false
            end, Options),
            {Name, [maps:with([<<"id">>, <<"text">>], Row) || Row <- Options]}
        end || {Name, Group, Field} <- Pairs])}
    catch _:_ -> {error, invalid_filter_metadata} end;
options(_) -> {error, invalid_filter_metadata}.

option(Key, Request, Options, Default) ->
    Value = maps:get(Key, Request, Default),
    require(lists:any(fun(#{<<"id">> := Id}) -> Id =:= Value end,
                      maps:get(Key, Options)), invalid_search_options),
    Value.

optional_option(Key, Request, Options, Base) ->
    case option(Key, Request, Options, null) of
        null -> Base;
        Value -> Base#{<<"option">> => Value}
    end.

level_bound(null) -> true;
level_bound(N) -> is_integer(N) andalso N >= 1 andalso N =< 100.

conditions(Request, Item) ->
    Available = maps:from_list([{Id, Filters} || #{<<"id">> := Id, <<"filters">> := Filters}
        <- maps:get(<<"conditions">>, Item), Id =/= null]),
    Selected = maps:get(<<"conditions">>, Request, maps:keys(Available)),
    require(is_list(Selected) andalso length(Selected) =< map_size(Available), invalid_conditions),
    require(lists:usort(Selected) =:= lists:sort(Selected), invalid_conditions),
    lists:foldl(fun(Id, Acc) ->
        require(maps:is_key(Id, Available), invalid_conditions),
        maps:merge(Acc, maps:map(fun(_, Value) -> #{<<"option">> => boolean(Value)} end,
                               maps:get(Id, Available)))
    end, #{}, Selected).

type_filters(<<"Gem">>) -> #{<<"category">> => #{<<"option">> => <<"gem">>}};
type_filters(<<"Currency">>) -> #{};
type_filters(<<"Quest">>) -> #{};
type_filters(Rarity) -> #{<<"rarity">> => #{<<"option">> => string:lowercase(Rarity)}}.

filter(F, Rows) when is_map(F) ->
    require(maps:keys(F) -- [<<"id">>, <<"min">>, <<"max">>] =:= [], invalid_filter),
    Id = maps:get(<<"id">>, F, undefined),
    Matches = [R || R = #{<<"id">> := RowId, <<"supported">> := true} <- Rows, RowId =:= Id],
    require(Matches =/= [], unsupported_filter),
    Min = maps:get(<<"min">>, F, null),
    Max = maps:get(<<"max">>, F, null),
    require(bound(Min) andalso bound(Max), invalid_bounds),
    require(Min =:= null orelse Max =:= null orelse Min =< Max, invalid_bounds),
    [Row | _] = Matches,
    require(maps:get(<<"min">>, Row) =/= null orelse (Min =:= null andalso Max =:= null),
            non_numeric_filter),
    Value = maps:filter(fun(_, V) -> V =/= null end, #{<<"min">> => Min, <<"max">> => Max}),
    case Id of
        <<"property.gem_level">> -> {property, <<"misc_filters">>, <<"gem_level">>, Value};
        <<"property.gem_sockets">> -> {property, <<"misc_filters">>, <<"gem_sockets">>, Value};
        _ -> case retinue_poe2_trade_properties:filter(Id) of
            {Group, Field} -> {property, Group, Field, Value};
            undefined ->
                require(not lists:prefix("property.", binary_to_list(Id)), unsupported_filter),
                {stat, #{<<"id">> => Id, <<"value">> => Value, <<"disabled">> => false}}
        end
    end;
filter(_, _) -> throw(invalid_filter).

bound(null) -> true;
bound(N) -> retinue_poe2_trade_item:sane_number(N).
boolean(true) -> <<"true">>;
boolean(false) -> <<"false">>.
require(true, _) -> ok;
require(false, Reason) -> throw(Reason).

search_result(#{<<"id">> := Id, <<"total">> := Total, <<"result">> := Ids}, League)
  when is_binary(Id), is_integer(Total), Total >= 0, is_list(Ids), length(Ids) =< 10000 ->
    case token(Id, 1024) andalso lists:all(fun(I) -> token(I, 128) end, Ids) of
        true ->
            Url = <<"https://www.pathofexile.com/trade2/search/poe2/",
                    (quote(League))/binary, "/", (quote(Id))/binary>>,
            {ok, #{<<"search_id">> => Id, <<"url">> => Url, <<"total">> => Total,
                   <<"returned">> => length(Ids),
                   <<"next_offset">> => case Ids of [] -> null; _ -> 0 end,
                   <<"listings">> => []}, Ids};
        false -> error(invalid_search_response)
    end;
search_result(_, _) -> error(invalid_search_response).

token(Bin, Max) when is_binary(Bin), byte_size(Bin) > 0, byte_size(Bin) =< Max ->
    re:run(Bin, <<"^[A-Za-z0-9_-]+$">>) =/= nomatch;
token(_, _) -> false.
quote(Bin) -> uri_string:quote(Bin).

listings(#{<<"result">> := Rows}) when is_list(Rows), length(Rows) =< 10 ->
    {ok, lists:filtermap(fun listing/1, Rows)};
listings(_) -> error(invalid_fetch_response).

listing(#{<<"id">> := Id, <<"item">> := Item, <<"listing">> := Listing})
  when is_map(Item), is_map(Listing) ->
    Price = maps:get(<<"price">>, Listing, #{}),
    Account = maps:get(<<"account">>, Listing, #{}),
    case {Price, Account} of
        {#{<<"amount">> := Amount, <<"currency">> := Currency}, #{<<"name">> := AccountName}}
          when is_number(Amount), Amount > 0, is_binary(Currency), is_binary(AccountName) ->
            {true, #{<<"id">> => Id, <<"name">> => maps:get(<<"name">>, Item, <<>>),
                     <<"base">> => maps:get(<<"baseType">>, Item, maps:get(<<"typeLine">>, Item, <<>>)),
                     <<"item_level">> => item_level(maps:get(<<"ilvl">>, Item, null)),
                     <<"gem_level">> => gem_level(Item),
                     <<"required_level">> => required_level(Item),
                     <<"gem_sockets">> => case maps:get(<<"sockets">>, Item, null) of
                         Sockets when is_list(Sockets) -> length(Sockets);
                         _ -> null
                     end,
                     <<"item">> => retinue_poe2_trade_item:from_listing(Item),
                     <<"type">> => text(maps:get(<<"type">>, Listing, <<>>)),
                     <<"quality">> => quality(Item),
                     <<"amount">> => Amount, <<"currency">> => Currency,
                     <<"account">> => AccountName,
                     <<"online">> => is_map(maps:get(<<"online">>, Account, null)),
                     <<"indexed">> => text(maps:get(<<"indexed">>, Listing, <<>>)),
                     <<"indexed_at">> => case retinue_time:parse(maps:get(<<"indexed">>, Listing, null)) of
                         {ok, Millis} -> Millis div 1000;
                         error -> null
                     end}};
        _ -> false
    end;
listing(_) -> false.

quality(Item) ->
    case [Value || #{<<"name">> := <<"Quality">>, <<"values">> := [[Value, _] | _]} <-
                      list_field(Item, <<"properties">>), is_binary(Value)] of
        [Value | _] ->
            case re:run(Value, <<"^\\+?([0-9]+)%">>, [{capture, [1], binary}]) of
                {match, [N]} -> binary_to_integer(N);
                nomatch -> 0
            end;
        [] -> 0
    end.

gem_level(#{<<"frameType">> := 4, <<"properties">> := Properties}) when is_list(Properties) ->
    case [Value || #{<<"type">> := 5, <<"values">> := [[Value, _] | _]} <- Properties,
                   is_binary(Value)] of
        [Value] ->
            case re:run(Value, <<"^([0-9]+)(?:$| )">>, [{capture, [1], binary}]) of
                {match, [Number]} -> item_level(binary_to_integer(Number));
                nomatch -> null
            end;
        _ -> null
    end;
gem_level(_) -> null.

required_level(Item) ->
    Requirements = list_field(Item, <<"requirements">>),
    case [Value || #{<<"name">> := <<"Level">>, <<"values">> := [[Value, _] | _]} <- Requirements,
                   is_binary(Value)] of
        [Value | _] ->
            case re:run(Value, <<"^([0-9]+)(?:$| )">>, [{capture, [1], binary}]) of
                {match, [Number]} -> item_level(binary_to_integer(Number));
                nomatch -> null
            end;
        [] -> null
    end.

list_field(Map, Key) ->
    case maps:get(Key, Map, []) of List when is_list(List) -> List; _ -> [] end.

item_level(N) when is_integer(N), N >= 0, N =< 100 -> N;
item_level(_) -> null.
text(Bin) when is_binary(Bin) -> Bin;
text(_) -> <<>>.

error(Reason) -> {error, #{<<"reason">> => atom_to_binary(Reason)}}.
error(Reason, Seconds) ->
    {error, #{<<"reason">> => atom_to_binary(Reason), <<"retry_after_seconds">> => Seconds}}.
