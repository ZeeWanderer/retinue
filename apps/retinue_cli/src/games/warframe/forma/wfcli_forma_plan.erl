%%%-------------------------------------------------------------------
%% Forma plan calculator CLI.
%%%-------------------------------------------------------------------
-module(wfcli_forma_plan).

-export([run/1, plans_to_yaml/1, to_list/1, slot_mod_labels/1, slot_mod_labels/2,
         build_arcane_entries/1, command/0]).

-type config() :: map().
-type plan() :: map().
-type planner_result() :: {ok, config(), plan(), non_neg_integer()}.
-type slot_mod_labels() :: [{term(), [{term(), term()}]}].
-type build_arcanes() :: [{term(), [term()]}].

-import(retinue_cli_args, [option/4, flag/3]).

command() ->
    #{help => "compute Forma/polarity plan across builds", handler => {?MODULE, run},
      arguments => [(option(configs, "config", string, "config YAML file (repeatable)"))#{
                        required => true, action => append},
                    option(output, "output", string, "plan output file"),
                    flag(allow_omni, "allow-omni", "allow Omnia polarity"),
                    flag(prefer_omni, "prefer-omni", "prefer Omnia polarity"),
                    flag(allow_umbral_forma, "allow-umbral-forma", "allow Umbral Forma"),
                    option(max_forma, "max-forma", {integer, [{min, 0}]}, "maximum Forma"),
                    flag(visualize, "visualize", "open plan visualization")] ++
                   wfcli_visualize:arguments()}.

run(#{configs := Files} = Parsed) ->
    Flags = maps:with([allow_omni, prefer_omni, allow_umbral_forma, max_forma, visualize], Parsed),
    Output = maps:get(output, Parsed, undefined),
    Visualize = maps:get(visualize, Flags, false),
    VizOut = maps:get(viz_output, Parsed, undefined),
    VizCfg = maps:get(viz_config, Parsed, false),
    VizMode0 = maps:get(viz_mode, Parsed, none),
    VizMode = case VizMode0 of
                  none -> case (Visualize orelse VizCfg) of true -> html; false -> none end;
                  Other -> Other
              end,
    Request = #{source => forma,
                configs => [filename:absname(File) || File <- Files],
                flags => Flags},
    case retinue_client:one_shot(warframe, Request) of
        {ok, #{results := Results}} ->
            render_results(Results, Output, VizMode, VizOut, VizCfg);
        {error, {config_errors, Errors}} ->
            lists:foreach(fun(Error) -> io:format(standard_error, "error: ~ts~n", [Error]) end, Errors),
            halt(1);
        {error, Reason} ->
            io:format(standard_error, "planner daemon error: ~ts~n", [retinue_client:format_error(Reason)]),
            halt(1)
    end.

render_results(Results, Output0, VizMode, VizOut, VizCfg) ->
    Configs = [Config || Result <- Results,
                         Config <- [result_config(Result)],
                         Config =/= undefined],
    Output = ensure_output_path(Output0, Configs),
    Good = [Result || {ok, _, _, _} = Result <- Results],
    case Good of
        [] ->
            print_results(Results, undefined, []),
            halt(1);
        _ -> ok
    end,
    case write_output(Output, Good) of
        {ok, Path} ->
            Artifacts = visualize(Results, VizMode, VizOut, VizCfg),
            print_results(Results, Path, Artifacts),
            case length(Good) =/= length(Results) of
                true -> halt(1);
                false -> ok
            end;
        {error, Reason} ->
            io:format(standard_error, "failed to write output: ~p~n", [Reason]),
            halt(1)
    end.

print_results(Results, Path, Artifacts) ->
    Rows = [case Result of
        {ok, Config, Plan, Cost} ->
            #{config => Config, plan => Plan, forma_cost => Cost};
        {error, Config, Reason} -> #{config => Config, error => Reason}
    end || Result <- Results],
    retinue_output:emit(#{results => Rows, output => Path, artifacts => Artifacts}, fun() ->
        lists:foreach(fun print_result/1, Results),
        case Path of undefined -> ok; _ -> io:format("plan output: ~ts~n", [Path]) end,
        wfcli_visualize:print_results(Artifacts)
    end).

result_config({ok, Config, _Plan, _Cost}) -> Config;
result_config({error, Config, _Reason}) -> Config;
result_config(_) -> undefined.

print_result({ok, Config = #{file := File}, Plan, Cost}) ->
    io:format("Config: ~s~n", [File]),
    io:format("  Forma cost: ~p~n", [Cost]),
    Sorted = sort_plan(Plan),
    print_plan_table(Sorted),
    SlotMods = slot_mod_labels(Config, Plan),
    print_slot_mods(SlotMods),
    BuildArcanes = build_arcane_entries(Config),
    print_build_arcanes(BuildArcanes),
    io:format("~n", []);
print_result({error, #{file := File}, Reason}) ->
    io:format("Config: ~s~n  Error: ~p~n~n", [File, Reason]).

print_plan_table(Sorted) ->
    io:format("  Plan:~n", []),
    Slots = [slot_label(Slot) || {Slot, _} <- Sorted],
    Width = max_label_width(["Slot" | Slots], 0),
    io:format("    ~s  Polarity~n", [retinue_tty:pad_right("Slot", Width)]),
    lists:foreach(
      fun({Slot, Pol}) ->
          io:format("    ~s  ~s~n", [retinue_tty:pad_right(slot_label(Slot), Width), polarity_label(Pol)])
      end,
      Sorted).

print_slot_mods([]) ->
    io:format("  Slot mods: none~n", []);
print_slot_mods(SlotMods) ->
    io:format("  Slot mods:~n", []),
    lists:foreach(
      fun({Slot, Mods}) ->
          io:format("    ~s:~n", [slot_label(Slot)]),
          lists:foreach(
            fun({Build, Mod}) ->
                io:format("      ~s: ~s~n", [to_list(Build), to_list(Mod)])
            end,
            Mods)
      end,
      SlotMods).

print_build_arcanes([]) ->
    ok;
print_build_arcanes(BuildArcanes) ->
    io:format("  Build arcanes:~n", []),
    lists:foreach(
      fun({Build, Arcanes}) ->
          io:format("    ~s:~n", [to_list(Build)]),
          lists:foreach(fun(Arcane) -> print_arcane(Arcane) end, Arcanes)
      end,
      BuildArcanes).

print_arcane(#{name := Name} = Arcane) ->
    Rank = maps:get(rank, Arcane, undefined),
    case Rank of
        undefined -> io:format("      - ~s~n", [to_list(Name)]);
        _ -> io:format("      - ~s (rank ~p)~n", [to_list(Name), Rank])
    end;
print_arcane(Arcane) ->
    io:format("      - ~s~n", [to_list(Arcane)]).

slot_label(Slot) -> to_list(Slot).

polarity_label(Pol) ->
    case wfcli_polarity:symbol(Pol) of
        null -> "none";
        Sym when is_list(Sym) -> Sym;
        Other -> to_list(Other)
    end.

max_label_width([], Width) -> Width;
max_label_width([Label | Rest], Width) ->
    max_label_width(Rest, max(Width, length(Label))).


ensure_output_path(undefined, Configs) ->
    default_output_path(Configs);
ensure_output_path(File, _Configs) ->
    File.

default_output_path(Configs) ->
    filename:absname(default_output_file(Configs)).

default_output_file([#{file := File} | _]) ->
    ConfigPath = to_list(File),
    Base = filename:rootname(filename:basename(ConfigPath)),
    filename:join(filename:dirname(ConfigPath), Base ++ ".plan.yml");
default_output_file(_) ->
    "wfcli.plan.yml".

write_output(File, Results) ->
    Content = plans_to_yaml(Results),
    case file:write_file(File, Content) of
        ok -> {ok, File};
        {error, Reason} -> {error, Reason}
    end.

-doc "Serialize successful planner results to the YAML shape written by `--output`.".
-spec plans_to_yaml([planner_result()]) -> iodata().
plans_to_yaml(Results) ->
    Lines = [plan_to_yaml(R) || R <- Results],
    lists:flatten(lists:join("---\n", Lines)).

plan_to_yaml({ok, Config = #{file := File}, Plan, Cost}) ->
    SortedPlan = sort_plan(Plan),
    SlotMods = slot_mod_labels(Config, Plan),
    BuildArcanes = build_arcane_entries(Config),
    [
      "config: ", yaml_string(File), "\n",
      "forma_cost: ", integer_to_list(Cost), "\n",
      "plan:\n",
      [iolist_to_binary(io_lib:format("  - slot: ~p~n    polarity: ~s~n",
          [Slot, polarity_yaml(Pol)]))
       || {Slot, Pol} <- SortedPlan],
      "slot_mods:\n",
      [slot_mod_entry(Slot, Mods) || {Slot, Mods} <- SlotMods],
      build_arcane_yaml(BuildArcanes),
      "\n"
    ].

polarity_yaml(Pol) ->
    case wfcli_polarity:symbol(Pol) of
        null -> "null";
        Symbol -> [$", Symbol, $"]
    end.

sort_plan(Plan) ->
    lists:sort(fun slot_order/2, maps:to_list(Plan)).

slot_order(aura, aura) -> true;
slot_order(aura, _) -> true;
slot_order(_, aura) -> false;
slot_order(exilus, exilus) -> true;
slot_order(exilus, _) -> true;
slot_order(_, exilus) -> false;
slot_order(A, B) when is_integer(A), is_integer(B) -> A =< B;
slot_order(A, _B) when is_integer(A) -> true;
slot_order(_, _) -> true.

-doc "Return per-slot build/mod labels for the config's current polarities.".
-spec slot_mod_labels(config()) -> slot_mod_labels().
slot_mod_labels(Config) ->
    slot_mod_labels(Config, undefined).

-doc "Return per-slot build/mod labels for an explicit planned polarity map.".
-spec slot_mod_labels(config(), plan() | undefined) -> slot_mod_labels().
slot_mod_labels(Config, _Plan) when is_map(Config) ->
    maps:get(computed_slot_mods, Config, []);
slot_mod_labels(_Other, _Plan) -> [].

-doc "Return arcane labels grouped by build for visualization and YAML output.".
-spec build_arcane_entries(config()) -> build_arcanes().
build_arcane_entries(#{computed_build_arcanes := BuildArcanes}) -> BuildArcanes;
build_arcane_entries(_Other) -> [].

-doc "Convert atoms, binaries, and other values to printable lists.".
-spec to_list(term()) -> string().
to_list(V) -> retinue_text:to_list(V).

slot_mod_entry(Slot, Mods) ->
    [
      "  - slot: ", io_lib:format("~p", [Slot]), "\n",
      "    mods:\n",
      [[ "      - build: ", yaml_string(Build), "\n",
         "        mod: ", yaml_string(Mod), "\n"]
       || {Build, Mod} <- Mods]
    ].

build_arcane_yaml([]) ->
    "build_arcanes: []\n";
build_arcane_yaml(BuildArcanes) ->
    ["build_arcanes:\n",
     [build_arcane_entry(Build, Arcanes) || {Build, Arcanes} <- BuildArcanes]].

build_arcane_entry(Build, Arcanes) ->
    [
      "  - build: ", yaml_string(Build), "\n",
      "    arcanes:\n",
      [arcane_yaml(Arcane) || Arcane <- Arcanes]
    ].

arcane_yaml(#{name := Name} = Arcane) ->
    Rank = maps:get(rank, Arcane, undefined),
    case Rank of
        undefined ->
            ["      - name: ", yaml_string(Name), "\n"];
        _ ->
            ["      - name: ", yaml_string(Name), "\n",
             "        rank: ", io_lib:format("~p", [Rank]), "\n"]
    end;
arcane_yaml(Arcane) ->
    ["      - name: ", yaml_string(Arcane), "\n"].

yaml_string(Value) -> binary_to_list(jsone:encode(unicode:characters_to_binary(to_list(Value)))).

visualize(_Results, none, _Out, _Config) -> [];
visualize(Results, Mode, Out, IncludeConfig) ->
    Kinds = case IncludeConfig of true -> [config, plan]; false -> [plan] end,
    [wfcli_visualize:render(
        #{config => maps:get(file, Config), kind => Kind,
          plan => case Kind of config -> maps:get(computed_current_plan, Config, #{}); plan -> Plan end,
          slot_mods => case Kind of
              config -> maps:get(computed_current_slot_mods, Config, []);
              plan -> slot_mod_labels(Config, Plan)
          end,
          build_arcanes => build_arcane_entries(Config)},
        #{viz_mode => Mode, viz_output => Out})
     || {ok, Config, Plan, _Cost} <- Results, Kind <- Kinds].
