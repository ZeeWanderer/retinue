-module(wf_native_views).
-export([encode/2]).

encode(Dataset, Data) ->
    case message(Dataset) of
        undefined -> dynamic;
        Type ->
            validate(Type, Data),
            try
                Selected = wf_views_pb:from_json(Data, Type),
                {ok, wf_views_pb:msg_name_to_fqbin(Type), wf_views_pb:encode_msg(Selected, Type)}
            catch error:_ -> error({invalid_native_view, Type})
            end
    end.

message(<<"foundry">>) -> 'FoundrySnapshot';
message(<<"inventory">>) -> 'ListingSnapshot';
message(<<"mastery">>) -> 'ListingSnapshot';
message(<<"relics">>) -> 'ListingSnapshot';
message(<<"build_equipment">>) -> 'Equipment';
message(_) -> undefined.

validate('Equipment', #{<<"definitions">> := Definitions, <<"instances">> := Instances})
  when is_list(Definitions), is_list(Instances) -> ok;
validate(Type, #{<<"items">> := Items}) when Type =/= 'Equipment', is_list(Items) -> ok;
validate(Type, _) -> error({invalid_native_view, Type}).
