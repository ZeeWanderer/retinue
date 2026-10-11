%%%-------------------------------------------------------------------
%% Presentation-neutral worldstate snapshot diffing.
%%%-------------------------------------------------------------------
-module(wf_worldstate_diff).

-export([diff/2, has_changes/1, entry_key/2]).

-doc "Compare canonical watch snapshots.".
-spec diff(map() | undefined, map()) -> map().
diff(undefined, Current) ->
    #{added => maps:to_list(Current), removed => [], changed => []};
diff(Previous, Current) ->
    Added = [{Key, maps:get(Key, Current)} || Key <- maps:keys(Current),
                                               not maps:is_key(Key, Previous)],
    Removed = [{Key, maps:get(Key, Previous)} || Key <- maps:keys(Previous),
                                                   not maps:is_key(Key, Current)],
    Changed = [{Key, maps:get(Key, Previous), maps:get(Key, Current)}
               || Key <- maps:keys(Current), maps:is_key(Key, Previous),
                  maps:get(value, maps:get(Key, Previous)) =/=
                      maps:get(value, maps:get(Key, Current))],
    #{added => Added, removed => Removed, changed => Changed}.

-doc "Return whether a canonical diff contains any change.".
-spec has_changes(map()) -> boolean().
has_changes(#{added := Added, removed := Removed, changed := Changed}) ->
    Added =/= [] orelse Removed =/= [] orelse Changed =/= [].

-doc "Return the stable watch key for one normalized entry.".
-spec entry_key(map(), non_neg_integer()) -> string().
entry_key(Entry, Index) ->
    case maps:get(id, Entry, undefined) of
        undefined -> retinue_text:to_list(maps:get(name, Entry, Index));
        Id -> retinue_text:to_list(Id)
    end.
