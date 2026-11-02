-module(wf_native).
-export([encode/2, event/2, decode_request/2]).

encode(Dataset, Data) ->
    case wf_native_views:encode(Dataset, Data) of
        dynamic -> retinue_native:encode(wf_native_pb, message(Dataset), Data, fun presentation/2);
        Reply -> Reply
    end.

presentation('Upgrade', #{<<"polarity">> := Value} = Upgrade) when is_integer(Value) ->
    Upgrade#{<<"polarity">> => atom_to_binary(wf_polarity:normalize(Value))};
presentation('FissureSettings', #{<<"filters">> := Filters} = Value) when is_list(Filters) ->
    Value#{<<"filters">> => #{<<"values">> => Filters}};
presentation(_, Value) -> Value.

message(<<"market">>) -> 'MarketReply';
message(<<"market_account">>) -> 'MarketAccount';
message(<<"market_presence">>) -> 'MarketPresence';
message(<<"assets">>) -> 'Assets';
message(<<"asset_cache">>) -> 'AssetCache';
message(<<"ducat_inventory">>) -> 'DucatInventory';
message(<<"relic_context">>) -> 'RelicContext';
message(<<"notifications">>) -> 'NotificationSettings';
message(<<"activity">>) -> 'Activity';
message(<<"player">>) -> 'PlayerSnapshot';
message(<<"game_metadata">>) -> 'PlayerSnapshot';
message(<<"overframe_account">>) -> 'OverframeAccount';
message(<<"build_items">>) -> 'BuildItems';
message(<<"builds">>) -> 'Builds';
message(<<"build_revision">>) -> 'BuildRevision';
message(<<"build_groups">>) -> 'BuildGroups';
message(<<"build_group">>) -> 'BuildGroup';
message(<<"build_plan">>) -> 'BuildPlan';
message(<<"daemon">>) -> 'DaemonIdentity';
message(<<"build_stats">>) -> 'Document';
message(Dataset) -> error({unsupported_native_dataset, Dataset}).

event(<<"dataset">>, Data) -> encode(<<"player">>, Data);
event(<<"asset">>, Data) -> retinue_native:encode(wf_native_pb, 'AssetDescriptor', Data);
event(<<"market_presence">>, Data) -> encode(<<"market_presence">>, Data);
event(<<"build_group">>, Data) -> encode(<<"build_group">>, Data).

decode_request(<<"retinue.warframe.native.v1.WarframeRequest">>, Bytes) ->
    Request0 = retinue_native:decode(wf_native_pb, 'WarframeRequest', Bytes),
    Request1 = case maps:take(<<"all">>, Request0) of
        {true, Rest} -> Rest#{<<"limit">> => <<"all">>};
        {_, Rest} -> Rest;
        error -> Request0
    end,
    Request = case maps:find(<<"patch">>, Request1) of
        {ok, Patch} ->
            case maps:take(<<"clear_instance">>, Patch) of
                {true, RestPatch} -> Request1#{<<"patch">> => RestPatch#{<<"instance_id">> => null}};
                {_, RestPatch} -> Request1#{<<"patch">> => RestPatch};
                error -> Request1
            end;
        error -> Request1
    end,
    {ok, case Request of
        #{<<"fissures">> := #{<<"filters">> := #{<<"values">> := Filters}} = Fissures} ->
            Request#{<<"fissures">> => Fissures#{<<"filters">> => Filters}};
        _ -> Request
    end};
decode_request(_, _) -> {error, wrong_request_schema}.
