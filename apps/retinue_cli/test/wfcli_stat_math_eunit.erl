-module(wfcli_stat_math_eunit).

-include_lib("eunit/include/eunit.hrl").

scalar_stages_test() ->
    Ops = [{add, 10}, {add_base, 20}, {stacking_multiply, 0.5},
           {stacking_multiply, 0.25}, {multiply, 2}, {multiply, 0.5}],
    ?assertEqual(220.0, wfcli_stat_math:scalar(100, Ops)),
    ?assertEqual(150.0, wfcli_stat_math:scalar(100, Ops ++ [{set_max, 150}])),
    ?assertEqual(250.0, wfcli_stat_math:scalar(100, Ops ++ [{set_min, 250}])),
    ?assertEqual(26.0, wfcli_stat_math:scalar(100, [{set_base, 3}, {add_base, 10},
                                                  {multiply, 2}])).

set_short_circuits_test() ->
    ?assertEqual(8.0, wfcli_stat_math:scalar(100, [{set_min, 30}, {set, 8},
                                                 {set, 4}, {multiply, 20}])).

direct_updates_do_not_stop_at_set_test() ->
    Entries = [{set, 0.75}, {multiply, 2}, {set, 0.5}],
    ?assertEqual(0.75, wfcli_stat_math:scalar(0.05, Entries)),
    ?assertEqual(0.5, wfcli_stat_math:scalar_updates(0.05, Entries)),
    ?assertEqual(0.75, wfcli_stat_math:scalar_updates(0.05,
        [{set, 0.75}, {multiply, 2}, {set_base, 3}, {set_min, 4}])).

native_weakness_updates_test_() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/native_weakness_updates.json"),
    #{<<"cases">> := Cases} = jsone:decode(Json),
    [{lists:flatten(io_lib:format("weakness ~p ~p", [Base, Entries])),
      ?_assertEqual(float32(Expected), wfcli_stat_math:scalar_updates(Base,
          [{binary_to_existing_atom(Op), Value} || [Op, Value] <- Entries]))}
     || #{<<"base">> := Base, <<"entries">> := Entries, <<"expected">> := Expected} <- Cases].

native_scalar_boundaries_test_() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/native_scalar_boundaries.json"),
    #{<<"cases">> := Cases} = jsone:decode(Json),
    [{binary_to_list(Name), ?_assertEqual(float32(Expected),
        wfcli_stat_math:scalar_updates(Base, lists:map(fun
            (<<"settle">>) -> settle;
            ([Op, Value]) -> {binary_to_existing_atom(Op), Value}
        end, Updates)))}
     || #{<<"name">> := Name, <<"base">> := Base, <<"updates">> := Updates,
          <<"expected">> := Expected} <- Cases].

settle_discards_previous_accumulator_state_test() ->
    ?assertEqual(16.0, wfcli_stat_math:scalar_updates(10,
        [{set, 8}, settle, {multiply, 2}])),
    ?assertEqual(10.0, wfcli_stat_math:scalar_updates(10,
        [{set_max, 5}, settle, {multiply, 2}])),
    ?assertEqual(10.0, wfcli_stat_math:scalar_updates(10, [settle, settle])).

deferred_modifiers_preserve_native_precision_test() ->
    ?assertEqual(56.54999923706055, wfcli_stat_math:scalar_updates(29,
        [{multiply, 1.3}, {multiply, 1.5}])),
    ?assertEqual(56.54999542236328, wfcli_stat_math:scalar_updates(29,
        [{multiply, 1.3}, settle, {multiply, 1.5}])).

native_minimum_clamp_initial_value_test() ->
    Minimum = 1.1754943508222875e-38,
    ?assertEqual(0.0, wfcli_stat_math:scalar(0, [])),
    ?assertEqual(Minimum, wfcli_stat_math:scalar(0, [{set_min, 0}])),
    ?assertEqual(0.0, wfcli_stat_math:scalar(0, [{set_min, 0}, {set_max, 0}])).

native_critical_multiplier_test_() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/native_critical_multiplier.json"),
    #{<<"cases">> := Cases} = jsone:decode(Json),
    lists:map(fun(#{<<"name">> := Name, <<"base">> := Base, <<"entries">> := Entries,
                    <<"policy">> := Policy, <<"expected">> := Expected}) ->
        {binary_to_list(Policy) ++ " " ++ binary_to_list(Name), ?_assertEqual(float32(Expected),
            wfcli_stat_math:critical_multiplier(Base,
                [{binary_to_existing_atom(Op), Value} || [Op, Value] <- Entries],
                binary_to_existing_atom(Policy)))}
    end, Cases).

critical_quantization_precedes_percent_bonuses_test() ->
    Actual = wfcli_stat_math:critical_multiplier(2.4, [{stacking_multiply, 1.5}], quantized),
    ?assertEqual(5.997558116912842, Actual),
    ?assertNotEqual(wfcli_stat_math:critical_multiplier(6, [], quantized), Actual),
    ?assert(wfcli_stat_math:critical_multiplier(2.4, [{stacking_multiply, 20}], quantized) > 32).

native_critical_factor_test_() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/native_critical_factor.json"),
    #{<<"cases">> := Cases} = jsone:decode(Json),
    lists:map(fun(#{<<"factor">> := Factor, <<"multiplier">> := Multiplier,
                    <<"tier">> := Tier, <<"expected">> := Expected}) ->
        {lists:flatten(io_lib:format("factor ~p multiplier ~p tier ~p",
                                    [Factor, Multiplier, Tier])),
         ?_assertEqual(float32(Expected),
                       wfcli_stat_math:critical_factor(Factor, Multiplier, Tier))}
    end, Cases).

critical_factor_preserves_existing_contribution_test() ->
    ?assertEqual(4.0, wfcli_stat_math:critical_factor(3, 1.5, 2)),
    ?assertEqual(3.0, wfcli_stat_math:critical_factor(3, 1.5, 0)),
    ?assertEqual(-0.25, wfcli_stat_math:critical_factor(0, 0.75, 1)),
    ?assertError(function_clause, wfcli_stat_math:critical_factor(1, 1.5, -1)),
    ?assertError(function_clause, wfcli_stat_math:critical_factor(1, 1.5, 1.5)).

native_critical_selection_test_() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/native_critical_selection.json"),
    #{<<"cases">> := Cases, <<"roll_bins">> := Bins} = jsone:decode(Json),
    [{float_to_list(float(Chance), [short]), ?_test(begin
        Expected = [#{tier => T, probability => Count / Bins}
                    || #{<<"tier">> := T, <<"roll_count">> := Count} <- Outcomes],
        Actual = wfcli_stat_math:critical_tiers(Chance),
        ?assertEqual(Expected, Actual),
        ?assertEqual(1.0, lists:sum([P || #{probability := P} <- Actual]))
    end)} || #{<<"chance">> := Chance, <<"expected">> := Outcomes} <- Cases].

critical_selection_boundary_test() ->
    ?assertEqual([#{tier => 0, probability => 1.0}], wfcli_stat_math:critical_tiers(0)),
    ?assertEqual([#{tier => 1, probability => 1.0}], wfcli_stat_math:critical_tiers(1)),
    ?assertEqual([#{tier => 2, probability => 1.0}], wfcli_stat_math:critical_tiers(2)),
    ?assertEqual([#{tier => 0, probability => 0.4999999403953552},
                  #{tier => 1, probability => 0.5000000596046448}],
                 wfcli_stat_math:critical_tiers(0.5)),
    ?assertError({unsupported_critical_chance, 2147483647},
                 wfcli_stat_math:critical_tiers(2147483647)).

health_outcomes_round_before_averaging_test() ->
    Packet = #{scalar => 1.4, channels => [{impact, 1}], quantization => quantized},
    Critical = #{chance => 0.5, multiplier => 1.5, factor => 1},
    Result = wfcli_stat_math:health_outcomes(Packet, Critical,
        #{multipliers => #{}, armor_reduction => 0}),
    Outcomes = maps:get(outcomes, Result),
    ?assertEqual([1, 2], [maps:get(health_amount, Hit) || #{hit := Hit} <- Outcomes]),
    Expected = maps:get(expected_health_amount, Result),
    ?assertEqual(1.5000000596046448, Expected),
    AveragedThenRounded = wfcli_stat_math:damage_amount(lists:sum(
        [P * maps:get(raw_amount, Hit) || #{probability := P, hit := Hit} <- Outcomes])),
    ?assertEqual(2, AveragedThenRounded),
    ?assertNotEqual(float(AveragedThenRounded), Expected).

health_outcomes_preserve_context_factor_test() ->
    Packet = #{scalar => 10, channels => [{impact, 1}], quantization => quantized},
    Result = wfcli_stat_math:health_outcomes(Packet,
        #{chance => 2, multiplier => 1.5, factor => 3},
        #{multipliers => #{}, armor_reduction => 0}),
    ?assertMatch(#{expected_health_amount := 40.0,
        outcomes := [#{tier := 2, factor := 4.0, probability := 1.0}]}, Result),
    Clamped = wfcli_stat_math:health_outcomes(Packet,
        #{chance => 3, multiplier => 0.5, factor => 1},
        #{multipliers => #{}, armor_reduction => 0}),
    ?assertMatch(#{expected_health_amount := +0.0,
        outcomes := [#{tier := 3, factor := +0.0}]}, Clamped).

karak_critical_outcomes_test() ->
    Source = wfcli_stat_math:source_packet(
        #{scalar => 29, channels => [{impact, 0.45}, {puncture, 0.3}, {slash, 0.25}]},
        [#{stat => damage, operation => stacking_multiply, value => 1.35}], []),
    Multiplier = wfcli_stat_math:critical_multiplier(1.5, [], quantized),
    Result = wfcli_stat_math:health_outcomes(Source#{quantization => quantized},
        #{chance => 0.5, multiplier => Multiplier, factor => 1},
        #{multipliers => #{impact => 1.5}, armor_reduction => 0.4298836886882782}),
    ?assertEqual([47, 71], [maps:get(health_amount, Hit)
                           || #{hit := Hit} <- maps:get(outcomes, Result)]),
    ?assertEqual(59.000001430511475, maps:get(expected_health_amount, Result)).

linear_rank_test() ->
    near(1.87, wfcli_stat_math:ranked(stacking_multiply, 0.17, 10, 10)),
    near(0.17, wfcli_stat_math:ranked(stacking_multiply, 0.17, 0, 10)),
    near(0.5, wfcli_stat_math:ranked(multiply, 0.9, 4, 5)),
    ?assertEqual(0.0, wfcli_stat_math:ranked(multiply, 0.5, 5, 5)),
    near(0.2, wfcli_stat_math:ranked(add, 0.2, 0, 0)).

pairing_respects_priority_test() ->
    Base = #{heat => 10, cold => 20, toxin => 30, electricity => 40, radiation => 5},
    ?assertEqual(#{viral => 50, radiation => 55},
                 wfcli_stat_math:pair_elements(Base, [toxin, cold, heat, electricity])),
    ?assertEqual(#{blast => 30, corrosive => 70, radiation => 5},
                 wfcli_stat_math:pair_elements(Base, [heat, cold, toxin, electricity])),
    ?assertEqual(#{heat => 10, radiation => 5},
                 wfcli_stat_math:pair_elements(#{heat => 10, radiation => 5}, [])),
    ?assertEqual(#{gas => 30},
                 wfcli_stat_math:pair_elements(#{heat => 10, toxin => 20}, [heat, heat, toxin])),
    ?assertEqual(#{gas => 30, magnetic => 40, radiation => 5},
        wfcli_stat_math:pair_elements(#{heat => 10, toxin => 20, radiation => 5, magnetic => 40},
                                     [radiation, heat, magnetic, toxin])).

npc_armor_test() ->
    ?assertEqual(0.0, wfcli_stat_math:armor_reduction(0)),
    near(0.3, wfcli_stat_math:armor_reduction(300)),
    near(0.9, wfcli_stat_math:armor_reduction(2700)),
    near(0.95, wfcli_stat_math:armor_reduction(5700)).

native_armor_test() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/native_armor.json"),
    #{<<"range">> := #{<<"start">> := Start, <<"end">> := End,
                         <<"float32_le_sha256">> := Digest}, <<"cases">> := Cases} = jsone:decode(Json),
    Results = << <<(wfcli_stat_math:armor_reduction(A)):32/float-little>>
                 || A <- lists:seq(Start, End) >>,
    ?assertEqual(Digest, binary:encode_hex(crypto:hash(sha256, Results), lowercase)),
    [?assertEqual(float32(R), wfcli_stat_math:armor_reduction(A))
     || #{<<"armor">> := A, <<"reduction">> := R} <- Cases].

native_source_packet_test_() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/native_source_packet.json"),
    #{<<"cases">> := Cases} = jsone:decode(Json),
    [{binary_to_list(Name), ?_test(begin
        Packet = #{scalar => Scalar,
                   channels => [{binary_to_existing_atom(C), V} || [C, V] <- Channels]},
        Effects = [#{stat => source_stat(S), operation => binary_to_existing_atom(Op), value => V}
                   || #{<<"stat">> := S, <<"operation">> := Op, <<"value">> := V} <- Entries],
        Actual = wfcli_stat_math:source_packet(Packet, Effects,
            [binary_to_existing_atom(C) || C <- Priority]),
        ?assertEqual(float32(maps:get(<<"scalar">>, Expected)), maps:get(scalar, Actual)),
        ExpectedChannels = maps:from_list([{binary_to_existing_atom(C), float32(V)}
            || {C, V} <- maps:to_list(maps:get(<<"channels">>, Expected))]),
        ?assertEqual(ExpectedChannels, maps:from_list(maps:get(channels, Actual)))
    end)} || #{<<"name">> := Name, <<"packet">> := #{<<"scalar">> := Scalar,
               <<"channels">> := Channels}, <<"effects">> := Entries,
               <<"priority">> := Priority, <<"expected">> := Expected} <- Cases].

source_stat(<<"base_damage">>) -> base_damage;
source_stat(<<"damage">>) -> damage;
source_stat(Channel) -> {element, binary_to_existing_atom(Channel)}.

source_packet_canonical_channel_order_test() ->
    Packet = #{scalar => 100, channels => [{slash, 0.5}, {impact, 0.2}, {puncture, 0.3}]},
    Result = wfcli_stat_math:source_packet(Packet, [], []),
    ?assertEqual([impact, puncture, slash], [C || {C, _} <- maps:get(channels, Result)]).

source_packet_rejects_ambiguous_channels_test() ->
    Packet = #{scalar => 100, channels => [{impact, 0.4}, {impact, 0.6}]},
    ?assertError({badmatch, false}, wfcli_stat_math:source_packet(Packet, [], [])),
    ?assertError({badmatch, false}, wfcli_stat_math:source_packet(
        Packet#{channels => [{unknown, 1}]}, [], [])).

source_packet_rescales_before_quantization_test() ->
    Packet = #{scalar => 100, channels => [{impact, 0.2}, {puncture, 0.3}, {slash, 0.5}]},
    Effects = [#{stat => {element, C}, operation => add, value => V}
               || {C, V} <- [{heat, 8}, {cold, 8}, {blast, 1}]],
    Source = wfcli_stat_math:source_packet(Packet, Effects, [heat, cold]),
    Weights = maps:from_list(maps:get(channels, Source)),
    ?assertEqual(106.45792388916016, maps:get(scalar, Source)),
    ?assertEqual(15.968750953674316, maps:get(blast, Weights)),
    ?assertEqual(15.96875, maps:get(blast, wfcli_stat_math:hit_channels(Weights, quantized))).

source_packet_composes_with_native_health_test() ->
    Base = #{scalar => 29, channels => [{impact, 0.45}, {puncture, 0.30}, {slash, 0.25}]},
    Source = wfcli_stat_math:source_packet(Base,
        [#{stat => damage, operation => stacking_multiply, value => 1.35}], []),
    Hit = wfcli_stat_math:health_hit(Source#{quantization => quantized},
        #{multipliers => #{impact => 1.5}, armor_reduction => 0.4298836886882782}),
    ?assertEqual(47, maps:get(health_amount, Hit)).

per_status_contribution_uses_separate_base_test() ->
    Step = wfcli_stat_math:ranked(stacking_multiply, 0.036363602, 8, 10),
    TwoKills = [{stacking_multiply, Step}, {stacking_multiply, Step}],
    Modded = wfcli_stat_math:scalar(29, [{stacking_multiply, 1.35}]),
    ?assertEqual(Modded, wfcli_stat_math:per_status_damage(29, Modded, [])),
    near(47.9818, wfcli_stat_math:per_status_damage(29, 29, [TwoKills])),
    near(87.1318, wfcli_stat_math:per_status_damage(29, Modded, [TwoKills])),
    near(125.0954, wfcli_stat_math:per_status_damage(29, Modded,
                                                   [TwoKills, TwoKills, TwoKills])),
    ?assertEqual(60.0, wfcli_stat_math:per_status_damage(20, 50,
        [[{add, 4}], [{stacking_multiply, 0.5}], [{multiply, 0.8}]])),
    ?assertEqual(50.0, wfcli_stat_math:per_status_damage(20, 50, [[], []])).

hit_channels_quantization_test() ->
    ?assertEqual(#{impact => 0.4375, puncture => 0.3125, slash => 0.25},
        wfcli_stat_math:hit_channels(#{impact => 0.45, puncture => 0.30, slash => 0.25},
                                    quantized)),
    Cases = [{-1, 0.0}, {0, 0.0}, {15.96875, 15.96875}, {16, 15.96875},
             {100, 15.96875}, {0.015624999, 0.03125}, {0.015625, 0.03125},
             {0.046875, 0.0625}],
    lists:foreach(fun({Input, Expected}) ->
        ?assertEqual(#{impact => Expected},
                     wfcli_stat_math:hit_channels(#{impact => Input}, quantized))
    end, Cases).

hit_channels_do_not_renormalize_test() ->
    Channels = wfcli_stat_math:hit_channels(
        #{impact => 0.34, puncture => 0.33, slash => 0.33}, quantized),
    ?assertEqual(#{impact => 0.34375, puncture => 0.34375, slash => 0.34375}, Channels),
    ?assertEqual(1.03125, lists:sum(maps:values(Channels))),
    ?assertEqual(#{}, wfcli_stat_math:hit_channels(#{}, quantized)).

hit_channels_bypass_test() ->
    Channels = #{impact => 0.45, puncture => 0.30, slash => 0.25},
    ?assertEqual(maps:map(fun(_, Value) -> float32(Value) end, Channels),
                 wfcli_stat_math:hit_channels(Channels, unquantized)).

karak_native_hit_quantization_test() ->
    %% e546 native replay: target dispatch quantizes channels before mitigation.
    Scalar = wfcli_stat_math:scalar(29, [{stacking_multiply, 1.35}]),
    ?assertEqual(68.14999389648438, Scalar),
    Packet = #{scalar => Scalar, channels => [{impact, 0.45}, {puncture, 0.30}, {slash, 0.25}],
               quantization => quantized},
    Target = #{multipliers => #{impact => 1.5}, armor_reduction => 0.4298836886882782},
    Plain = wfcli_stat_math:health_hit(Packet#{quantization => unquantized}, Target),
    Quantized = wfcli_stat_math:health_hit(Packet, Target),
    ?assertEqual(47.59543991088867, maps:get(raw_amount, Plain)),
    ?assertEqual(48, maps:get(health_amount, Plain)),
    ?assertEqual(47.35260772705078, maps:get(raw_amount, Quantized)),
    ?assertEqual(47, maps:get(health_amount, Quantized)),
    ?assertEqual(Scalar, maps:get(scalar, Quantized)).

karak_recorded_status_progression_test() ->
    Step = wfcli_stat_math:ranked(stacking_multiply, 0.036363602, 8, 10),
    TwoKills = [{stacking_multiply, Step}, {stacking_multiply, Step}],
    Modded = wfcli_stat_math:scalar(29, [{stacking_multiply, 1.35}]),
    Target = #{multipliers => #{impact => 1.5}, armor_reduction => 0.4298836886882782},
    Amounts = [begin
        Scalar = wfcli_stat_math:per_status_damage(29, Modded, lists:duplicate(Kinds, TwoKills)),
        Packet = #{scalar => Scalar, channels => [{impact, 0.45}, {puncture, 0.30}, {slash, 0.25}],
                   quantization => quantized},
        maps:get(health_amount, wfcli_stat_math:health_hit(Packet, Target))
    end || Kinds <- lists:seq(0, 3)],
    ?assertEqual([47, 61, 74, 87], Amounts).

karak_recorded_critical_hit_test() ->
    Scalar = wfcli_stat_math:scalar(29, [{stacking_multiply, 1.35}]),
    Multiplier = wfcli_stat_math:critical_multiplier(1.5, [], quantized),
    Factor = wfcli_stat_math:critical_factor(1, Multiplier, 1),
    HitScalar = wfcli_stat_math:scalar(Scalar, [{multiply, max(0, Factor)}]),
    Packet = #{scalar => HitScalar,
               channels => [{impact, 0.45}, {puncture, 0.30}, {slash, 0.25}],
               quantization => quantized},
    Target = #{multipliers => #{impact => 1.5}, armor_reduction => 0.4298836886882782},
    ?assertEqual(71, maps:get(health_amount, wfcli_stat_math:health_hit(Packet, Target))).

health_hit_rounds_sum_not_individual_channels_test() ->
    Packet = #{scalar => 1, channels => [{impact, 0.4}, {puncture, 0.4}],
               quantization => unquantized},
    Hit = wfcli_stat_math:health_hit(Packet, #{multipliers => #{}, armor_reduction => 0}),
    ?assertEqual(1, maps:get(health_amount, Hit)),
    ?assertEqual(0, lists:sum([wfcli_stat_math:damage_amount(V)
                              || V <- maps:values(maps:get(applied_channels, Hit))])).

health_hit_preserves_native_channel_order_test() ->
    Packet = #{scalar => 1, channels => [{impact, 16777216}, {puncture, 1}, {slash, 1}],
               quantization => unquantized},
    Target = #{multipliers => #{}, armor_reduction => 0},
    Forward = wfcli_stat_math:health_hit(Packet, Target),
    Reverse = wfcli_stat_math:health_hit(
        Packet#{channels => lists:reverse(maps:get(channels, Packet))}, Target),
    ?assertEqual(16777216.0, maps:get(raw_amount, Forward)),
    ?assertEqual(16777218.0, maps:get(raw_amount, Reverse)).

health_hit_duplicate_channels_rejected_test() ->
    Packet = #{scalar => 1, channels => [{impact, 0.4}, {impact, 0.4}],
               quantization => unquantized},
    ?assertError({badmatch, false}, wfcli_stat_math:health_hit(Packet,
        #{multipliers => #{}, armor_reduction => 0})).

health_hit_neutral_and_zero_damage_test() ->
    Packet = #{scalar => 32, channels => [{impact, 0.5}, {puncture, 0.5}],
               quantization => quantized},
    Target = #{multipliers => #{}, armor_reduction => 0},
    ?assertEqual(32, maps:get(health_amount, wfcli_stat_math:health_hit(Packet, Target))),
    lists:foreach(fun({P, T}) ->
        ?assertEqual(0, maps:get(health_amount, wfcli_stat_math:health_hit(P, T)))
    end, [{Packet#{scalar => 0}, Target}, {Packet#{channels => []}, Target},
          {Packet, Target#{multipliers => #{impact => 0, puncture => 0}}}]).

health_hit_armor_floor_test() ->
    Packet = #{scalar => 1, channels => [{impact, 0.5}, {puncture, 1}, {slash, 2}],
               quantization => unquantized},
    Target = #{multipliers => #{}, armor_reduction => 0.9},
    Hit = wfcli_stat_math:health_hit(Packet, Target),
    ?assertEqual(#{impact => 0.5, puncture => 1.0, slash => 1.0}, maps:get(applied_channels, Hit)),
    ?assertEqual(3, maps:get(health_amount, Hit)),
    ?assertEqual(Hit, wfcli_stat_math:health_hit(Packet, Target#{armor_reduction => 1})),
    Resistant = wfcli_stat_math:health_hit(Packet,
        Target#{multipliers => #{impact => 0, slash => 0.25}}),
    ?assertEqual(#{impact => 0.0, puncture => 1.0, slash => 0.5}, maps:get(applied_channels, Resistant)).

native_health_hit_test_() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/native_health_hit.json"),
    #{<<"cases">> := Cases} = jsone:decode(Json),
    [{binary_to_list(Policy) ++ " " ++ float_to_list(float(Scalar), [short]), ?_test(begin
        Packet = #{scalar => Scalar,
                   channels => [{binary_to_existing_atom(K), V} || [K, V] <- Channels],
                   quantization => binary_to_existing_atom(Policy)},
        Target = #{armor_reduction => Reduction,
                   multipliers => maps:from_list([{binary_to_existing_atom(K), V}
                                                  || {K, V} <- maps:to_list(Multipliers)])},
        Actual = wfcli_stat_math:health_hit(Packet, Target),
        ?assertEqual(float32(maps:get(<<"raw_amount">>, Expected)), maps:get(raw_amount, Actual)),
        ?assertEqual(maps:get(<<"health_amount">>, Expected), maps:get(health_amount, Actual)),
        case maps:get(<<"display_amount">>, Expected) of
            null -> ?assertEqual(0, maps:get(health_amount, Actual));
            Display -> ?assertEqual(Display, maps:get(health_amount, Actual))
        end
    end)} || #{<<"packet">> := #{<<"scalar">> := Scalar, <<"channels">> := Channels,
                                  <<"quantization">> := Policy},
               <<"target">> := #{<<"armor_reduction">> := Reduction,
                                  <<"multipliers">> := Multipliers},
               <<"expected">> := Expected} <- Cases].

float32(Value) ->
    <<Rounded:32/float>> = <<(float(Value)):32/float>>,
    Rounded.

damage_amount_float32_rounding_test() ->
    ?assertEqual(0, wfcli_stat_math:damage_amount(-3)),
    ?assertEqual(0, wfcli_stat_math:damage_amount(0.49)),
    ?assertEqual(1, wfcli_stat_math:damage_amount(0.5)),
    ?assertEqual(48, wfcli_stat_math:damage_amount(47.5)),
    ?assertEqual(47, wfcli_stat_math:damage_amount(47.49999)),
    ?assertEqual(48, wfcli_stat_math:damage_amount(47.499999)),
    ?assertEqual(8388610, wfcli_stat_math:damage_amount(8388609)),
    ?assertEqual(2147483520, wfcli_stat_math:damage_amount(1.0e100)),
    ?assertEqual(0, wfcli_stat_math:damage_amount(-1.0e100)).

native_health_packet_rounding_test() ->
    %% Executable 45fa6ad0769c, 140d16cf0 -> 1409664d0 -> HUD integer formatter.
    Cases = [{20.253379821777344, 20}, {33.510128021240234, 34},
             {46.766868591308594, 47}, {60.02362060546875, 60},
             {47.59543991088867, 48}, {60.8521842956543, 61},
             {74.10894012451172, 74}, {87.36567687988281, 87}],
    lists:foreach(fun({Amount, Integer}) ->
        ?assertEqual(Integer, wfcli_stat_math:damage_amount(Amount))
    end, Cases).

crit_tiers_and_timing_test() ->
    Base = #{damage => #{impact => 100}, critical_chance => 1.5,
             packet => #{scalar => 100, channels => [{impact, 1}], quantization => unquantized},
             critical_multiplier => 2, multishot => 2.5, status_chance => 0.2,
             magazine => 10, fire_rate => 5, reload_time => 2},
    Result = wfcli_stat_math:direct_fire(Base, [], #{armor => 2700}),
    near(2.5, maps:get(expected_critical_multiplier, Result)),
    near(625, maps:get(expected_raw_damage_per_trigger, Result)),
    near(62.5, maps:get(expected_target_damage_per_trigger, Result)),
    near(156.25, maps:get(theoretical_sustained_dps, Result)),
    ?assertEqual(0.0, maps:get(theoretical_sustained_dps,
        wfcli_stat_math:direct_fire(Base#{fire_rate => 0}, [], #{armor => 0}))).

direct_fire_rounds_outcomes_before_magazine_and_dps_test() ->
    Base = #{packet => #{scalar => 1.4, channels => [{impact, 1}], quantization => unquantized},
             critical_chance => 0.5, critical_multiplier => 1.5,
             multishot => 1, status_chance => 0, magazine => 7, fire_rate => 2, reload_time => 1},
    Result = wfcli_stat_math:direct_fire(Base, [], #{armor => 0}),
    Mean = 1.5000000596046448,
    ?assertEqual([1, 2], [N || #{health_damage := N} <- maps:get(projectile_outcomes, Result)]),
    ?assertEqual(Mean, maps:get(expected_target_damage_per_trigger, Result)),
    ?assertEqual(Mean * 7, maps:get(expected_target_damage_per_magazine, Result)),
    ?assertEqual(Mean * 2, maps:get(theoretical_burst_dps, Result)),
    ?assertEqual(Mean * 7 / 4.5, maps:get(theoretical_sustained_dps, Result)),
    ?assertNotEqual(Mean, maps:get(expected_raw_damage_per_trigger, Result)),
    Multiple = wfcli_stat_math:direct_fire(Base#{multishot => 2.5}, [], #{armor => 0}),
    ?assertEqual(maps:get(projectile_outcomes, Result), maps:get(projectile_outcomes, Multiple)),
    ?assertEqual(Mean * 2.5, maps:get(expected_target_damage_per_trigger, Multiple)).

ammo_efficiency_changes_reload_losses_not_hit_damage_test() ->
    Base = #{damage => #{impact => 100}, critical_chance => 0,
             packet => #{scalar => 100, channels => [{impact, 1}], quantization => quantized},
             critical_multiplier => 2, multishot => 1, status_chance => 0,
             magazine => 10, fire_rate => 5, reload_time => 2},
    Plain = wfcli_stat_math:direct_fire(Base, [], #{armor => 0}),
    Ammo = #{stat => ammo_consumption, operation => stacking_multiply, value => -0.5},
    Half = wfcli_stat_math:direct_fire(Base, [Ammo], #{armor => 0}),
    Free = wfcli_stat_math:direct_fire(Base, [Ammo#{value => -1}], #{armor => 0}),
    near(250, maps:get(theoretical_sustained_dps, Plain)),
    near(2000 / 6, maps:get(theoretical_sustained_dps, Half)),
    near(500, maps:get(theoretical_sustained_dps, Free)),
    near(100, maps:get(expected_raw_damage_per_trigger, Free)),
    ?assertEqual(null, maps:get(expected_target_damage_per_magazine, Free)),
    ?assertEqual(true, maps:get(magazine_unbounded, Free)),
    ?assert(is_binary(jsone:encode(Free))),
    ?assertEqual(0.0, maps:get(theoretical_sustained_dps,
        wfcli_stat_math:direct_fire(Base#{fire_rate => 0}, [Ammo#{value => -1}], #{armor => 0}))).

base_damage_evolution_precedes_elements_test() ->
    Base = #{damage => #{impact => 20, slash => 80}, critical_chance => 0,
             packet => #{scalar => 100, channels => [{impact, 0.2}, {slash, 0.8}],
                         quantization => quantized},
             critical_multiplier => 2, multishot => 1, status_chance => 0,
             magazine => 6, fire_rate => 5, reload_time => 2},
    Effects = [#{stat => base_damage, operation => add_base, value => 50},
               #{stat => {element, heat}, operation => add, value => 0.5},
               #{stat => damage, operation => stacking_multiply, value => 1},
               #{stat => magazine, operation => add_base, value => 6}],
    Result = wfcli_stat_math:direct_fire(Base, Effects, #{armor => 0}),
    ?assertEqual(#{impact => 60.0, slash => 240.0, heat => 150.0}, maps:get(damage_per_projectile, Result)),
    ?assertEqual(12, maps:get(magazine, Result)).

status_upgrades_precede_base_pellet_division_test() ->
    Base = #{damage => #{impact => 40}, critical_chance => 0,
             packet => #{scalar => 40, channels => [{impact, 1}], quantization => quantized},
             critical_multiplier => 2, multishot => 8, status_chance => 0.1125,
             magazine => 20, fire_rate => 5, reload_time => 2},
    Effects = [#{stat => status_chance, operation => add_base, value => 0.96},
               #{stat => status_chance, operation => stacking_multiply, value => 0.6}],
    Shot = wfcli_stat_math:direct_fire(Base, Effects, #{armor => 0}),
    near(0.372, maps:get(status_chance, Shot)),
    Many = wfcli_stat_math:direct_fire(Base,
        [#{stat => multishot, operation => stacking_multiply, value => 2} | Effects], #{armor => 0}),
    near(24, maps:get(multishot, Many)),
    ?assertEqual(maps:get(status_chance, Shot), maps:get(status_chance, Many)),
    Set = wfcli_stat_math:direct_fire(Base,
        [#{stat => status_chance, operation => set, value => 12}], #{armor => 0}),
    near(1.5, maps:get(status_chance, Set)),
    Clamped = wfcli_stat_math:direct_fire(Base,
        [#{stat => status_chance, operation => add, value => -1}], #{armor => 0}),
    ?assertEqual(0.0, maps:get(status_chance, Clamped)).

near(Expected, Actual) -> ?assert(abs(Expected - Actual) < 0.0001).
