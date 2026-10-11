%%%-------------------------------------------------------------------
%% Typed Deep/Temporal Archimedea projection over raw Conquests data.
%%%-------------------------------------------------------------------
-module(wfcli_archimedea).

-export([name/1, project/2]).

-type raw_archimedea() :: map().
-type opts() :: map().

-doc "Return stable display identity for one raw Conquests rotation.".
-spec name(raw_archimedea()) -> string().
name(Data) ->
    {_Kind, Name} = identity(Data),
    Name.

-doc "Derive semantic mission, risk, and modifier fields while retaining raw data upstream.".
-spec project(raw_archimedea(), opts()) -> map().
project(Data, Opts) ->
    {Kind, Name} = identity(Data),
    MissionRows = missions(Data, Opts),
    MissionNames = join([maps:get(label, Mission) || Mission <- MissionRows], "; "),
    Deviations = join(unique([maps:get(deviation, Mission) || Mission <- MissionRows]), ", "),
    Risks = join(unique(lists:append([maps:get(risks, Mission) || Mission <- MissionRows])), ", "),
    EliteRisks = join(
                   unique(lists:append([maps:get(elite_risks, Mission) || Mission <- MissionRows])),
                   ", "),
    MissionDetails = join([maps:get(details, Mission) || Mission <- MissionRows], "\n  "),
    Variables = [Value || Value <- maps:get(<<"Variables">>, Data, []),
                           retinue_text:value_present(Value)],
    {PersonalModifierNames, PersonalModifierDetails} = personal_modifier_text(Variables, Opts),
    Seed = maps:get(<<"RandomSeed">>, Data, undefined),
    ModifierDetails = join(PersonalModifierDetails, "\n    "),
    Loadout = wfcli_archimedea_loadout:resolve(
                Data, maps:get(archimedea_loadout, Opts, #{})),
    maps:merge(#{type => "Archimedea",
      summary => Name,
      name => Name,
      archimedea => Kind,
      missions => MissionNames,
      deviations => Deviations,
      risks => Risks,
      elite_risks => EliteRisks,
      personal_modifiers => join(PersonalModifierNames, ", "),
      mission_details => MissionDetails,
      modifier_details => ModifierDetails,
      seed => Seed,
      randomseed => Seed,
      details => retinue_text:join_parts([MissionDetails, ModifierDetails], "\n  ")},
      loadout_fields(Loadout)).

identity(#{<<"Type">> := <<"CT_LAB">>}) -> {"Deep", "Deep Archimedea"};
identity(#{<<"Type">> := <<"CT_HEX">>}) -> {"Temporal", "Temporal Archimedea"};
identity(Data) ->
    Raw = retinue_text:to_list(maps:get(<<"Type">>, Data, <<"Archimedea">>)),
    {Raw, Raw}.

missions(Data, Opts) ->
    RawMissions = [Mission || Mission <- maps:get(<<"Missions">>, Data, []),
                              is_map(Mission)],
    [project_mission(Mission, Index, Opts)
     || {Index, Mission} <- lists:zip(lists:seq(1, length(RawMissions)), RawMissions)].

project_mission(Mission, Index, Opts) ->
    MissionName = wfcli_resolve:resolve(
                    "missiontype",
                    maps:get(<<"missionType">>, Mission,
                             maps:get(<<"MissionType">>, Mission, <<"Unknown">>)), Opts),
    Faction = wfcli_resolve:resolve(
                "faction", maps:get(<<"faction">>, Mission, <<"Unknown">>), Opts),
    Difficulties = [Difficulty || Difficulty <- maps:get(<<"difficulties">>, Mission, []),
                                  is_map(Difficulty)],
    Normal = normal_difficulty(Difficulties),
    DeviationKey = maps:get(<<"deviation">>, Normal, <<>>),
    RiskKeys = maps:get(<<"risks">>, Normal, []),
    EliteKeys = lists:append(
                  [additional_risks(maps:get(<<"risks">>, Difficulty, []))
                   || Difficulty <- Difficulties,
                      maps:get(<<"type">>, Difficulty, undefined) =:= <<"CD_HARD">>]),
    Label = lists:flatten(io_lib:format("~p. ~s (~s)", [Index, MissionName, Faction])),
    DetailLines = [Label,
                   labeled_modifier("Deviation", DeviationKey, Opts),
                   labeled_modifiers("Risk", RiskKeys, Opts),
                   labeled_modifiers("Elite risk", EliteKeys, Opts)],
    #{label => Label,
      deviation => modifier_name(DeviationKey, Opts),
      risks => [modifier_name(Key, Opts) || Key <- RiskKeys],
      elite_risks => [modifier_name(Key, Opts) || Key <- EliteKeys],
      details => retinue_text:join_parts(DetailLines, "\n    ")}.

normal_difficulty([First | _] = Difficulties) ->
    case [Difficulty || Difficulty <- Difficulties,
                        maps:get(<<"type">>, Difficulty, undefined) =:= <<"CD_NORMAL">>] of
        [Normal | _] -> Normal;
        [] -> First
    end;
normal_difficulty([]) -> #{}.

additional_risks([_Shared | Additional]) -> Additional;
additional_risks(_) -> [].

labeled_modifier(_Label, Key, _Opts) when Key =:= <<>>; Key =:= "" -> "";
labeled_modifier(Label, Key, Opts) ->
    lists:flatten(io_lib:format("~s: ~s", [Label, modifier_detail(Key, Opts)])).

labeled_modifiers(_Label, [], _Opts) -> "";
labeled_modifiers(Label, Keys, Opts) ->
    Details = [modifier_detail(Key, Opts) || Key <- Keys],
    lists:flatten(io_lib:format("~s: ~s", [Label, join(Details, "; ")])).

modifier_name(Key, Opts) ->
    wfcli_resolve:resolve("any", Key, Opts).

modifier_detail(Key, Opts) ->
    detail_with_label(modifier_name(Key, Opts), Key, Opts).

personal_modifier_text(Keys, Opts) ->
    Names = [modifier_name(Key, Opts) || Key <- Keys],
    Labels = [duplicate_modifier_label(Key, Name, Names)
              || {Key, Name} <- lists:zip(Keys, Names)],
    Details = [detail_with_label(Label, Key, Opts)
               || {Label, Key} <- lists:zip(Labels, Keys)],
    {Labels, Details}.

duplicate_modifier_label(Key, Name, Names) ->
    case length([Other || Other <- Names, Other =:= Name]) > 1 of
        true -> lists:flatten(io_lib:format("~s [~s]", [Name, retinue_text:to_list(Key)]));
        false -> Name
    end.

detail_with_label(Label, Key, Opts) ->
    case wfcli_resolve:resolve_description(Key, Opts) of
        "" -> Label;
        Description -> lists:flatten(io_lib:format("~s - ~s", [Label, Description]))
    end.

unique(Values) ->
    lists:reverse(
      lists:foldl(
        fun("", Acc) -> Acc;
           (Value, Acc) ->
                case lists:member(Value, Acc) of
                    true -> Acc;
                    false -> [Value | Acc]
                end
        end,
        [], Values)).

join(Values, Separator) ->
    retinue_text:join_list(Values, Separator).

loadout_fields(#{status := ready} = Loadout) ->
    Suits = category_text(maps:get(suits, Loadout)),
    Primaries = category_text(maps:get(primaries, Loadout)),
    Secondaries = category_text(maps:get(secondaries, Loadout)),
    Melees = category_text(maps:get(melees, Loadout)),
    Lines = ["Warframes: " ++ Suits,
             "Primary: " ++ Primaries,
             "Secondary: " ++ Secondaries,
             "Melee: " ++ Melees],
    #{loadout_status => "ready",
      loadout_warning => warning_text(maps:get(warning, Loadout, none)),
      loadout_seed => maps:get(effective_seed, Loadout),
      suits => Suits,
      primaries => Primaries,
      secondaries => Secondaries,
      melees => Melees,
      loadouts => join(Lines, "\n  ")};
loadout_fields(#{reason := Reason}) ->
    #{loadout_status => atom_to_list(Reason),
      loadout_warning => "",
      loadout_seed => undefined,
      suits => "", primaries => "", secondaries => "", melees => "",
      loadouts => unavailable_text(Reason)}.

category_text(Items) ->
    join([item_text(Item) || Item <- Items], ", ").

item_text(#{name := Name, owned := true}) -> Name ++ " [owned]";
item_text(#{name := Name}) -> Name.

warning_text(none) -> "";
warning_text(unverified_game_metadata) ->
    "Using cached equipment pools; loadouts are unverified for the current Warframe build".

unavailable_text(missing_account_seed) ->
    "Unavailable: launch Warframe through wfcompanion once to cache the account seed";
unavailable_text(missing_player_inventory) ->
    "Unavailable: cached player inventory is missing";
unavailable_text(missing_game_metadata) ->
    "Unavailable: open an Archimedea screen while wfcompanion is running once";
unavailable_text(invalid_game_metadata) ->
    "Unavailable: cached game metadata is invalid";
unavailable_text(unsupported_game_build) ->
    "Unavailable: no cached equipment pools; capture is not supported for this Warframe build";
unavailable_text(game_metadata_unavailable) ->
    "Unavailable: game metadata could not be captured";
unavailable_text(missing_equipment_exports) ->
    "Unavailable: equipment exports are missing";
unavailable_text(missing_worldstate_seed) ->
    "Unavailable: worldstate rotation has no random seed";
unavailable_text(player_store_unavailable) ->
    "Unavailable: player store is not running";
unavailable_text(game_metadata_store_unavailable) ->
    "Unavailable: game metadata store is not running";
unavailable_text(Reason) ->
    "Unavailable: " ++ atom_to_list(Reason).
