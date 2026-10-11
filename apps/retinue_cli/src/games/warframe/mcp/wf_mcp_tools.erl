%%%-------------------------------------------------------------------
%% Structured MCP tools backed exclusively by retinue-daemon requests.
%%%-------------------------------------------------------------------
-module(wf_mcp_tools).

-export([definitions/0, call/2]).

-type tool_result() :: {ok, term()} | {error, term()}.

-spec definitions() -> [map()].
definitions() ->
    [query_definition(), forma_definition(), status_definition(), update_definition()].

-spec call(binary(), map()) -> tool_result().
call(Name, Args) when is_binary(Name), is_map(Args) ->
    try call_tool(Name, Args)
    catch
        throw:{invalid_argument, Reason} -> {error, {invalid_arguments, Reason}};
        Class:Reason:Stack -> {error, {mcp_tool_crash, Class, Reason, Stack}}
    end;
call(_Name, _Args) ->
    {error, {invalid_arguments, object_required}}.

call_tool(<<"query">>, Args) ->
    Query = required_string(<<"query">>, Args),
    Datasets = dataset_selector(maps:get(<<"datasets">>, Args, [])),
    Tokens = case Datasets of
        [] -> [binary_to_list(Query)];
        _ -> ["dataset=" ++ string:join(Datasets, "|") , binary_to_list(Query)]
    end,
    Cwd = cwd(Args),
    Request0 = #{source => query, query_tokens => Tokens, cwd => Cwd, utc => true},
    Request1 = optional_boolean(<<"refresh">>, refresh, Args, Request0),
    Request2 = optional_boolean(<<"raw">>, raw, Args, Request1),
    Request3 = optional_integer(<<"ttl">>, ttl, 60, Args, Request2),
    Request4 = optional_integer(<<"limit">>, limit, 0, Args, Request3),
    Request5 = optional_integer(<<"offset">>, offset, 0, Args, Request4),
    Request6 = optional_path(<<"cache">>, cache, Cwd, Args, Request5),
    Request7 = optional_path(<<"exports_dir">>, exports_dir, Cwd, Args, Request6),
    Request8 = optional_path(<<"knowledge_dir">>, knowledge_dir, Cwd, Args, Request7),
    Request = optional_string(<<"language">>, event_lang, Args, Request8),
    retinue_client:one_shot(warframe, Request);
call_tool(<<"forma_plan">>, Args) ->
    Cwd = cwd(Args),
    Configs = required_paths(<<"configs">>, Args, Cwd),
    Flags0 = #{},
    Flags1 = optional_boolean(<<"allow_omni">>, allow_omni, Args, Flags0),
    Flags2 = optional_boolean(<<"prefer_omni">>, prefer_omni, Args, Flags1),
    Flags3 = optional_boolean(<<"allow_umbral_forma">>, allow_umbral_forma, Args, Flags2),
    Flags = optional_integer(<<"max_forma">>, max_forma, 0, Args, Flags3),
    retinue_client:one_shot(warframe, #{source => forma, configs => Configs, flags => Flags});
call_tool(<<"daemon_status">>, _Args) ->
    retinue_client:call(status);
call_tool(<<"update_knowledge">>, Args) ->
    Selections = update_selections(maps:get(<<"selections">>, Args, [<<"default">>])),
    retinue_client:one_shot(warframe, #{source => metadata, action => refresh, selections => Selections});
call_tool(Name, _Args) ->
    {error, {unknown_tool, Name}}.

query_definition() ->
    #{<<"name">> => <<"query">>,
      <<"title">> => <<"Query Warframe data">>,
      <<"description">> =>
          <<"Run the shared wfcli query language in retinue-daemon and return canonical structured results.">>,
      <<"inputSchema">> =>
          object_schema(
            #{<<"query">> => string_schema(<<"Query expression; adjacent terms imply AND and uppercase OR is explicit.">>),
              <<"datasets">> => array_schema(enum_schema(dataset_names()),
                                               <<"Datasets to select; omit for daemon defaults.">>),
              <<"limit">> => integer_schema(0, <<"Maximum matches per dataset; omit for unlimited.">>),
              <<"offset">> => integer_schema(0, <<"Matches to skip per dataset.">>),
              <<"raw">> => boolean_schema(<<"Preserve raw identifiers and values.">>),
              <<"refresh">> => boolean_schema(<<"Force a worldstate network refresh.">>),
              <<"ttl">> => integer_schema(60, <<"Worldstate cache TTL in seconds.">>),
              <<"language">> => string_schema(<<"Worldstate event language code.">>),
              <<"cache">> => string_schema(<<"Optional worldstate cache path.">>),
              <<"exports_dir">> => string_schema(<<"Optional PublicExport directory.">>),
              <<"knowledge_dir">> => string_schema(<<"Optional WFCD knowledge directory.">>),
              <<"cwd">> => string_schema(<<"Base directory for relative paths.">>)},
            [<<"query">>]),
      <<"annotations">> => annotations(true, true, true)}.

forma_definition() ->
    #{<<"name">> => <<"forma_plan">>,
      <<"title">> => <<"Plan Forma polarities">>,
      <<"description">> =>
          <<"Queue an optimal Forma plan in retinue-daemon and return the unformatted planner result.">>,
      <<"inputSchema">> =>
          object_schema(
            #{<<"configs">> => array_schema(string_schema(<<"YAML configuration path.">>),
                                              <<"One or more planner configuration files.">>),
              <<"allow_omni">> => boolean_schema(<<"Allow Omni Forma.">>),
              <<"prefer_omni">> => boolean_schema(<<"Prefer Omni Forma when costs tie.">>),
              <<"allow_umbral_forma">> => boolean_schema(<<"Allow Umbral Forma.">>),
              <<"max_forma">> => integer_schema(0, <<"Maximum normal Forma cost.">>),
              <<"cwd">> => string_schema(<<"Base directory for relative config paths.">>)},
            [<<"configs">>]),
      <<"annotations">> => annotations(true, true, false)}.

status_definition() ->
    #{<<"name">> => <<"daemon_status">>,
      <<"title">> => <<"Inspect retinue-daemon">>,
      <<"description">> => <<"Ensure retinue-daemon is running and return its structured status.">>,
      <<"inputSchema">> => object_schema(#{}, []),
      <<"annotations">> => annotations(true, true, false)}.

update_definition() ->
    #{<<"name">> => <<"update_knowledge">>,
      <<"title">> => <<"Refresh Warframe knowledge">>,
      <<"description">> =>
          <<"Refresh daemon-managed node, language, PublicExport, or WFCD knowledge caches.">>,
      <<"inputSchema">> =>
          object_schema(
            #{<<"selections">> =>
                  array_schema(enum_schema(update_names()),
                               <<"Sources to refresh; defaults to all managed public metadata.">>)}, []),
      <<"annotations">> => annotations(false, true, true)}.

annotations(ReadOnly, Idempotent, OpenWorld) ->
    #{<<"readOnlyHint">> => ReadOnly,
      <<"destructiveHint">> => false,
      <<"idempotentHint">> => Idempotent,
      <<"openWorldHint">> => OpenWorld}.

object_schema(Properties, Required) ->
    #{<<"type">> => <<"object">>, <<"properties">> => Properties,
      <<"required">> => Required, <<"additionalProperties">> => false}.

string_schema(Description) ->
    #{<<"type">> => <<"string">>, <<"description">> => Description}.

boolean_schema(Description) ->
    #{<<"type">> => <<"boolean">>, <<"description">> => Description}.

integer_schema(Minimum, Description) ->
    #{<<"type">> => <<"integer">>, <<"minimum">> => Minimum,
      <<"description">> => Description}.

enum_schema(Values) ->
    #{<<"type">> => <<"string">>, <<"enum">> => Values}.

array_schema(Items, Description) ->
    #{<<"type">> => <<"array">>, <<"items">> => Items,
      <<"description">> => Description}.

required_string(Key, Args) ->
    case maps:get(Key, Args, undefined) of
        Value when is_binary(Value), byte_size(Value) > 0 -> Value;
        _ -> invalid(Key, non_empty_string_required)
    end.

required_paths(Key, Args, Cwd) ->
    case maps:get(Key, Args, undefined) of
        Values when is_list(Values), Values =/= [] ->
            [absolute_path(Value, Cwd) || Value <- Values];
        _ -> invalid(Key, non_empty_string_array_required)
    end.

absolute_path(Value, Cwd) when is_binary(Value), byte_size(Value) > 0 ->
    filename:absname(binary_to_list(Value), Cwd);
absolute_path(_Value, _Cwd) ->
    throw({invalid_argument, non_empty_path_required}).

cwd(Args) ->
    case maps:get(<<"cwd">>, Args, undefined) of
        undefined -> filename:absname(".");
        Value when is_binary(Value), byte_size(Value) > 0 -> filename:absname(binary_to_list(Value));
        _ -> invalid(<<"cwd">>, non_empty_string_required)
    end.

optional_boolean(JsonKey, Key, Args, Acc) ->
    case maps:get(JsonKey, Args, undefined) of
        undefined -> Acc;
        Value when is_boolean(Value) -> Acc#{Key => Value};
        _ -> invalid(JsonKey, boolean_required)
    end.

optional_integer(JsonKey, Key, Minimum, Args, Acc) ->
    case maps:get(JsonKey, Args, undefined) of
        undefined -> Acc;
        Value when is_integer(Value), Value >= Minimum -> Acc#{Key => Value};
        _ -> invalid(JsonKey, {integer_at_least, Minimum})
    end.

optional_string(JsonKey, Key, Args, Acc) ->
    case maps:get(JsonKey, Args, undefined) of
        undefined -> Acc;
        Value when is_binary(Value), byte_size(Value) > 0 -> Acc#{Key => binary_to_list(Value)};
        _ -> invalid(JsonKey, non_empty_string_required)
    end.

optional_path(JsonKey, Key, Cwd, Args, Acc) ->
    case maps:get(JsonKey, Args, undefined) of
        undefined -> Acc;
        Value -> Acc#{Key => absolute_path(Value, Cwd)}
    end.

dataset_selector(Values) when is_list(Values) -> [dataset_name(Value) || Value <- Values];
dataset_selector(_Values) -> invalid(<<"datasets">>, string_array_required).

dataset_name(<<"default">>) -> "default";
dataset_name(<<"worldstate">>) -> "worldstate";
dataset_name(<<"mods">>) -> "mods";
dataset_name(<<"items">>) -> "items";
dataset_name(<<"codex">>) -> "codex";
dataset_name(<<"enemies">>) -> "enemies";
dataset_name(<<"drops">>) -> "drops";
dataset_name(<<"player">>) -> "player";
dataset_name(<<"market">>) -> "market";
dataset_name(<<"diagnostics">>) -> "diagnostics";
dataset_name(<<"incidents">>) -> "incidents";
dataset_name(<<"captures">>) -> "captures";
dataset_name(<<"all">>) -> "all";
dataset_name(Value) -> invalid(<<"datasets">>, {unknown_dataset, Value}).

update_selections(Values) when is_list(Values), Values =/= [] ->
    [update_selection(Value) || Value <- Values];
update_selections(_Values) -> invalid(<<"selections">>, non_empty_string_array_required).

update_selection(<<"default">>) -> default;
update_selection(<<"all">>) -> all;
update_selection(<<"nodes">>) -> nodes;
update_selection(<<"languages">>) -> languages;
update_selection(<<"manifest">>) -> manifest;
update_selection(<<"exports">>) -> exports;
update_selection(<<"recipes">>) -> recipes;
update_selection(<<"upgrades">>) -> upgrades;
update_selection(<<"weapons">>) -> weapons;
update_selection(<<"warframes">>) -> warframes;
update_selection(<<"resources">>) -> resources;
update_selection(<<"wfcd">>) -> wfcd;
update_selection(Value) -> invalid(<<"selections">>, {unknown_selection, Value}).

dataset_names() ->
    [<<"default">>, <<"worldstate">>, <<"mods">>, <<"items">>, <<"codex">>,
     <<"enemies">>, <<"drops">>, <<"player">>, <<"market">>,
     <<"diagnostics">>, <<"incidents">>, <<"captures">>, <<"all">>].

update_names() ->
    [<<"default">>, <<"all">>, <<"nodes">>, <<"languages">>, <<"manifest">>,
     <<"exports">>, <<"recipes">>, <<"upgrades">>, <<"weapons">>,
     <<"warframes">>, <<"resources">>, <<"wfcd">>].

invalid(Key, Reason) ->
    throw({invalid_argument, {Key, Reason}}).
