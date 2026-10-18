-module(wf_ducat_inventory).

-export([build/4]).

build(Snapshot, Catalog, Market, Quotes) ->
    Index = wf_item_catalog:index(Catalog),
    ByRef = lists:foldl(fun(Item, Acc) ->
        case maps:get(<<"gameRef">>, Item, undefined) of
            Ref when is_binary(Ref) ->
                case lists:member(<<"set">>, maps:get(<<"tags">>, Item, [])) of
                    true -> Acc;
                    false -> maps:update_with(Ref, fun(Items) -> [Item | Items] end, [Item], Acc)
                end;
            _ -> Acc
        end
    end, #{}, Market),
    Items = wf_player_views:ducat_items(Snapshot, Index),
    Roots = lists:usort([maps:get(<<"parentUniqueName">>, maps:get(
        maps:get(<<"item_type">>, Item), Index, #{}), undefined) || Item <- Items]),
    {ok, Foundry} = wf_player_views:foundry(Snapshot,
        [Item || Item <- Catalog, lists:member(maps:get(<<"uniqueName">>, Item), Roots)]),
    Parents = maps:from_list([{maps:get(<<"id">>, Item), maps:with(
        [<<"name">>, <<"owned">>, <<"mastered">>, <<"subsumed">>], Item)}
        || Item <- maps:get(<<"items">>, Foundry)]),
    #{<<"items">> => [entry(Item, Index, ByRef, Parents, Quotes) || Item <- Items]}.

entry(Item, Index, ByRef, Parents, Quotes) ->
    Ref = maps:get(<<"item_type">>, Item),
    Catalog = maps:get(Ref, Index, #{}),
    Root = maps:get(<<"parentUniqueName">>, Catalog, undefined),
    Base = #{<<"id">> => Ref, <<"name">> => maps:get(<<"name">>, Item),
             <<"ducats">> => maps:get(<<"ducats">>, Item),
             <<"parent">> => maps:get(Root, Parents, null)},
    case maps:get(Ref, ByRef, []) of
        [Market] ->
            Slug = maps:get(<<"slug">>, Market),
            English = maps:get(<<"en">>, maps:get(<<"i18n">>, Market, #{}), #{}),
            Base#{<<"name">> => maps:get(<<"name">>, English, maps:get(<<"name">>, Item)),
                  <<"slug">> => Slug, <<"quote">> => cached_quote(Slug, Quotes)};
        _ -> Base#{<<"slug">> => null, <<"quote">> => null}
    end.

cached_quote(Slug, Quotes) ->
    case maps:find(Slug, Quotes) of
        {ok, Quote} -> maps:map(fun(_Key, undefined) -> null; (_Key, Value) -> Value end,
                               maps:with([lowest_sell, quoted_at, stale], Quote));
        error -> null
    end.
