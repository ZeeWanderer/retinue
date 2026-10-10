-module(wfcli_stat_critical_eunit).

-include_lib("eunit/include/eunit.hrl").

native_sequence_test_() ->
    {ok, Json} = file:read_file("apps/wfcli/test/fixtures/native_critical_sequence.json"),
    Fixture = jsone:decode(Json),
    [{binary_to_list(maps:get(<<"name">>, Case)), ?_test(begin
        Hit = fields(maps:get(<<"hit">>, Case)),
        Context = fields(maps:get(<<"selection">>, Case)),
        Rolls = maps:get(<<"rolls">>, Case),
        {Selected, Tail} = wfcli_stat_math:critical_selection(Hit, Context, Rolls),
        ?assertEqual(lists:nthtail(maps:get(<<"selection_draws">>, Case), Rolls), Tail),
        ?assertEqual(fields(maps:get(<<"selected">>, Case)), outcome(Selected)),
        {Final, Remaining} = wfcli_stat_math:critical_effects(Selected,
            fields(maps:get(<<"effects">>, Case)), Tail),
        ?assertEqual(lists:nthtail(maps:get(<<"total_draws">>, Case), Rolls), Remaining),
        ?assertEqual(fields(maps:get(<<"expected">>, Case)), outcome(Final))
    end)} || Case <- maps:get(<<"cases">>, Fixture)].

stages_consume_separate_rolls_test() ->
    {Selected, [0, 42]} = wfcli_stat_math:critical_selection(hit(), context(), [8388608, 16777215, 0, 42]),
    ?assertEqual(2, maps:get(tier, Selected)),
    ?assertEqual(3.0, maps:get(factor, Selected)),
    {Final, [42]} = wfcli_stat_math:critical_effects(Selected, effects(), [0, 42]),
    ?assertEqual(3.5, maps:get(factor, Final)),
    ?assertEqual(#{2 => 1}, maps:get(forced_procs, Final)).

stored_tier_skips_selection_but_not_post_effects_test() ->
    {Selected, [17, 42]} = wfcli_stat_math:critical_selection((hit())#{tier := 1}, context(), [17, 42]),
    ?assertEqual(1, maps:get(tier, Selected)),
    ?assertEqual(2.0, maps:get(factor, Selected)),
    {Final, [42]} = wfcli_stat_math:critical_effects(Selected, effects(), [17, 42]),
    ?assertEqual(1, maps:get(tier, Final)),
    ?assertEqual(#{2 => 1}, maps:get(forced_procs, Final)).

failed_initial_critical_skips_extra_tier_roll_test() ->
    {Selected, [0, 42]} = wfcli_stat_math:critical_selection(hit(), context(), [16777215, 0, 42]),
    ?assertEqual(0, maps:get(tier, Selected)),
    {Final, [42]} = wfcli_stat_math:critical_effects(Selected, effects(), [0, 42]),
    ?assertEqual(0, maps:get(tier, Final)),
    ?assertEqual(1.25, maps:get(factor, Final)).

rolls_are_required_only_when_used_test() ->
    ?assertError(critical_rolls_exhausted, wfcli_stat_math:critical_selection(hit(), context(), [])),
    ?assertError(critical_rolls_exhausted, wfcli_stat_math:critical_selection(hit(), context(), [0])),
    {NoChance, []} = wfcli_stat_math:critical_selection((hit())#{critical_chance := 0}, context(), []),
    ?assertEqual(0, maps:get(tier, NoChance)),
    {Stored, []} = wfcli_stat_math:critical_selection((hit())#{tier := 1}, context(), []),
    ?assertError(critical_rolls_exhausted, wfcli_stat_math:critical_effects(Stored, effects(), [])),
    {Integer, []} = wfcli_stat_math:critical_selection((hit())#{critical_chance := 1},
        (context())#{extra_tier_chance := none}, []),
    ?assertEqual(1, maps:get(tier, Integer)).

bypass_override_and_stored_factor_test() ->
    Bypass = (context())#{bypass := true},
    {Skipped, []} = wfcli_stat_math:critical_selection(hit(), Bypass, []),
    ?assertEqual(0, maps:get(tier, Skipped)),
    {Allowed, []} = wfcli_stat_math:critical_selection(hit(), Bypass#{source_bypass_override := true}, [0, 0]),
    ?assertEqual(2, maps:get(tier, Allowed)),
    {Stored, []} = wfcli_stat_math:critical_selection((hit())#{tier := 3}, Bypass, []),
    ?assertEqual(4.0, maps:get(factor, Stored)).

hit() ->
    #{tier => 0, factor => 1, critical_chance => 0.5, critical_multiplier => 2,
      has_weapon => true, self_hit => false, hit_type => 0, proc_type => 30,
      active_status_types => [], forced_procs => #{}}.

context() -> #{bypass => false, source_bypass_override => false, extra_tier_chance => 1}.

effects() ->
    #{slash_chance => 1, low_status_bonus => 0.5, low_status_limit => 3,
      noncritical_chance => 1, noncritical_bonus => 0.25}.

outcome(Hit) -> maps:with([tier, factor, forced_procs], Hit).

fields(Map) ->
    {module, wfcli_stat_math} = code:ensure_loaded(wfcli_stat_math),
    maps:from_list([case binary_to_existing_atom(Key) of
        forced_procs -> {forced_procs, maps:from_list(
            [{binary_to_integer(K), V} || {K, V} <- maps:to_list(Value)])};
        extra_tier_chance when Value =:= null -> {extra_tier_chance, none};
        factor -> {factor, float32(Value)};
        Name -> {Name, Value}
    end || {Key, Value} <- maps:to_list(Map)]).

float32(Value) ->
    <<Result:32/float>> = <<(float(Value)):32/float>>,
    Result.
