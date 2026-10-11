%%%-------------------------------------------------------------------
%% Persisted multi-build planning goals.
%%%-------------------------------------------------------------------
-module(wfcli_build_group).

-export([create/4, update/4, add_source/3, add_config/5,
         remove_member/3, public/1, refresh_target/2]).

-define(SCHEMA, 1).

-doc "Create a group, optionally targeting a concrete equipment instance.".
-spec create(map(), map(), integer(), binary()) -> {ok, map()} | {error, term()}.
create(Input, Equipment, Now, Id) ->
    DefinitionId = maps:get(<<"definition_id">>, Input, undefined),
    case definition(DefinitionId, Equipment) of
        {ok, Definition} ->
            InstanceId = maps:get(<<"instance_id">>, Input, null),
            case target(DefinitionId, InstanceId, Equipment) of
                {ok, Baseline} ->
                    DefaultName = <<(maps:get(<<"name">>, Definition))/binary,
                                    " builds">>,
                    case name(maps:get(<<"name">>, Input, DefaultName)) of
                        {ok, Name} ->
                            {ok, #{<<"schema">> => ?SCHEMA,
                                   <<"id">> => Id,
                                   <<"name">> => Name,
                                   <<"definition_id">> => DefinitionId,
                                   <<"instance_id">> => nullable(InstanceId),
                                   <<"baseline">> => nullable(Baseline),
                                   <<"members">> => [],
                                   <<"options">> => default_options(),
                                   <<"revision">> => 1,
                                   <<"created_at">> => Now,
                                   <<"updated_at">> => Now}};
                        Error -> Error
                    end;
                Error -> Error
            end;
        Error -> Error
    end.

-doc "Update editable group metadata and optionally retarget its instance.".
-spec update(map(), map(), map(), integer()) -> {ok, map()} | {error, term()}.
update(Group, Patch, Equipment, Now) ->
    case updated_name(Group, Patch) of
        {error, _Reason} = Error -> Error;
        {ok, Name} ->
            DefinitionId = maps:get(<<"definition_id">>, Group),
            InstanceId = maps:get(<<"instance_id">>, Patch,
                                  maps:get(<<"instance_id">>, Group, null)),
            case target(DefinitionId, InstanceId, Equipment) of
                {ok, Baseline} ->
                    Options0 = maps:get(<<"options">>, Group, default_options()),
                    Options = case maps:get(<<"options">>, Patch, undefined) of
                        Value when is_map(Value) -> maps:merge(Options0, Value);
                        _ -> Options0
                    end,
                    Changed = Group#{<<"name">> => Name,
                                     <<"instance_id">> => nullable(InstanceId),
                                     <<"baseline">> => nullable(Baseline),
                                     <<"options">> => Options},
                    {ok, touch_if_changed(Group, Changed, Now)};
                Error -> Error
            end
    end.

-doc "Attach an immutable source revision to a compatible group.".
-spec add_source(map(), map(), integer()) -> {ok, map()} | {error, term()}.
add_source(Group, Revision, Now) ->
    Content = maps:get(<<"content">>, Revision, #{}),
    case maps:get(<<"item">>, Content, null) =:=
         maps:get(<<"definition_id">>, Group) of
        false -> {error, build_group_item_mismatch};
        true ->
            Identity = maps:get(<<"identity">>, Revision, #{}),
            Source = maps:get(<<"source">>, Identity, undefined),
            ExternalId = maps:get(<<"external_id">>, Identity, undefined),
            Fingerprint = maps:get(<<"fingerprint">>, Revision, undefined),
            case is_binary(Source) andalso ExternalId =/= undefined andalso
                 is_binary(Fingerprint) of
                false -> {error, invalid_build_revision};
                true ->
                    Metadata = maps:get(<<"metadata">>, Revision, #{}),
                    Member = #{<<"id">> => source_member_id(Source, ExternalId,
                                                            Fingerprint),
                               <<"kind">> => <<"source_revision">>,
                               <<"name">> => maps:get(<<"title">>, Metadata,
                                                       <<"Imported build">>),
                               <<"source">> => Source,
                               <<"external_id">> => ExternalId,
                               <<"fingerprint">> => Fingerprint,
                               <<"snapshot">> => Revision},
                    {ok, add_member(Group, Member, Now)}
            end
    end.

-doc "Capture one current player configuration as an immutable group member.".
-spec add_config(map(), binary(), non_neg_integer(), map(), integer()) ->
    {ok, map()} | {error, term()}.
add_config(Group, InstanceId, ConfigIndex, Equipment, Now)
  when is_binary(InstanceId), is_integer(ConfigIndex), ConfigIndex >= 0 ->
    DefinitionId = maps:get(<<"definition_id">>, Group),
    case instance(InstanceId, Equipment) of
        {ok, Instance = #{<<"definition_id">> := DefinitionId}} ->
            case config(ConfigIndex, maps:get(<<"configs">>, Instance, [])) of
                {ok, Config} ->
                    Baseline = baseline(Instance),
                    Snapshot0 = #{<<"definition_id">> => DefinitionId,
                                  <<"instance_id">> => InstanceId,
                                  <<"config">> => Config},
                    Fingerprint = fingerprint(Snapshot0),
                    Snapshot = Snapshot0#{<<"fingerprint">> => Fingerprint},
                    Member = #{<<"id">> => config_member_id(InstanceId,
                                                            ConfigIndex,
                                                            Fingerprint),
                               <<"kind">> => <<"player_config">>,
                               <<"name">> => config_name(Config, ConfigIndex),
                               <<"instance_id">> => InstanceId,
                               <<"config_index">> => ConfigIndex,
                               <<"fingerprint">> => Fingerprint,
                               <<"snapshot">> => Snapshot},
                    Group1 = case maps:get(<<"instance_id">>, Group, null) of
                        null -> Group#{<<"instance_id">> => InstanceId,
                                       <<"baseline">> => Baseline};
                        InstanceId -> Group;
                        _Other -> Group
                    end,
                    {ok, add_member(Group1, Member, Now)};
                Error -> Error
            end;
        {ok, _Instance} -> {error, build_group_item_mismatch};
        Error -> Error
    end;
add_config(_Group, _InstanceId, _ConfigIndex, _Equipment, _Now) ->
    {error, invalid_player_config_member}.

-doc "Remove one member by stable member ID.".
-spec remove_member(map(), binary(), integer()) -> {ok, map()} | {error, term()}.
remove_member(Group, MemberId, Now) when is_binary(MemberId) ->
    Members = maps:get(<<"members">>, Group, []),
    Kept = [Member || Member <- Members,
                      maps:get(<<"id">>, Member, undefined) =/= MemberId],
    case length(Kept) =:= length(Members) of
        true -> {error, build_group_member_not_found};
        false -> {ok, touch(Group#{<<"members">> => Kept}, Now)}
    end;
remove_member(_Group, _MemberId, _Now) -> {error, invalid_build_group_member_id}.

-doc "Return protocol-facing group data with derived summary fields.".
-spec public(map()) -> map().
public(Group) ->
    Members = maps:get(<<"members">>, Group, []),
    Group#{<<"member_count">> => length(Members),
           <<"source_member_count">> => count_kind(<<"source_revision">>, Members),
           <<"config_member_count">> => count_kind(<<"player_config">>, Members)}.

-spec refresh_target(map(), map()) -> map().
refresh_target(Group, Equipment) ->
    Id = maps:get(<<"instance_id">>, Group, null),
    case target(maps:get(<<"definition_id">>, Group), Id, Equipment) of
        {ok, undefined} -> Group#{<<"baseline">> => null,
                                  <<"target_status">> => <<"unassigned">>};
        {ok, Baseline} -> Group#{<<"baseline">> => Baseline,
                                 <<"target_status">> => <<"available">>};
        {error, _} -> Group#{<<"baseline">> => null,
                             <<"target_status">> => <<"missing">>}
    end.

updated_name(Group, Patch) ->
    case maps:find(<<"name">>, Patch) of
        error -> {ok, maps:get(<<"name">>, Group)};
        {ok, Value} -> name(Value)
    end.

name(Value) when is_binary(Value) ->
    Trimmed = string:trim(Value),
    case byte_size(Trimmed) of
        0 -> {error, invalid_build_group_name};
        N when N =< 120 -> {ok, Trimmed};
        _ -> {error, invalid_build_group_name}
    end;
name(_Value) -> {error, invalid_build_group_name}.

definition(Id, #{<<"definitions">> := Definitions}) when is_binary(Id) ->
    find(<<"id">>, Id, Definitions, build_definition_not_found);
definition(_Id, _Equipment) -> {error, invalid_build_definition}.

instance(Id, #{<<"instances">> := Instances}) when is_binary(Id) ->
    find(<<"instance_id">>, Id, Instances, build_instance_not_found);
instance(_Id, _Equipment) -> {error, build_instance_not_found}.

config(Index, Configs) -> find(<<"config_index">>, Index, Configs,
                               build_config_not_found).

find(Key, Value, Values, Error) when is_list(Values) ->
    case [Item || Item <- Values, is_map(Item),
                  maps:get(Key, Item, undefined) =:= Value] of
        [Item | _] -> {ok, Item};
        [] -> {error, Error}
    end;
find(_Key, _Value, _Values, Error) -> {error, Error}.

target(_DefinitionId, null, _Equipment) -> {ok, undefined};
target(_DefinitionId, undefined, _Equipment) -> {ok, undefined};
target(DefinitionId, InstanceId, Equipment) when is_binary(InstanceId) ->
    case instance(InstanceId, Equipment) of
        {ok, Instance = #{<<"definition_id">> := DefinitionId}} ->
            {ok, baseline(Instance)};
        {ok, _Instance} -> {error, build_group_item_mismatch};
        Error -> Error
    end;
target(_DefinitionId, _InstanceId, _Equipment) -> {error, invalid_build_instance}.

baseline(Instance) ->
    Base0 = maps:with([<<"instance_id">>, <<"definition_id">>, <<"class">>,
                       <<"capacity">>,
                       <<"forma_count">>, <<"feature_flags">>, <<"features">>,
                       <<"effective_polarities">>, <<"shard_slots">>,
                       <<"topology">>], Instance),
    Base0#{<<"fingerprint">> => fingerprint(Base0)}.

add_member(Group, Member, Now) ->
    Members = maps:get(<<"members">>, Group, []),
    Id = maps:get(<<"id">>, Member),
    case lists:any(fun(Existing) -> maps:get(<<"id">>, Existing, undefined) =:= Id end,
                   Members) of
        true -> Group;
        false -> touch(Group#{<<"members">> => Members ++ [Member]}, Now)
    end.

touch_if_changed(Before, After, Now) ->
    ComparableKeys = [<<"name">>, <<"instance_id">>, <<"baseline">>, <<"options">>],
    case maps:with(ComparableKeys, Before) =:= maps:with(ComparableKeys, After) of
        true -> Before;
        false -> touch(After, Now)
    end.

touch(Group, Now) ->
    Group#{<<"revision">> => maps:get(<<"revision">>, Group, 0) + 1,
           <<"updated_at">> => Now}.

default_options() ->
    #{<<"preserve_source_slots">> => false,
      <<"allow_omni">> => false,
      <<"allow_umbral_forma">> => false,
      <<"prefer_omni">> => false}.

config_name(Config, Index) ->
    case maps:get(<<"name">>, Config, undefined) of
        Name when is_binary(Name), byte_size(Name) > 0 -> Name;
        _ -> <<"Configuration ", (integer_to_binary(Index + 1))/binary>>
    end.

source_member_id(Source, ExternalId, Fingerprint) ->
    <<"source:", Source/binary, ":", (text(ExternalId))/binary, ":",
      Fingerprint/binary>>.

config_member_id(InstanceId, ConfigIndex, Fingerprint) ->
    <<"config:", InstanceId/binary, ":", (integer_to_binary(ConfigIndex))/binary,
      ":", Fingerprint/binary>>.

count_kind(Kind, Members) ->
    length([ok || Member <- Members,
                  maps:get(<<"kind">>, Member, undefined) =:= Kind]).

fingerprint(Value) ->
    binary:encode_hex(crypto:hash(sha256, term_to_binary(Value, [deterministic])),
                      lowercase).

text(Value) when is_binary(Value) -> Value;
text(Value) when is_integer(Value) -> integer_to_binary(Value);
text(Value) -> iolist_to_binary(io_lib:format("~p", [Value])).

nullable(undefined) -> null;
nullable(Value) -> Value.
