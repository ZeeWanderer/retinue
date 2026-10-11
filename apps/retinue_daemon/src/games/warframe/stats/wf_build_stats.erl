-module(wf_build_stats).

-export([validate/1, run/1, calculate/3]).

validate(Options) -> wf_stat_buildset:validate(Options).

run(Options) ->
    case validate(Options) of
        ok ->
            case load_definitions(["ExportWeapons_en.json", "ExportWarframes_en.json"]) of
                {ok, Definitions} ->
                    {ok, Equipment} = wf_build_equipment:snapshot(),
                    Game = maps:get(data, wf_game_metadata_service:snapshot()),
                    Executable = maps:get(<<"executable">>, Game, #{}),
                    calculate(Options, Equipment#{<<"game_executable_sha256">> =>
                        maps:get(<<"sha256">>, Executable, null)}, Definitions);
                Error -> Error
            end;
        Error -> Error
    end.

load_definitions([]) -> {ok, []};
load_definitions([File | Rest]) ->
    case load_definition(File) of
        {ok, Items} ->
            case load_definitions(Rest) of
                {ok, More} -> {ok, Items ++ More};
                Error -> Error
            end;
        Error -> Error
    end.

load_definition(File) ->
    case wf_exports:load_raw_items(undefined, File) of
        {ok, _} = Found -> Found;
        {error, _} ->
            case wf_source_manager:ensure_catalog(
                    "items", #{files => [File]}) of
                ok -> wf_exports:load_raw_items(undefined, File);
                Error -> Error
            end
    end.

-spec calculate(map(), map(), [map()]) -> {ok, map()} | {error, term()}.
calculate(Options, Equipment, Catalog) ->
    case validate(Options) of
        ok ->
            try evaluate(Options, Equipment, Catalog)
            catch throw:{stats, Reason} -> {error, Reason}
            end;
        Error -> Error
    end.

evaluate(Options, Equipment, Catalog) ->
    Buildset = wf_stat_buildset:resolve(maps:get(<<"buildset">>, Options), Equipment, Catalog),
    Target = maps:get(<<"target">>, Options),
    Item = maps:get(Target, Buildset),
    Type = maps:get(<<"definition_id">>, Item),
    {Kind, Profile} = profile(Item),
    ensure(maps:get(<<"class">>, Item) =:= maps:get(class, Profile), invalid_target_class),
    Export = case [W || W <- Catalog, maps:get(<<"uniqueName">>, W, null) =:= Type] of
        [Found] -> Found;
        _ -> fail({item_stats_unavailable, Type})
    end,
    Base = case Kind of
        weapon -> wf_stat_profiles:base(Export, Profile);
        warframe ->
            ensure(maps:get(<<"armor">>, Options, 0) =:= 0, armor_requires_weapon_target),
            wf_stat_frames:base(Export, Item)
    end,
    Graph = wf_stat_effects:compile(Buildset, Target, Profile, Catalog),
    Inputs = maps:get(<<"effects">>, Options, #{}),
    Conditions = wf_stat_effects:conditions(Graph, Inputs),
    Modes = case maps:get(<<"bounds">>, Options, false) of
        true -> [selected, inactive, lower, upper];
        false -> [selected]
    end,
    States = maps:from_list([{Mode, wf_stat_effects:evaluate(Graph, Mode, Inputs)} || Mode <- Modes]),
    Scenario = case Kind of
        weapon -> #{armor => maps:get(<<"armor">>, Options, 0), hit => body,
                    defenses => neutral_health, effects => Inputs};
        warframe -> #{effects => Inputs}
    end,
    Estimates = maps:map(fun(_, State) ->
        Effects = maps:get(Target, maps:get(effects, State), []),
        case Kind of
            weapon -> wf_stat_math:direct_fire(Base, Effects, Scenario);
            warframe -> wf_stat_frames:evaluate(Base, Effects)
        end
    end, States),
    Provenance = wf_stat_rules:provenance(),
    Observed = maps:get(<<"game_executable_sha256">>, Equipment, null),
    Match = case Observed of null -> unknown; _ -> Observed =:= maps:get(executable_sha256, Provenance) end,
    Changed = case Match of
        false -> [<<"Observed game executable differs from the researched rule version">>];
        _ -> []
    end,
    Grouping = case Kind =:= weapon andalso
        lists:any(fun(S) -> maps:get(multishot, S) > 1 end, maps:values(Estimates)) of
        true -> [<<"Multi-projectile totals assume separate health packets; native grouping is not verified">>];
        false -> []
    end,
    Excluded = lists:usort(maps:get(excluded, Graph) ++ Changed ++ Grouping),
    Result = (description(Kind))#{schema => 1, kind => Kind,
        status => case Excluded of [] -> ready; _ -> partial end,
        target => Target, buildset => maps:map(fun(_, I) -> wf_stat_buildset:summary(I) end, Buildset),
        player_revision => maps:get(<<"player_revision">>, Equipment, 0),
        player_updated_at => maps:get(<<"player_updated_at">>, Equipment, null),
        provenance => Provenance#{base_definition_sha256 => hash(Export), buildset_sha256 => hash(Buildset),
            dependencies => wf_stat_effects:dependencies(Graph, Buildset),
            observed_executable_sha256 => Observed, executable_matches => Match},
        scenario => Scenario, profile => Profile, base => public_base(Base), stats => maps:get(selected, Estimates),
        frame_multipliers => frame_multipliers(Buildset, maps:get(selected, States)),
        abilities => maps:map(fun(_, State) -> wf_stat_abilities:outputs(Graph, State) end, States),
        effect_graph => wf_stat_effects:public(Graph, States),
        available_effects => Conditions, excluded => Excluded},
    {ok, case maps:get(<<"bounds">>, Options, false) of
        false -> Result;
        true -> Result#{bounds => (maps:without([selected], Estimates))#{
            meaning => <<"Inactive baseline and conditional lower/upper snapshots of reviewed effects, not a combat timeline">>,
            lower_condition => <<"Equipped bonuses and declared minimum active buffs; no earned stacks">>,
            upper_condition => <<"Enabled effects at cap; temporary effects held active">>}}
    end}.

profile(#{<<"class">> := Class, <<"definition_id">> := Type}) ->
    {Kind, Found, Error} = case Class of
        <<"warframe">> -> {warframe, wf_stat_frames:profile(Type), unsupported_frame_profile};
        _ -> {weapon, wf_stat_profiles:weapon(Type), unsupported_attack_profile}
    end,
    case Found of unsupported -> fail({Error, Type}); _ -> {Kind, Found} end.

description(weapon) ->
    #{scope => <<"normal-mode direct fire; not a combat simulation">>, assumptions => [
            <<"All projectiles hit a neutral health target; body shots, no falloff">>,
            <<"Per-projectile critical outcomes are rounded before averaging health damage">>,
            <<"Trigger totals sum projectile expectations, not guaranteed whole-shot outcomes">>,
            <<"Conditional states are held constant; buildup and uptime are not simulated">>,
            <<"No status damage over time, shield, overguard, faction bonus or attenuation">>,
            <<"DPS assumes uninterrupted firing and ideal reloads; Incarnon form is not evaluated">>
        ]};
description(warframe) ->
    #{scope => <<"Warframe properties and reviewed PvE ability previews; not a combat simulation">>,
      assumptions => [
          <<"Frame properties use the selected equipment rank before mod multipliers">>,
          <<"Only selected equipment slots contribute; absent melee enables Mesa's health passive">>,
          <<"Overshield capacity is not current overshields, a shield bonus or a generation rate">>,
          <<"Ability outputs describe a cast; enemy application, resistance and combat timing are not simulated">>,
          <<"Ability efficiency is an uncapped stat multiplier, not a universal cast or channel cost">>
      ]}.

public_base(#{packet := Packet} = Base) ->
    Base#{packet => Packet#{channels => maps:from_list(maps:get(channels, Packet))}};
public_base(Base) -> Base.

frame_multipliers(Buildset, State) ->
    maps:from_list([{Owner, wf_stat_frames:multipliers(maps:get(Owner, maps:get(effects, State), []))}
        || {Owner, #{<<"class">> := <<"warframe">>}} <- maps:to_list(Buildset)]).

ensure(true, _) -> ok;
ensure(false, Reason) -> fail(Reason).
fail(Reason) -> throw({stats, Reason}).
hash(Value) -> binary:encode_hex(crypto:hash(sha256, jsone:encode(Value)), lowercase).
