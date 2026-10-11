%%%-------------------------------------------------------------------
%% Metadata gaps in daemon-owned projection trees.
%%%-------------------------------------------------------------------
-module(wfcli_resolution_audit).

-export([scan/1]).

-doc "Return unresolved names and assets from one projected value tree.".
-spec scan(term()) -> [map()].
scan(Value) ->
    maps:values(maps:from_list(
                  [{issue_key(Issue), Issue} || Issue <- walk(Value, [])])).

walk(Value, Acc) when is_list(Value) ->
    lists:foldl(fun walk/2, Acc, Value);
walk(Value, Acc) when is_map(Value) ->
    Acc1 = entity_issues(Value) ++ Acc,
    maps:fold(fun(_Key, Child, ChildAcc) -> walk(Child, ChildAcc) end,
              Acc1, Value);
walk(_Value, Acc) -> Acc.

entity_issues(Entity) ->
    case identity(Entity) of
        undefined -> [];
        Identity -> name_issue(Entity, Identity) ++ asset_issue(Entity, Identity)
    end.

name_issue(Entity, Identity) ->
    Source = first_present(
               [maps:get(<<"name_source">>, Entity, undefined),
                maps:get(<<"name">>, maps:get(<<"metadata_sources">>, Entity, #{}),
                         undefined)]),
    case Source of
        <<"path">> ->
            [(common(Entity, Identity))#{
               <<"kind">> => <<"friendly_name">>,
               <<"reason">> => <<"projection fell back to an internal identity">>,
               <<"attempts">> => [<<"catalog">>, <<"builtin">>, <<"path">>]}];
        _ -> []
    end.

asset_issue(Entity, Identity) ->
    case maps:find(<<"asset">>, Entity) of
        {ok, null} ->
            [(common(Entity, Identity))#{
               <<"kind">> => <<"asset">>,
               <<"reason">> => <<"projection has no resolvable asset descriptor">>,
               <<"attempts">> => [<<"catalog">>, <<"builtin">>]}];
        _ -> []
    end.

common(Entity, Identity) ->
    Name = first_present([maps:get(<<"name">>, Entity, undefined), Identity]),
    Class = first_present([maps:get(<<"class">>, Entity, undefined),
                           maps:get(<<"role">>, Entity, undefined),
                           maps:get(<<"group">>, Entity, undefined)]),
    Collection = first_present([maps:get(<<"collection">>, Entity, undefined),
                                maps:get(<<"category">>, Entity, undefined)]),
    compact(#{<<"identity">> => Identity, <<"fallback">> => Name,
              <<"class">> => Class, <<"collection">> => Collection}).

identity(Entity) ->
    first_present([maps:get(<<"item_type">>, Entity, undefined),
                   maps:get(<<"id">>, Entity, undefined),
                   maps:get(<<"unique_name">>, Entity, undefined)]).

first_present([]) -> undefined;
first_present([Value | Rest]) when Value =:= undefined; Value =:= null;
                                  Value =:= <<>> ->
    first_present(Rest);
first_present([Value | _Rest]) when is_binary(Value) -> Value;
first_present([_Value | Rest]) -> first_present(Rest).

compact(Map) -> maps:filter(fun(_Key, Value) -> Value =/= undefined end, Map).

issue_key(Issue) ->
    {maps:get(<<"kind">>, Issue), maps:get(<<"identity">>, Issue)}.
