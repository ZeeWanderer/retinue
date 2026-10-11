-module(wfcli_player_inventory).

-export([apply/4, boot_id/0]).

boot_id() ->
    case file:read_file("/proc/sys/kernel/random/boot_id") of
        {ok, Id} -> string:trim(Id);
        _ -> undefined
    end.

apply(Source, Observation, Sources, Boot) ->
    try
        Stamp = maps:get(<<"observation">>, Observation),
        true = valid_stamp(Stamp, Boot),
        true = is_integer(maps:get(<<"process_pid">>, Observation)),
        true = is_integer(maps:get(<<"collected_at">>, Observation)),
        false = key(maps:get(<<"sync">>, Observation)) =:= <<>>,
        update(Source, Observation, Stamp, Sources)
    catch
        error:_ -> {error, invalid_inventory_observation}
    end.

valid_stamp(#{<<"boot_id">> := Boot, <<"stream">> := Stream,
              <<"game_started">> := GameStarted, <<"generation">> := Generation,
              <<"sequence">> := Sequence, <<"baseline">> := Baseline,
              <<"started_ns">> := Start, <<"finished_ns">> := Finish}, Boot)
  when is_binary(Boot), byte_size(Boot) =:= 36,
       is_integer(Stream), Stream > 0, is_integer(GameStarted), GameStarted >= 0,
       is_integer(Generation), Generation > 0, is_integer(Sequence), Sequence > 0,
       is_integer(Baseline), Baseline > 0, Baseline =< Sequence,
       is_integer(Start), Start >= 0, is_integer(Finish), Finish >= Start -> true;
valid_stamp(_, _) -> false.

update(<<"inventory_http">>, Observation, Stamp, Sources) ->
    2 = maps:get(<<"schema">>, Observation),
    <<"native_http_buffer">> = maps:get(<<"collector">>, Observation),
    Raw = maps:get(<<"raw">>, Observation),
    true = is_map(Raw),
    true = key(maps:get(<<"LastInventorySync">>, Raw)) =:= key(maps:get(<<"sync">>, Observation)),
    true = maps:get(<<"sequence">>, Stamp) =:= maps:get(<<"baseline">>, Stamp),
    Previous = maps:get(<<"inventory_http">>, Sources, #{}),
    case newer(Stamp, maps:get(<<"observation">>, Previous, #{})) of
        false -> {ok, Sources};
        true ->
            {ok, (maps:remove(<<"inventory_native">>, Sources))#{
                <<"inventory_http">> => Observation, <<"inventory">> => Observation}}
    end;
update(<<"inventory_native">>, Observation, Stamp, Sources) ->
    1 = maps:get(<<"schema">>, Observation),
    <<"native_inventory">> = maps:get(<<"collector">>, Observation),
    Fields = maps:get(<<"fields">>, Observation),
    true = is_map(Fields) andalso map_size(Fields) =:= 3,
    lists:foreach(fun({Field, Identity}) -> validate_rows(Field, maps:get(Field, Fields), Identity) end,
                  scopes()),
    Baseline = maps:get(<<"inventory_http">>, Sources, #{}),
    Previous = maps:get(<<"inventory_native">>, Sources, #{}),
    Check = case baseline_check(Observation, Baseline) of
        ok -> case newer(Stamp, maps:get(<<"observation">>, Previous, #{})) of
                  true -> ok;
                  false -> stale_sequence
              end;
        Reason -> Reason
    end,
    case Check of
        ok ->
            Current = maps:get(<<"inventory">>, Sources),
            Raw = maps:get(<<"raw">>, Current),
            Merged = lists:foldl(fun({Field, Identity}, Acc) ->
                Acc#{Field => merge_rows(maps:get(Field, Raw, []), maps:get(Field, Fields), Identity,
                                        owned_fields(Field))}
            end, Raw, scopes()),
            Effective = case Merged =:= Raw of
                true -> Current;
                false -> Current#{<<"raw">> => Merged,
                                  <<"collector">> => <<"native_inventory">>,
                                  <<"collected_at">> => maps:get(<<"collected_at">>, Observation),
                                  <<"observation">> => Stamp}
            end,
            {ok, Sources#{<<"inventory_native">> => Observation, <<"inventory">> => Effective}};
        _ -> {ok, Sources}
    end.

newer(Stamp, #{<<"boot_id">> := Boot, <<"game_started">> := GameStarted,
               <<"stream">> := Stream, <<"sequence">> := Sequence}) ->
    maps:get(<<"boot_id">>, Stamp) =/= Boot orelse
    {maps:get(<<"game_started">>, Stamp), maps:get(<<"stream">>, Stamp), maps:get(<<"sequence">>, Stamp)} >
    {GameStarted, Stream, Sequence};
newer(_, _) -> true.

baseline_check(#{<<"observation">> := Stamp, <<"sync">> := Sync, <<"process_pid">> := Pid},
               #{<<"observation">> := Base, <<"sync">> := BaseSync, <<"process_pid">> := BasePid}) ->
    Identity = [<<"boot_id">>, <<"stream">>, <<"game_started">>, <<"generation">>],
    Checks = [{session_mismatch, Pid =:= BasePid andalso maps:with(Identity, Stamp) =:= maps:with(Identity, Base)},
              {baseline_mismatch, maps:get(<<"baseline">>, Stamp) =:= maps:get(<<"sequence">>, Base)},
              {stale_sequence, maps:get(<<"sequence">>, Stamp) > maps:get(<<"sequence">>, Base)},
              {pre_baseline, maps:get(<<"started_ns">>, Stamp) >= maps:get(<<"finished_ns">>, Base)},
              {sync_mismatch, key(Sync) =:= key(BaseSync)}],
    case [Reason || {Reason, false} <- Checks] of
        [] -> ok;
        [Reason | _] -> Reason
    end;
baseline_check(_, _) -> waiting_for_baseline.

scopes() -> [{<<"MiscItems">>, <<"ItemType">>}, {<<"Recipes">>, <<"ItemType">>},
             {<<"PendingRecipes">>, <<"ItemId">>}].

validate_rows(Field, Rows, Identity) when is_list(Rows) ->
    Limit = case Field of <<"PendingRecipes">> -> 512; _ -> 32768 end,
    true = length(Rows) =< Limit,
    Keys = [begin
        true = is_map(Row),
        Id = key(maps:get(Identity, Row)),
        true = byte_size(Id) > 0,
        case Field of
            <<"PendingRecipes">> -> ok;
            _ -> Count = maps:get(<<"ItemCount">>, Row),
                 true = is_integer(Count) andalso Count >= 0
        end,
        Id
    end || Row <- Rows],
    true = length(Keys) =:= map_size(maps:from_keys(Keys, true)).

owned_fields(<<"PendingRecipes">>) ->
    [<<"ItemId">>, <<"ItemType">>, <<"CompletionDate">>, <<"TargetItemId">>,
     <<"TargetFingerprint">>, <<"IngredientId">>];
owned_fields(_) -> [<<"ItemType">>, <<"ItemCount">>].

merge_rows(Previous, Rows, Identity, Owned) ->
    ById = maps:from_list([{key(maps:get(Identity, Row)), Row} || Row <- Rows]),
    {Retained, Remaining} = lists:foldl(fun(Row, {Acc, Pending}) ->
        case maps:take(key(maps:get(Identity, Row, <<>>)), Pending) of
            error -> {Acc, Pending};
            {Next, Rest} -> {[maps:merge(maps:without(Owned, Row), Next) | Acc], Rest}
        end
    end, {[], ById}, Previous),
    lists:reverse(Retained) ++ [Row || Row <- Rows, maps:is_key(key(maps:get(Identity, Row)), Remaining)].

key(#{<<"$oid">> := Id}) -> key(Id);
key(Id) when is_binary(Id) -> Id;
key(_) -> <<>>.
