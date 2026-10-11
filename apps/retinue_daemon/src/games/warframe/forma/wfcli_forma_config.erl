%%%-------------------------------------------------------------------
%% Load and validate forma-plan YAML configs.
%%%-------------------------------------------------------------------
-module(wfcli_forma_config).

-export([load_files/1]).

-define(ALLOWED_TYPES, [warframe, weapon, companion, melee, necramech]).

-type config() :: map().
-type filename() :: file:filename_all().

-doc "Load one or more forma-plan YAML files and return normalized, validated config maps.".
-spec load_files([filename()]) -> {ok, [config()]} | {error, [iodata()]}.
load_files(Files) ->
    Results = [load_file(File) || File <- Files],
    Errors = [Err || {error, Err} <- Results],
    case Errors of
        [] ->
            {ok, [Config || {ok, Config} <- Results]};
        _ ->
            {error, Errors}
    end.

load_file(File) ->
    case file:read_file(File) of
        {ok, Bin} ->
            parse_yaml(File, Bin);
        {error, Reason} ->
            {error, io_lib:format("~s: read error ~p", [File, Reason])}
    end.

parse_yaml(File, Bin) ->
    try yamerl_constr:string(Bin, yaml_opts()) of
        [Doc] ->
            validate(File, Doc);
        _ ->
            {error, io_lib:format("~s: expected single YAML document", [File])}
    catch
        Class:Reason ->
            {error, io_lib:format("~s: YAML parse failed ~p:~p", [File, Class, Reason])}
    end.

yaml_opts() ->
    [
        {map_node_format, map},
        {str_node_as_binary, true},
        {atom_node_style, plain}
    ].

validate(File, Doc) when is_map(Doc) ->
    Item = normalize_keys(maps:get(<<"item">>, Doc, maps:get(item, Doc, #{}))),
    Builds = normalize_keys_list(maps:get(<<"builds">>, Doc, maps:get(builds, Doc, []))),
    Constraints = normalize_keys(maps:get(<<"constraints">>, Doc, maps:get(constraints, Doc, #{}))),
    case {validate_item(Item), validate_builds(Builds), validate_constraints(Constraints)} of
        {ok, ok, ok} ->
            {ok, #{file => File, item => Item, builds => Builds, constraints => Constraints}};
        {ItemRes, BuildRes, ConRes} ->
            Errors = lists:flatten([
                format_errors(File, "item", ItemRes),
                format_errors(File, "builds", BuildRes),
                format_errors(File, "constraints", ConRes)
            ]),
            {error, Errors}
    end;
validate(File, _Other) ->
    {error, io_lib:format("~s: root must be a map", [File])}.

format_errors(_File, _Section, ok) -> [];
format_errors(File, Section, {error, List}) when is_list(List) ->
    [io_lib:format("~s (~s): ~s", [File, Section, Msg]) || Msg <- List];
format_errors(File, Section, {error, Msg}) ->
    [io_lib:format("~s (~s): ~s", [File, Section, Msg])].

validate_item(Map) when is_map(Map) ->
    Type = get_atom(Map, <<"type">>),
    Capacity = get_int(Map, <<"capacity">>),
    Errors0 = required(Type, "type"),
    Errors1 = required(Capacity, "capacity") ++ Errors0,
    Errors2 = case Type of
        undefined ->
            Errors1;
        T ->
            case lists:member(T, ?ALLOWED_TYPES) of
                true -> Errors1;
                false -> ["unsupported type (use warframe|weapon|companion|melee|necramech)" | Errors1]
            end
    end,
    case Errors2 of
        [] -> ok;
        _ -> {error, lists:reverse(Errors2)}
    end;
validate_item(_) ->
    {error, ["item must be a map"]}.

validate_builds(Builds) when is_list(Builds) ->
    Errors = lists:flatten(
        [validate_build(Build, Index) || {Build, Index} <- lists:zip(Builds, lists:seq(1, length(Builds)))]
    ),
    case Errors of
        [] -> ok;
        _ -> {error, Errors}
    end;
validate_builds(_) ->
    {error, ["builds must be a list"]}.

validate_build(Build, Index) when is_map(Build) ->
    Mods = normalize_keys_list(maps:get(<<"mods">>, Build, maps:get(mods, Build, []))),
    Arcanes = normalize_keys_list(maps:get(<<"arcanes">>, Build, maps:get(arcanes, Build, undefined))),
    Errors0 = case maps:get(<<"name">>, Build, maps:get(name, Build, undefined)) of
        undefined -> [io_lib:format("build #~B: missing name", [Index])];
        _ -> []
    end,
    Errors1 = case Mods of
        _ when is_list(Mods) -> [];
        _ -> [io_lib:format("build #~B: mods must be a list", [Index])]
    end,
    Errors2 = case Arcanes of
        undefined -> [];
        _ when is_list(Arcanes) -> [];
        _ -> [io_lib:format("build #~B: arcanes must be a list", [Index])]
    end,
    lists:flatten(Errors0 ++ Errors1 ++ Errors2);
validate_build(_, Index) ->
    [io_lib:format("build #~B: must be a map", [Index])].

validate_constraints(Map) when is_map(Map) ->
    Errors = case get_int(Map, <<"max_forma">>) of
        undefined -> [];
        N when N >= 0 -> [];
        _ -> ["constraints.max_forma must be >= 0"]
    end,
    case Errors of
        [] -> ok;
        _ -> {error, Errors}
    end;
validate_constraints(_) ->
    {error, ["constraints must be a map"]}.

normalize_keys(Map) when is_map(Map) ->
    lists:foldl(
      fun({K, V}, Acc) ->
          maps:put(normalize_key(K), V, Acc)
      end,
      #{},
      maps:to_list(Map)
    );
normalize_keys(Other) ->
    Other.

normalize_keys_list(List) when is_list(List) ->
    [normalize_keys(E) || E <- List];
normalize_keys_list(Other) ->
    Other.

normalize_key(K) when is_atom(K) -> atom_to_binary(K, utf8);
normalize_key(K) when is_binary(K) -> K;
normalize_key(K) when is_list(K) -> list_to_binary(K);
normalize_key(K) -> list_to_binary(io_lib:format("~p", [K])).

get_atom(Map, Key) ->
    case maps:get(Key, Map, undefined) of
        undefined -> undefined;
        Bin when is_binary(Bin) ->
            normalize_atom(binary_to_list(Bin));
        Atom when is_atom(Atom) -> Atom;
        Str when is_list(Str) -> normalize_atom(Str);
        _ -> undefined
    end.

normalize_atom(Str) when is_list(Str) ->
    case string:lowercase(Str) of
        "warframe" -> warframe;
        "weapon" -> weapon;
        "companion" -> companion;
        "melee" -> melee;
        "necramech" -> necramech;
        _ -> undefined
    end.

get_int(Map, Key) ->
    case maps:get(Key, Map, undefined) of
        undefined -> undefined;
        N when is_integer(N) -> N;
        Bin when is_binary(Bin) ->
            case string:to_integer(binary_to_list(Bin)) of
                {Int, _} -> Int;
                _ -> undefined
            end;
        _ -> undefined
    end.

required(undefined, Field) ->
    [io_lib:format("missing ~s", [Field])];
required(_, _Field) -> [].
