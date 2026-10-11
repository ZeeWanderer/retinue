%%%-------------------------------------------------------------------
%% Datasource-agnostic entity builder with sparse fields.
%%%-------------------------------------------------------------------
-module(wf_entity).

-export([build/6, collect_strings/1]).

build(Type, Id, Name, Data, Opts, Spec) ->
    Entry0 = #{type => Type, id => Id, name => Name, data => Data},
    RowMap = row_map(Entry0, Opts, Spec),
    ToList = to_list_fun(Spec),
    Extras = extra_fields(Data, RowMap, ToList, Opts, Spec),
    FieldValues = field_values(RowMap, Extras, ToList),
    Haystack = haystack(Data, FieldValues, Opts, Spec, ToList),
    Entry0#{row_map => RowMap, extra_fields => Extras, fields => FieldValues, haystack => Haystack}.

row_map(Entry, Opts, Spec) ->
    case maps:get(row_map_fun, Spec, undefined) of
        Fun when is_function(Fun, 2) -> Fun(Entry, Opts);
        Fun when is_function(Fun, 1) -> Fun(Entry);
        _ -> #{}
    end.

to_list_fun(Spec) ->
    case maps:get(to_list_fun, Spec, undefined) of
        Fun when is_function(Fun, 1) -> Fun;
        _ -> fun retinue_text:to_list/1
    end.

extra_fields(Data, RowMap, ToList, Opts, Spec) when is_map(Data) ->
    Core = [string:lowercase(atom_to_list(K)) || K <- maps:keys(RowMap)],
    lists:foldl(
      fun({K, V}, Acc) ->
          Key = key_string(K),
          case include_extra(Key, V, Core) of
              true ->
                  case extra_field_value(Key, V, Opts, Spec) of
                      skip -> Acc;
                      {keep, Key1, V1} ->
                          case retinue_text:value_present(V1) of
                              true -> maps:put(Key1, ToList(V1), Acc);
                              false -> Acc
                          end
                  end;
              false -> Acc
          end
      end,
      #{},
      maps:to_list(Data));
extra_fields(_, _RowMap, _ToList, _Opts, _Spec) ->
    #{}.

include_extra(undefined, _V, _Core) -> false;
include_extra(_Key, V, _Core) when is_map(V) -> false;
include_extra(_Key, V, _Core) when is_list(V) -> false;
include_extra(Key, V, Core) ->
    Lower = string:lowercase(Key),
    not lists:member(Lower, Core) andalso
    not skip_key(Lower) andalso
    retinue_text:value_present(V).

skip_key("_id") -> true;
skip_key("id") -> true;
skip_key([$_ | _]) -> true;
skip_key(_) -> false.

key_string(K) when is_binary(K) -> binary_to_list(K);
key_string(K) when is_atom(K) -> atom_to_list(K);
key_string(K) when is_list(K) -> K;
key_string(_) -> undefined.

extra_field_value(Key, V, Opts, Spec) ->
    case maps:get(extra_field_fun, Spec, undefined) of
        Fun when is_function(Fun, 3) -> Fun(Key, V, Opts);
        _ -> {keep, Key, V}
    end.

field_values(RowMap, Extras, ToList) ->
    RowVals = [ToList(V) || {_K, V} <- maps:to_list(RowMap)],
    ExtraVals = [ToList(V) || {_K, V} <- maps:to_list(Extras)],
    [V || V <- RowVals ++ ExtraVals, retinue_text:value_present(V)].

haystack(Data, FieldValues, Opts, Spec, ToList) ->
    LowerFields = [string:lowercase(S) || S <- FieldValues],
    case maps:get(search_raw, Opts, false) of
        true ->
            Raw = collect_strings(Data),
            LowerRaw = [string:lowercase(S) || S <- Raw],
            LowerResolved = resolve_strings(Raw, Opts, Spec, ToList),
            string:join(LowerRaw ++ LowerResolved ++ LowerFields, "|");
        false ->
            string:join(LowerFields, "|")
    end.

resolve_strings(Raw, Opts, Spec, ToList) ->
    case maps:get(resolve_strings_fun, Spec, undefined) of
        Fun when is_function(Fun, 2) ->
            [string:lowercase(ToList(V))
             || S <- Raw,
                V <- [Fun(S, Opts)],
                retinue_text:value_present(V)];
        _ -> []
    end.

collect_strings(Value) ->
    collect_strings(Value, []).

collect_strings(Map, Acc) when is_map(Map) ->
    lists:foldl(fun({K, V}, A) -> collect_strings(V, collect_strings(K, A)) end, Acc, maps:to_list(Map));
collect_strings(List, Acc) when is_list(List) ->
    lists:foldl(fun(E, A) -> collect_strings(E, A) end, Acc, List);
collect_strings(V, Acc) when is_binary(V) -> [binary_to_list(V) | Acc];
collect_strings(V, Acc) when is_atom(V) -> [atom_to_list(V) | Acc];
collect_strings(V, Acc) when is_integer(V) -> [integer_to_list(V) | Acc];
collect_strings(V, Acc) when is_float(V) -> [lists:flatten(io_lib:format("~p", [V])) | Acc];
collect_strings(_, Acc) -> Acc.
