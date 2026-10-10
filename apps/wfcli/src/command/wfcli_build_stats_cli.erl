-module(wfcli_build_stats_cli).

-export([command/0, request/1, format/1]).

command() ->
    #{help => "evaluate an item alone or in a buildset",
      handler => fun run/1,
      arguments => [
          #{name => item, required => false, help => "exact item name, path or owned instance ID"},
          wfcli_cli_args:option(buildset, "buildset", string, "JSON file containing slot-keyed builds"),
          (wfcli_cli_args:option(target, "target", string, "slot to evaluate"))#{default => "item"},
          (wfcli_cli_args:option(members, "with", string, "add SLOT=ITEM to the buildset; repeatable"))#{
              action => append, default => []},
          wfcli_cli_args:option(config, "config", {integer, [{min, 0}, {max, 99}]},
                                "target configuration index (0 = A; default: active)"),
          (wfcli_cli_args:option(configs, "config-for", string, "select SLOT=INDEX; repeatable"))#{
              action => append, default => []},
          wfcli_cli_args:flag(unmodded, "unmodded", "use a catalog item without upgrades (frames: rank 30)"),
          wfcli_cli_args:flag(bounds, "bounds", "compare inactive and conditional lower/upper states"),
          wfcli_cli_args:option(armor, "armor", {integer, [{min, 0}, {max, 10000000}]},
                                "neutral target armor (default: 0)"),
          (wfcli_cli_args:option(effects, "effect", string, "hold EFFECT=STACKS active; repeatable"))#{
              action => append, default => []}
      ] ++ wfcli_cli_args:format([text, json], text)}.

request(Args) ->
    try
        Target = value(maps:get(target, Args, "item")),
        Initial = case maps:find(buildset, Args) of
            error -> #{};
            {ok, Path} -> read_buildset(Path)
        end,
        Members = lists:foldl(fun(Text, Acc) ->
            {Slot, Item} = assignment(Text),
            insert(Slot, #{<<"item">> => Item}, Acc)
        end, Initial, maps:get(members, Args, [])),
        Builds = case maps:find(item, Args) of
            error -> Members;
            {ok, Item} -> insert(Target, #{<<"item">> => value(Item)}, Members)
        end,
        Configs = lists:foldl(fun effect/2, #{}, maps:get(configs, Args, [])),
        TargetConfig = case maps:find(config, Args) of
            error -> Configs;
            {ok, Index} -> insert(Target, Index, Configs)
        end,
        Configured = maps:fold(fun(Slot, Index, Acc) -> update_member(Slot, <<"config">>, Index, Acc) end,
                              Builds, TargetConfig),
        Final = case maps:get(unmodded, Args, false) of
            true -> update_member(Target, <<"unmodded">>, true, Configured);
            false -> Configured
        end,
        case maps:is_key(Target, Final) of true -> ok; false -> throw({missing_buildset_target, Target}) end,
        Options = maps:from_list([{atom_to_binary(K), V} || {K, V} <-
                                  maps:to_list(maps:with([armor, bounds], Args))]),
        Effects = lists:foldl(fun effect/2, #{}, maps:get(effects, Args, [])),
        {ok, #{source => build_stats, options => Options#{<<"effects">> => Effects,
                   <<"target">> => Target, <<"buildset">> => Final}}}
    catch throw:Reason -> {error, Reason}
    end.

value(V) when is_list(V) -> unicode:characters_to_binary(V);
value(V) -> V.

read_buildset(Path) ->
    case file:read_file(Path) of
        {ok, Binary} when byte_size(Binary) =< 4194304 ->
            try jsone:decode(Binary) of
                Builds when is_map(Builds) -> Builds;
                _ -> throw(invalid_buildset_file)
            catch error:_ -> throw(invalid_buildset_file)
            end;
        {ok, _} -> throw(buildset_file_too_large);
        {error, Reason} -> throw({buildset_file, Reason})
    end.

assignment(Text) ->
    case string:split(Text, "=") of
        [Key, Val] when Key =/= "", Val =/= "" -> {value(Key), value(Val)};
        _ -> throw({invalid_assignment, Text})
    end.
insert(Key, Value, Map) ->
    case maps:is_key(Key, Map) of
        true -> throw({duplicate_buildset_slot, Key});
        false -> Map#{Key => Value}
    end.
update_member(Slot, Key, Value, Builds) ->
    case maps:find(Slot, Builds) of
        {ok, Member = #{<<"item">> := _}} -> Builds#{Slot => Member#{Key => Value}};
        _ -> throw({config_requires_item_selector, Slot})
    end.

effect(Text, Acc) ->
    case string:split(Text, "=", all) of
        [Name, Count] when Name =/= "" ->
            Id = unicode:characters_to_binary(Name),
            case {maps:is_key(Id, Acc), string:to_integer(Count)} of
                {false, {N, []}} when N >= 0 -> Acc#{Id => N};
                {true, _} -> throw({duplicate_effect, Name});
                _ -> throw({invalid_effect, Text})
            end;
        _ -> throw({invalid_effect, Text})
    end.

run(Args) ->
    Result = case request(Args) of
        {ok, Request} -> wfcli_client:one_shot(Request);
        Error -> Error
    end,
    case Result of
        {ok, Data} ->
            case maps:get(output_format, Args, text) of
                json -> io:format("~ts~n", [jsone:encode(Data)]);
                text -> io:put_chars(format(Data))
            end;
        {error, Reason} -> wfcli_cli:fail(io_lib:format("build stats: ~p", [Reason]))
    end.

format(#{target := Target, buildset := Buildset, stats := Stats} = Data) ->
    Kind = maps:get(kind, Data),
    Heading = case {Kind, maps:get(status, Data)} of
        {weapon, ready} -> "Direct-fire estimate";
        {weapon, partial} -> "Partial direct-fire estimate";
        {warframe, ready} -> "Warframe stat estimate";
        {warframe, partial} -> "Partial Warframe stat estimate"
    end,
    [Heading, "\n  target: ", Target, " - ", selection(maps:get(Target, Buildset)),
     [["\n  ", Slot, ": ", selection(Build)] || {Slot, Build} <- lists:sort(maps:to_list(Buildset)), Slot =/= Target],
     scenario(Kind, Data),
     io_lib:format("\n  player revision: ~p; updated: ~ts\n",
                   [maps:get(player_revision, Data), timestamp(maps:get(player_updated_at, Data))]),
     "\n",
     metrics(Kind, Stats),
     abilities(maps:get(selected, maps:get(abilities, Data))),
     bounds(Data),
     "\nConditional effects (held constant):\n",
     case maps:get(available_effects, Data) of
         [] -> "  none\n";
         Conditions -> [io_lib:format("  ~ts=~ts (maximum ~p)~ts\n",
                                      [maps:get(id, C), condition_value(maps:get(active_stacks, C)),
                                       maps:get(max_stacks, C),
                                       case maps:get(enabled, C) of true -> "";
                                           false -> " [requirement not met by this buildset]" end]) || C <- Conditions]
     end,
     case maps:get(excluded, Data) of
         [] -> [];
         Gaps -> ["\nNot included:\n", [["  ", note(Gap), "\n"] || Gap <- Gaps]]
     end,
     "\nAssumptions:\n", [["  ", A, "\n"] || A <- maps:get(assumptions, Data)],
     "\nLive parity not yet verified. Use --format json for the effect breakdown.\n"].

scenario(weapon, Data) -> io_lib:format("\n  target: neutral health, ~p armor, body shots",
                                      [maps:get(armor, maps:get(scenario, Data))]);
scenario(warframe, Data) -> io_lib:format("\n  equipment rank: ~p (~p)",
                                        [maps:get(rank, maps:get(base, Data)),
                                         maps:get(rank_source, maps:get(base, Data))]).

metrics(weapon, Stats) ->
    [[metric(Label, Key, Stats) || {Label, Key} <-
        [{"Expected damage / trigger (raw packet)", expected_raw_damage_per_trigger},
         {"Expected damage / trigger (target)", expected_target_damage_per_trigger},
         {"Expected damage / magazine (target)", expected_target_damage_per_magazine},
         {"Theoretical burst DPS", theoretical_burst_dps},
         {"Theoretical sustained DPS", theoretical_sustained_dps},
         {"Fire rate / second", fire_rate}, {"Reload seconds", reload_time}]],
     "\nPer-projectile outcomes (body hits):\n",
     io_lib:format("  ~-13s ~10s ~14s ~14s~n", ["Critical tier", "Chance", "Raw packet", "Health"]),
     [io_lib:format("  ~-13s ~9.2f% ~14s ~14s~n",
         [case Tier of 0 -> "Normal"; _ -> integer_to_list(Tier) end,
          P * 100, metric_value(Raw), metric_value(Health)])
      || #{tier := Tier, probability := P, raw_damage := Raw, health_damage := Health}
             <- maps:get(projectile_outcomes, Stats)]];
metrics(warframe, Stats) ->
    [[metric(Label, Key, Stats) || {Label, Key} <-
        [{"Health", health}, {"Shields", shield}, {"Armor", armor}, {"Energy", energy},
         {"Overshield capacity", overshield_capacity}, {"Sprint speed", sprint_speed}]],
     [metric(Label, Key, maps:map(fun(_, V) -> V * 100 end, Stats)) || {Label, Key} <-
        [{"Ability strength %", ability_strength}, {"Ability duration %", ability_duration},
         {"Ability range %", ability_range}, {"Ability efficiency % (uncapped)", ability_efficiency}]]].

abilities([]) -> [];
abilities(Abilities) ->
    ["\nReviewed ability previews:\n",
     [[io_lib:format("  ~ts: ~ts (rank ~p)\n", [maps:get(owner, A), maps:get(item, A), maps:get(rank, A)]),
       [ability_value(Key, Value) || {Key, Value} <- lists:sort(maps:to_list(maps:get(outputs, A)))]]
        || A <- Abilities]].
ability_value(Key, #{variants := Variants, unit := Unit}) ->
    [["    ", label(Key), " (", atom_to_list(Variant), "): ", metric_value(Value),
      " ", atom_to_list(Unit), "\n"] || {Variant, Value} <- lists:sort(maps:to_list(Variants))];
ability_value(Key, #{value := Value, unit := Unit}) ->
    ["    ", label(Key), ": ", metric_value(Value), " ", atom_to_list(Unit), "\n"].
label(Key) -> string:replace(atom_to_list(Key), "_", " ", all).

selection(#{name := Name, config_index := null}) -> [Name, " (catalog/local build)"];
selection(#{name := Name, config_index := Index}) ->
    io_lib:format("~ts (config ~p)", [Name, Index]).
metric(Label, Key, Stats) -> io_lib:format("  ~-44s ~ts~n", [Label, metric_value(maps:get(Key, Stats))]).
metric_value(null) -> "unbounded while ammo saving stays active";
metric_value(Value) -> io_lib:format("~.2f", [float(Value)]).
condition_value(null) -> "auto (initial combo)";
condition_value(Value) -> integer_to_binary(Value).

bounds(#{kind := Kind, bounds := Bounds, effect_graph := Graph}) ->
    ["\nConditional bounds (held-state estimates):\n",
     io_lib:format("  ~-32s ~12s ~12s ~12s~n", ["", "Inactive", "Lower", "Upper"]),
     [io_lib:format("  ~-32s ~12.2f ~12.2f ~12.2f~n", [Label |
          [float(maps:get(Key, maps:get(Mode, Bounds))) || Mode <- [inactive, lower, upper]]])
      || {Label, Key} <- bound_metrics(Kind)],
     [io_lib:format("  ~ts: initial combo ~B; combo multiplier ~px to ~px\n",
          [maps:get(owner, N), round(maps:get(initial_combo, maps:get(lower, maps:get(states, N)))),
           maps:get(multiplier, maps:get(lower, maps:get(states, N))),
           maps:get(multiplier, maps:get(upper, maps:get(states, N)))])
      || N = #{kind := combo} <- Graph],
     [["  ", maps:get(item, N), ": active from recovered initial combo in the lower state.\n"]
      || N = #{kind := outburst} <- Graph],
     "  Upper: enabled effects at cap. Buildup, buff expiry and swaps are not simulated.\n"];
bounds(_) -> [].
bound_metrics(weapon) -> [{"Target damage / trigger", expected_target_damage_per_trigger},
                         {"Burst DPS", theoretical_burst_dps},
                         {"Held-state sustained DPS", theoretical_sustained_dps}];
bound_metrics(warframe) -> [{"Health", health}, {"Shields", shield}, {"Armor", armor}, {"Energy", energy}].
note(Note) when is_binary(Note) -> Note;
note(#{item := Name, reason := Reason}) -> [Name, ": ", Reason];
note(#{reason := Reason, count := Count}) -> io_lib:format("~ts (~p)", [Reason, Count]);
note(#{reason := Reason, ability := #{<<"Ability">> := Type}}) ->
    [Reason, ": ", filename:basename(Type)];
note(Note) -> jsone:encode(Note).

timestamp(null) -> "unknown";
timestamp(Value) -> wfcli_output:timestamp(Value).
