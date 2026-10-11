%%%-------------------------------------------------------------------
%% Common Test coverage for forma-plan planner and YAML output.
%%%-------------------------------------------------------------------
-module(wf_forma_plan_SUITE).

-export([all/0,
         init_per_suite/1,
         end_per_suite/1,
         plan_requires_single_polarity/1,
         plan_requires_omni/1,
         plan_assigns_unslotted_mods/1,
         plan_complex_dual_build/1,
         plan_complex_three_build/1,
         plan_wisp_regression/1,
         plan_movable_aura_reuse/1,
         plan_allows_optional_slot/1,
         mod_lookup_fills_missing_fields/1,
         mod_lookup_preserves_cost/1,
         mod_lookup_warns_on_mismatch/1,
         yaml_output_serialization/1,
         independent_plan_documents/1,
         default_plan_output_path/1,
         visualize_plan_html/1,
         default_visualization_paths/1,
         visualize_config_html/1,
         invalid_config_errors/1,
         unsupported_service_action_rejected/1,
         over_budget_rejected/1,
         no_solution_rejected/1,
         max_forma_flag_rejected/1]).

-include_lib("common_test/include/ct.hrl").
-include_lib("eunit/include/eunit.hrl").

all() ->
    [plan_requires_single_polarity,
     plan_requires_omni,
     plan_assigns_unslotted_mods,
     plan_complex_dual_build,
     plan_complex_three_build,
     plan_wisp_regression,
     plan_movable_aura_reuse,
     plan_allows_optional_slot,
     mod_lookup_fills_missing_fields,
     mod_lookup_preserves_cost,
    mod_lookup_warns_on_mismatch,
    yaml_output_serialization,
    independent_plan_documents,
    default_plan_output_path,
    visualize_plan_html,
    default_visualization_paths,
    visualize_config_html,
    invalid_config_errors,
    unsupported_service_action_rejected,
    over_budget_rejected,
    no_solution_rejected,
    max_forma_flag_rejected].

init_per_suite(Config) ->
    {ok, _} = application:ensure_all_started(retinue_cli),
    ok = retinue_test_daemon:start(),
    Config.

end_per_suite(_Config) ->
    retinue_test_daemon:stop(),
    ok.

plan_requires_single_polarity(_Config) ->
    {Config, Plan, Cost} = plan_for("simple_capacity.yml", #{}),
    ?assertEqual(1, Cost),
    ExpectedPlan = #{1 => none, 2 => madurai, aura => none, exilus => none},
    ?assertEqual(ExpectedPlan, Plan),
    assert_plan_defaults(Config, Plan).

plan_requires_omni(CtConfig) ->
    {Config, Plan, Cost} = plan_for("omni_required.yml", #{}),
    ?assertEqual(4, Cost),
    ExpectedPlan = #{1 => omni, aura => none, exilus => none},
    ?assertEqual(ExpectedPlan, Plan),
    _ = write_plan_yaml("omni_required", CtConfig, {Config, Plan, Cost}).

plan_assigns_unslotted_mods(_Config) ->
    {Config, Plan, Cost} = plan_for("unslotted_mods.yml", #{}),
    ?assertEqual(1, Cost),
    SlotMods = wf_forma_plan:slot_mod_labels(Config, Plan),
    SortedSlots = lists:sort([S || {S, _} <- SlotMods]),
    ?assertEqual([1, 2], SortedSlots),
    ?assert(mod_slot_present(SlotMods, <<"Primary">>, <<"Madurai One">>)),
    ?assert(mod_slot_present(SlotMods, <<"Primary">>, <<"Vazarin One">>)).

plan_allows_optional_slot(_Config) ->
    {Config, Plan, Cost} = plan_for("optional_slot.yml", #{}),
    ?assertEqual(0, Cost),
    ExpectedPlan = #{1 => none, 2 => none, aura => none, exilus => none},
    ?assertEqual(ExpectedPlan, Plan),
    assert_plan_defaults(Config, Plan),
    _ = write_plan_yaml("optional_slot", _Config, {Config, Plan, Cost}).

mod_lookup_fills_missing_fields(_Config) ->
    Config = config_for("mod_lookup_missing_fields.yml"),
    [Build] = maps:get(builds, Config),
    [Mod] = maps:get(mods, Build),
    ?assertEqual(madurai, maps:get(polarity, Mod)),
    ?assertEqual(7, maps:get(cost, Mod)).

mod_lookup_preserves_cost(_Config) ->
    Config = config_for("mod_lookup_preserve_cost.yml"),
    [Build] = maps:get(builds, Config),
    [Mod] = maps:get(mods, Build),
    ?assertEqual(madurai, maps:get(polarity, Mod)),
    ?assertEqual(5, maps:get(cost, Mod)).

mod_lookup_warns_on_mismatch(_Config) ->
    Events = capture_warnings(fun() -> _ = config_for("mod_lookup_warn_mismatch.yml") end),
    Output = string:join([warning_text(Event) || Event <- Events], "\n"),
    ?assert(string:find(Output, "polarity") =/= nomatch),
    ?assert(string:find(Output, "cost") =/= nomatch).

plan_movable_aura_reuse(CtConfig) ->
    {Config, Plan, Cost} = plan_for("movable_aura.yml", #{}),
    %% Move existing V from aura to slot 1, apply one Forma to aura -> D.
    ?assertEqual(1, Cost),
    ExpectedPlan = #{1 => madurai, 2 => none, aura => vazarin, exilus => none},
    ?assertEqual(ExpectedPlan, Plan),
    _ = write_plan_yaml("movable_aura", CtConfig, {Config, Plan, Cost}).

plan_complex_dual_build(CtConfig) ->
    {Config, Plan, Cost} = plan_for("complex_dual_build.yml", #{}),
    ?assertEqual(8, Cost),
    ExpectedPlan = #{1 => madurai, 2 => vazarin, 3 => omni, 4 => omni, 5 => none, 6 => none, aura => madurai, exilus => none},
    ?assertEqual(ExpectedPlan, Plan),
    _ = write_plan_yaml("complex_dual_build", CtConfig, {Config, Plan, Cost}).

plan_complex_three_build(CtConfig) ->
    {Config, Plan, Cost} = plan_for("complex_three_build.yml", #{}),
    ?assertEqual(7, Cost),
    ExpectedPlan = #{1 => madurai, 2 => vazarin, 3 => zenurik, 4 => naramon,
                     aura => omni, exilus => naramon},
    ?assertEqual(ExpectedPlan, Plan),
    ?assertEqual(
       {ok,
        #{1 => [{<<"Endurance">>, <<"Endurance D1">>},
                {<<"Range">>, <<"Range V1">>},
                {<<"Strength">>, <<"Strength V1">>}],
          2 => [{<<"Endurance">>, <<"Endurance V2">>},
                {<<"Range">>, <<"Range D2">>},
                {<<"Strength">>, <<"Strength D2">>}],
          3 => [{<<"Endurance">>, <<"Endurance Z3">>},
                {<<"Range">>, <<"Range D3">>},
                {<<"Strength">>, <<"Strength V3">>}],
          4 => [{<<"Endurance">>, <<"Endurance N4">>},
                {<<"Range">>, <<"Range Z4">>},
                {<<"Strength">>, <<"Strength D4">>}],
          aura => [{<<"Endurance">>, <<"Endurance Aura">>},
                   {<<"Range">>, <<"Range Aura">>},
                   {<<"Strength">>, <<"Strength Aura">>}],
          exilus => [{<<"Endurance">>, <<"Endurance Exilus">>},
                     {<<"Range">>, <<"Range Exilus">>},
                     {<<"Strength">>, <<"Strength Exilus">>}]}},
       wf_forma_planner:assignments_for_plan(Config, Plan)),
    Generated = write_plan_yaml("complex_three_build", CtConfig, {Config, Plan, Cost}),
    _ = read_yaml_doc(Generated).

plan_wisp_regression(CtConfig) ->
    {Config, Plan, Cost} = plan_for("wisp.yml", #{}),
    ?assertEqual(2, Cost),
    ExpectedPlan = #{1 => madurai, 2 => madurai, 3 => naramon, 4 => vazarin,
                     5 => naramon, 6 => madurai, 7 => madurai, 8 => madurai,
                     aura => madurai, exilus => none},
    ?assertEqual(ExpectedPlan, Plan),
    ?assertEqual({error, no_plan}, wf_forma_planner:plan(Config, #{max_forma => 1})),
    Generated = write_plan_yaml("wisp", CtConfig, {Config, Plan, Cost}),
    ?assertEqual(
       normalize_plan_doc(read_yaml_doc(fixture("wisp.plan.expected.yml"))),
       normalize_plan_doc(read_yaml_doc(Generated))).

yaml_output_serialization(CtConfig) ->
    {Config, Plan, Cost} = plan_for("simple_capacity.yml", #{}),
    File = write_plan_yaml("simple_capacity", CtConfig, {Config, Plan, Cost}),
    {ok, Bin} = file:read_file(File),
    [Doc] = yamerl_constr:string(Bin, [{map_node_format, map}, {str_node_as_binary, true}]),
    ?assertEqual("simple_capacity.yml", config_basename(maps:get(<<"config">>, Doc))),
    ?assertEqual(1, maps:get(<<"forma_cost">>, Doc)),
    PlanEntries = maps:get(<<"plan">>, Doc),
    Slot1 = expect_slot(PlanEntries, 1),
    Slot2 = expect_slot(PlanEntries, 2),
    ?assert(is_null(maps:get(<<"polarity">>, Slot1))),
    ?assertEqual(<<"V">>, maps:get(<<"polarity">>, Slot2)),
    SlotMods = maps:get(<<"slot_mods">>, Doc),
    Slot1Mods = maps:get(<<"mods">>, expect_slot(SlotMods, 1)),
    Slot2Mods = maps:get(<<"mods">>, expect_slot(SlotMods, 2)),
    ?assert(mod_present(Slot1Mods, <<"Primary">>, <<"Madurai One">>)),
    ?assert(mod_present(Slot2Mods, <<"Primary">>, <<"Madurai Two">>)).

independent_plan_documents(_CtConfig) ->
    {Config, Plan, Cost} = plan_for("simple_capacity.yml", #{}),
    Special = Config#{file := "name: with # marks.yml",
                      computed_slot_mods := [{1, [{<<"Build: ON #1">>, <<"Mod: quoted">>}]}]},
    Bin = iolist_to_binary(wf_forma_plan:plans_to_yaml(
                            [{ok, Config, Plan, Cost}, {ok, Special, Plan, Cost}])),
    {ok, [First, Second]} = wf_visualize:load_plan(Bin),
    ?assertEqual(maps:get(file, Config), maps:get(config, First)),
    ?assertEqual("name: with # marks.yml", maps:get(config, Second)),
    ?assertEqual([{1, [{"Build: ON #1", "Mod: quoted"}]}], maps:get(slot_mods, Second)),
    ?assertEqual(Plan, maps:get(plan, First)),
    ?assertEqual(Plan, maps:get(plan, Second)).

default_plan_output_path(CtConfig) ->
    Priv = proplists:get_value(priv_dir, CtConfig, "."),
    ConfigPath = filename:join(Priv, "simple_capacity.yml"),
    Cwd = filename:join(Priv, "cwd"),
    ok = filelib:ensure_dir(filename:join(Cwd, "dummy")),
    {ok, _} = file:copy(fixture("simple_capacity.yml"), ConfigPath),
    Expected = filename:absname(filename:join(Priv, "simple_capacity.plan.yml")),
    Output = with_cwd(Cwd, fun() ->
        capture_output(fun() ->
            retinue_cli:main(warframe, ["forma-plan","--config", ConfigPath])
        end)
    end),
    ?assert(filelib:is_file(Expected)),
    ?assert(string:find(Output, "simple_capacity.plan.yml") =/= nomatch),
    ?assert(string:find(Output, ".plan.plan.yml") =:= nomatch).

visualize_plan_html(CtConfig) ->
    {Config, Plan, _Cost} = plan_for("simple_capacity.yml", #{}),
    SlotMods = wf_forma_plan:slot_mod_labels(Config, Plan),
    BuildArcanes = wf_forma_plan:build_arcane_entries(Config),
    Priv = proplists:get_value(priv_dir, CtConfig, "."),
    Target = filename:join(Priv, "simple_capacity.plan.html"),
    {ok, Path} = wf_forma_visualizer:render_html("simple_capacity.yml", Plan, SlotMods, BuildArcanes, Target),
    {ok, Bin} = file:read_file(Path),
    ?assert(byte_size(Bin) > 0).

default_visualization_paths(CtConfig) ->
    {Config, Plan, _Cost} = plan_for("simple_capacity.yml", #{}),
    SlotMods = wf_forma_plan:slot_mod_labels(Config, Plan),
    BuildArcanes = wf_forma_plan:build_arcane_entries(Config),
    Priv = proplists:get_value(priv_dir, CtConfig, "."),
    SourceDir = filename:join(Priv, "viz-src"),
    ok = filelib:ensure_dir(filename:join(SourceDir, "dummy")),
    SourceConfig = filename:join(SourceDir, "simple_capacity.yml"),
    SourcePlan = filename:join(SourceDir, "simple_capacity.plan.yml"),
    with_cwd(Priv, fun() ->
        {ok, HtmlPath} = wf_forma_visualizer:render_html(SourceConfig, Plan, SlotMods, BuildArcanes, undefined),
        ?assertEqual(filename:absname(filename:join(SourceDir, "simple_capacity.plan.html")), HtmlPath),
        ?assert(filelib:is_file(HtmlPath)),
        {ok, PlanHtmlPath} = wf_forma_visualizer:render_html(SourcePlan, Plan, SlotMods, BuildArcanes, undefined),
        ?assertEqual(filename:absname(filename:join(SourceDir, "simple_capacity.plan.html")), PlanHtmlPath),
        {ok, ConfigHtmlPath} = wf_forma_visualizer:render_config_html(SourceConfig, Plan, SlotMods, BuildArcanes, undefined),
        ?assertEqual(filename:absname(filename:join(SourceDir, "simple_capacity.config.html")), ConfigHtmlPath),
        ?assert(filelib:is_file(ConfigHtmlPath)),
        ok = wf_forma_visualizer:show_config_plan(SourceConfig, Plan, SlotMods, BuildArcanes),
        ?assert(filelib:is_file(filename:absname(filename:join(SourceDir, "simple_capacity.config.html"))))
    end).

visualize_config_html(CtConfig) ->
    {Config, _Plan, _Cost} = plan_for("simple_capacity.yml", #{}),
    Priv = proplists:get_value(priv_dir, CtConfig, "."),
    Target = filename:join(Priv, "simple_capacity.config.html"),
    PlanCfg = maps:get(computed_current_plan, Config),
    SlotMods = maps:get(computed_current_slot_mods, Config),
    BuildArcanes = wf_forma_plan:build_arcane_entries(Config),
    {ok, Path} = wf_forma_visualizer:render_config_html("simple_capacity.yml", PlanCfg, SlotMods, BuildArcanes, Target),
    ?assert(filelib:is_file(Path)).

invalid_config_errors(_Config) ->
    Path = fixture("invalid_config.yml"),
    ?assertMatch({error, _}, wf_forma_config:load_files([Path])).

unsupported_service_action_rejected(_Config) ->
    Path = fixture("simple_capacity.yml"),
    ?assertEqual({error, {unsupported_forma_action, delete_everything}},
                 wf_forma_service:plan_request(
                   #{configs => [Path], action => delete_everything})).

over_budget_rejected(_Config) ->
    Path = fixture("over_budget.yml"),
    {ok, [Raw]} = wf_forma_config:load_files([Path]),
    {ok, Config} = wf_forma_model:normalize_config(Raw),
    ?assertEqual({error, no_plan}, wf_forma_planner:plan(Config, #{})).

no_solution_rejected(_Config) ->
    Path = fixture("no_solution.yml"),
    {ok, [Raw]} = wf_forma_config:load_files([Path]),
    {ok, Config} = wf_forma_model:normalize_config(Raw),
    ?assertEqual({error, no_plan}, wf_forma_planner:plan(Config, #{})).

max_forma_flag_rejected(_Config) ->
    Path = fixture("simple_capacity.yml"),
    {ok, [Raw]} = wf_forma_config:load_files([Path]),
    {ok, Config} = wf_forma_model:normalize_config(Raw),
    {Plan, Cost} = wf_forma_planner:plan(Config, #{}),
    ?assert(is_map(Plan)),
    ?assert(Cost > 0),
    ?assertEqual({error, no_plan}, wf_forma_planner:plan(Config, #{max_forma => 0})).

plan_for(Name, Flags) ->
    Path = fixture(Name),
    {ok, #{results := [{ok, Config, Plan, Cost}]}} =
        wf_forma_service:plan_request(#{configs => [Path], flags => Flags}),
    {Config, Plan, Cost}.

config_for(Name) ->
    Path = fixture(Name),
    {ok, [Raw]} = wf_forma_config:load_files([Path]),
    {ok, Config} = wf_forma_model:normalize_config(Raw),
    Config.

fixture(Name) ->
    filename:join([code:lib_dir(retinue_cli), "test", "fixtures", Name]).

write_plan_yaml(Name, CtConfig, {Config, Plan, Cost}) ->
    Priv = proplists:get_value(priv_dir, CtConfig, "."),
    File = filename:join(Priv, Name ++ ".plan.yml"),
    Bin = iolist_to_binary(wf_forma_plan:plans_to_yaml([{ok, Config, Plan, Cost}])),
    ok = file:write_file(File, Bin),
    File.

read_yaml_doc(Path) ->
    {ok, Bin} = file:read_file(Path),
    [Doc] = yamerl_constr:string(Bin, [{map_node_format, map},
                                       {str_node_as_binary, true}]),
    Doc.

normalize_plan_doc(Doc) ->
    Config = unicode:characters_to_binary(config_basename(maps:get(<<"config">>, Doc))),
    Doc#{<<"config">> => Config}.

config_basename(Value) when is_binary(Value) ->
    filename:basename(binary_to_list(Value));
config_basename(Value) when is_list(Value) ->
    filename:basename(Value);
config_basename(Value) when is_atom(Value) ->
    filename:basename(atom_to_list(Value)).

expect_slot(PlanEntries, Slot) ->
    [Match | _] = [Entry || Entry <- PlanEntries, slot_match(Slot, maps:get(<<"slot">>, Entry, undefined))],
    Match.

slot_match(Slot, Slot) -> true;
slot_match(Slot, Value) ->
    normalize_slot(Value) =:= normalize_slot(Slot).

normalize_slot(Value) when is_binary(Value) ->
    binary_to_list(Value);
normalize_slot(Value) when is_atom(Value) ->
    atom_to_list(Value);
normalize_slot(Value) when is_integer(Value) ->
    integer_to_list(Value);
normalize_slot(Value) when is_list(Value) ->
    Value;
normalize_slot(Value) ->
    io_lib:format("~p", [Value]).

mod_present(List, Build, Mod) ->
    lists:any(
      fun(Entry) ->
          maps:get(<<"build">>, Entry, undefined) =:= Build andalso
          maps:get(<<"mod">>, Entry, undefined) =:= Mod
      end,
      List).

mod_slot_present(SlotMods, Build, Mod) ->
    lists:any(
      fun({_Slot, Mods}) ->
          lists:any(fun({B, M}) -> B =:= Build andalso M =:= Mod end, Mods)
      end,
      SlotMods).

assert_plan_defaults(Config, Plan) ->
    Item = maps:get(item, Config),
    AuraSlot = maps:get(aura_slot, Item, none),
    ExilusSlot = maps:get(exilus_slot, Item, none),
    ?assertEqual(AuraSlot, maps:get(aura, Plan, none)),
    ?assertEqual(ExilusSlot, maps:get(exilus, Plan, none)).

is_null(null) -> true;
is_null(undefined) -> true;
is_null(<<"null">>) -> true;
is_null(_) -> false.

capture_output(Fun) ->
    Capturer = spawn(fun() -> io_capture_loop([]) end),
    Old = group_leader(),
    group_leader(Capturer, self()),
    try
        _ = Fun()
    after
        group_leader(Old, self())
    end,
    Capturer ! {get, self()},
    receive
        {captured, Output} -> to_list(Output)
    after 1000 ->
        ""
    end.

capture_warnings(Fun) ->
    Filter = fun(Event, Pid) ->
        case maps:get(level, Event, undefined) of
            warning -> Pid ! {captured_warning, Event};
            _ -> ok
        end,
        Event
    end,
    ok = logger:add_primary_filter(?MODULE, {Filter, self()}),
    try
        _ = Fun(),
        collect_warnings([])
    after
        ok = logger:remove_primary_filter(?MODULE)
    end.

collect_warnings(Acc) ->
    receive
        {captured_warning, Event} -> collect_warnings([Event | Acc])
    after 0 ->
        lists:reverse(Acc)
    end.

warning_text(#{msg := {Format, Args}}) ->
    lists:flatten(io_lib:format(Format, Args));
warning_text(#{msg := Msg}) ->
    lists:flatten(io_lib:format("~p", [Msg])).

with_cwd(Dir, Fun) ->
    {ok, Old} = file:get_cwd(),
    ok = file:set_cwd(Dir),
    try Fun()
    after
        ok = file:set_cwd(Old)
    end.

io_capture_loop(Acc) ->
    receive
        {io_request, From, ReplyAs, Request} ->
            {NewAcc, Reply} = handle_io_request(Request, Acc),
            From ! {io_reply, ReplyAs, Reply},
            io_capture_loop(NewAcc);
        {get, Requestor} ->
            Requestor ! {captured, lists:flatten(lists:reverse(Acc))},
            io_capture_loop(Acc)
    end.

handle_io_request({put_chars, Chars}, Acc) ->
    {[Chars | Acc], ok};
handle_io_request({put_chars, _Enc, Chars}, Acc) ->
    {[Chars | Acc], ok};
handle_io_request({put_chars, Enc, Mod, Fun, Args}, Acc) ->
    handle_io_request({put_chars, Enc, apply(Mod, Fun, Args)}, Acc);
handle_io_request({put_chars, Mod, Fun, Args}, Acc) ->
    handle_io_request({put_chars, apply(Mod, Fun, Args)}, Acc);
handle_io_request({format, Format, Args}, Acc) ->
    {[io_lib:format(Format, Args) | Acc], ok};
handle_io_request({fwrite, Format, Args}, Acc) ->
    {[io_lib:format(Format, Args) | Acc], ok};
handle_io_request({requests, Reqs}, Acc) when is_list(Reqs) ->
    lists:foldl(
      fun(Req, {Acc0, _Reply0}) -> handle_io_request(Req, Acc0) end,
      {Acc, ok},
      Reqs);
handle_io_request(_Req, Acc) ->
    {Acc, ok}.

to_list(V) when is_binary(V) -> binary_to_list(V);
to_list(V) when is_atom(V) -> atom_to_list(V);
to_list(V) when is_list(V) -> V;
to_list(V) -> io_lib:format("~p", [V]).
