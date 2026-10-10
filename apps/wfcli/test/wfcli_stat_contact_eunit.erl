-module(wfcli_stat_contact_eunit).

-include_lib("eunit/include/eunit.hrl").

native_contact_test_() ->
    Fixture = fixture(),
    [{binary_to_list(maps:get(<<"name">>, Case)), ?_assertEqual(
        packet(maps:get(<<"expected">>, Case)), contact(Case, Fixture))}
     || Case <- maps:get(<<"contact_cases">>, Fixture)].

native_contact_critical_pipeline_test_() ->
    Fixture = fixture(),
    [{integer_to_list(Index), ?_test(begin
        Contact = contact(Case, Fixture),
        ?assertEqual(packet(maps:get(<<"expected">>, Case)), Contact),
        Factor = max(0.0, wfcli_stat_math:critical_factor(1,
            maps:get(critical_multiplier, Contact), maps:get(<<"tier">>, Case))),
        ?assertEqual(float32(maps:get(<<"critical_factor">>, Case)), Factor),
        Final = Contact#{scalar := wfcli_stat_math:scalar(
            maps:get(scalar, Contact), [{multiply, Factor}])},
        ?assertEqual(packet(maps:get(<<"final">>, Case)), Final)
    end)} || {Index, Case} <- lists:enumerate(maps:get(<<"combined_cases">>, Fixture))].

native_weakpoint_test_() ->
    {ok, Json} = file:read_file("apps/wfcli/test/fixtures/native_weakpoint_contact.json"),
    Fixture = jsone:decode(Json),
    Fields = [binary_to_existing_atom(K) || K <- maps:get(<<"expected_fields">>, Fixture)],
    [{integer_to_list(Index), ?_test(begin
        Result = weakpoint_contact(Case, Fixture),
        ?assertEqual([float32(V) || V <- maps:get(<<"expected">>, Case)],
                     [maps:get(K, Result) || K <- Fields]),
        ?assertEqual(maps:get(channels, packet(maps:get(<<"packet">>, Fixture))),
                     maps:get(channels, Result))
    end)} || {Index, Case} <- lists:enumerate(maps:get(<<"cases">>, Fixture))].

native_critical_context_test_() ->
    {ok, Json} = file:read_file("apps/wfcli/test/fixtures/native_critical_context.json"),
    Fixture = jsone:decode(Json),
    Fields = [binary_to_existing_atom(K) || K <- maps:get(<<"expected_fields">>, Fixture)],
    [{binary_to_list(maps:get(<<"name">>, Case)), ?_test(begin
        Result = critical_contact(Case, Fixture),
        ?assertEqual([float32(V) || V <- maps:get(<<"expected">>, Case)],
                     [maps:get(K, Result) || K <- Fields]),
        ?assertEqual(maps:get(channels, packet(maps:get(<<"packet">>, Fixture))),
                     maps:get(channels, Result))
    end)} || Case <- maps:get(<<"cases">>, Fixture)].

native_critical_links_test_() ->
    {ok, Json} = file:read_file("apps/wfcli/test/fixtures/native_critical_links.json"),
    Fixture = jsone:decode(Json),
    [{integer_to_list(Index), ?_assertEqual(float32(maps:get(<<"expected">>, Case)),
                                          linked_chance(Case))}
     || {Index, Case} <- lists:enumerate(maps:get(<<"cases">>, Fixture))].

linked_chance(Case) ->
    {module, wfcli_stat_math} = code:ensure_loaded(wfcli_stat_math),
    Packet = #{scalar => 1, critical_chance => maps:get(<<"base">>, Case), critical_multiplier => 2,
               status_chance => 0, channels => [], pending_updates => #{
                   critical_chance => operations(maps:get(<<"pending">>, Case))}},
    Link = case maps:get(<<"linked">>, Case) of
        null -> none;
        #{<<"base">> := Base, <<"value">> := Value} -> #{base => Base, value => Value}
    end,
    Status = case maps:get(<<"per_status">>, Case) of
        null -> none;
        Statuses -> maps:from_list([{binary_to_integer(Id),
            #{count => maps:get(<<"count">>, S), entries => operations(maps:get(<<"entries">>, S))}}
            || {Id, S} <- maps:to_list(Statuses)])
    end,
    Critical = #{chance => [entry_bucket(maps:get(<<"before_link">>, Case))], multiplier => [],
                 chance_limit => none, low_chance => none, linked_chance => Link,
                 weapon_chance => entry_bucket(maps:get(<<"weapon">>, Case)),
                 per_status_chance => Status, stealth_chance => entry_bucket(maps:get(<<"stealth">>, Case))},
    maps:get(critical_chance, wfcli_stat_math:contact_packet(Packet, none, critical_context(Critical))).

operations(Entries) -> [{binary_to_existing_atom(Op), V} || [Op, V] <- Entries].

entry_bucket(null) -> none;
entry_bucket(Entries) -> #{entries => operations(Entries)}.

critical_context(Critical) ->
    #{head => false, suppressed => false, head_factor => 1, deferred_scalar => true, critical => Critical}.

critical_status_counts_stacks_before_cap_test() ->
    Packet = #{scalar => 1, critical_chance => 0.25, critical_multiplier => 2,
               status_chance => 0, channels => [], pending_updates => #{
                   critical_chance => [{add_base, 0.25}, {add, 1}, {multiply, 2}]}},
    Critical = #{chance => [], multiplier => [], chance_limit => none, low_chance => none,
                 per_status_chance => #{2 => #{count => 3, entries => [{stacking_multiply, 1}]}}},
    Result = wfcli_stat_math:contact_packet(Packet, none, critical_context(Critical)),
    ?assertEqual(6.0, maps:get(critical_chance, Result)),
    Capped = Critical#{chance_limit := 5, low_chance := #{threshold => 5,
                       multiplier => #{entries => [{add, 1}]}}},
    Limited = wfcli_stat_math:contact_packet(Packet, none, critical_context(Capped)),
    ?assertEqual(5.0, maps:get(critical_chance, Limited)),
    ?assertEqual(3.0, maps:get(critical_multiplier, Limited)).

critical_status_rejects_invalid_ids_and_counts_test() ->
    Packet = #{scalar => 1, critical_chance => 0.25, critical_multiplier => 2,
               status_chance => 0, channels => []},
    Critical = #{chance => [], multiplier => [], chance_limit => none, low_chance => none},
    [?assertError(function_clause, wfcli_stat_math:contact_packet(Packet, none,
        critical_context(Critical#{per_status_chance => #{Id => #{count => Count, entries => []}}})))
     || {Id, Count} <- [{-1, 1}, {30, 1}, {0.0, 1}, {0, -1}, {0, 1.5}, {0, 1 bsl 64}]].

native_post_critical_test_() ->
    {ok, Json} = file:read_file("apps/wfcli/test/fixtures/native_post_critical.json"),
    Fixture = jsone:decode(Json),
    [{binary_to_list(maps:get(<<"name">>, Case)), ?_test(begin
        Hit = post_critical_fields(maps:get(<<"hit">>, Case)),
        Effects = post_critical_fields(maps:get(<<"effects">>, Case)),
        Expected = post_critical_fields(maps:get(<<"expected">>, Case)),
        {Result, Tail} = wfcli_stat_math:critical_effects(Hit, Effects, [maps:get(<<"roll">>, Case)]),
        ?assertEqual(maps:remove(roll_consumed, Expected), maps:with(maps:keys(Expected), Result)),
        ?assertEqual(maps:get(roll_consumed, Expected), Tail =:= [])
    end)} || Case <- maps:get(<<"cases">>, Fixture)].

post_critical_preserves_other_procs_and_counts_types_test() ->
    Hit = #{factor => 2.0, tier => 1, has_weapon => true, self_hit => false,
            forced_procs => #{2 => 254, 7 => 6}, active_status_types => [0, 0, 13, 29]},
    Effects = #{slash_chance => 1.0, low_status_bonus => 0.5, low_status_limit => 2},
    {Result, []} = wfcli_stat_math:critical_effects(Hit, Effects, [16777215]),
    ?assertEqual(#{2 => 255, 7 => 6}, maps:get(forced_procs, Result)),
    ?assertEqual(2.5, maps:get(factor, Result)),
    {Saturated, []} = wfcli_stat_math:critical_effects(Result, Effects, [0]),
    ?assertEqual(#{2 => 255, 7 => 6}, maps:get(forced_procs, Saturated)),
    ?assertEqual(1, maps:get(tier, Saturated)).

post_critical_never_reapplies_critical_multiplier_test() ->
    Hit = #{factor => 3.0, tier => 2, has_weapon => true, self_hit => false,
            forced_procs => #{}, active_status_types => []},
    Effects = #{slash_chance => 0.0, low_status_bonus => 0.5, low_status_limit => 3},
    {Result, [0]} = wfcli_stat_math:critical_effects(Hit, Effects, [0]),
    ?assertEqual(3.5, maps:get(factor, Result)),
    ?assertEqual(#{}, maps:get(forced_procs, Result)).

post_critical_roll_range_test() ->
    Hit = #{factor => 1, tier => 0, has_weapon => true, self_hit => false, hit_type => 0, proc_type => 30},
    Effects = #{noncritical_chance => 1, noncritical_bonus => 0},
    [?assertError({invalid_critical_roll, Roll}, wfcli_stat_math:critical_effects(Hit, Effects, [Roll]))
     || Roll <- [-1, 16777216, 0.5]].

post_critical_requires_resolved_integer_limit_test() ->
    Hit = #{factor => 1, tier => 1, has_weapon => true, self_hit => false,
            forced_procs => #{}, active_status_types => [0, 1]},
    Effects = #{slash_chance => 0, low_status_bonus => 0.5, low_status_limit => 2.9},
    ?assertError(function_clause, wfcli_stat_math:critical_effects(Hit, Effects, [0])).

post_critical_fields(Map) ->
    {module, wfcli_stat_math} = code:ensure_loaded(wfcli_stat_math),
    maps:from_list([case binary_to_existing_atom(Key) of
        forced_procs -> {forced_procs, maps:from_list(
            [{binary_to_integer(K), V} || {K, V} <- maps:to_list(Value)])};
        factor -> {factor, float32(Value)};
        Name -> {Name, Value}
    end || {Key, Value} <- maps:to_list(Map)]).

critical_context_preserves_pending_contact_modifiers_test() ->
    Packet = #{scalar => 29, critical_chance => 0.25, critical_multiplier => 2,
               status_chance => 0.5, channels => [], pending_updates => #{
                   critical_chance => [{add, 0.25}, {multiply, 2}],
                   critical_multiplier => [{add, 1}, {multiply, 2}]}},
    Proxy = #{enabled => true, treat_head_as_normal => false, damage_multiplier => 1,
              hit_type_multiplier => 1, critical_chance_add => 0.25, critical_multiplier => 2,
              status_multiplier => 1, channel_add => #{}, channel_multiply => #{}},
    Context = #{head => false, suppressed => false, head_factor => 1.5, deferred_scalar => true},
    Critical = #{chance => [#{entries => [{add_base, 0.25}]}],
                 multiplier => [#{entries => [{add_base, 1}]}], chance_limit => none, low_chance => none},
    Result = wfcli_stat_math:contact_packet(Packet, Proxy, Context#{critical => Critical}),
    ?assertEqual(2.0, maps:get(critical_chance, Result)),
    ?assertEqual(16.0, maps:get(critical_multiplier, Result)),
    Materialized = wfcli_stat_math:contact_packet(Packet, Proxy, Context),
    Separate = wfcli_stat_math:contact_packet(Materialized, none, Context#{critical => Critical}),
    ?assertEqual(1.75, maps:get(critical_chance, Separate)),
    ?assertEqual(13.0, maps:get(critical_multiplier, Separate)).

critical_chance_limit_precedes_inclusive_damage_threshold_test() ->
    Packet = #{scalar => 29, critical_chance => 1.5, critical_multiplier => 2,
               status_chance => 0.5, channels => []},
    Critical = #{chance => [], multiplier => [], chance_limit => 1,
                 low_chance => #{threshold => 1, multiplier => #{entries => [{multiply, 2}]}}},
    Context = #{head => false, suppressed => false, head_factor => 1.5,
                deferred_scalar => true, critical => Critical},
    Result = wfcli_stat_math:contact_packet(Packet, none, Context),
    ?assertEqual(1.0, maps:get(critical_chance, Result)),
    ?assertEqual(4.0, maps:get(critical_multiplier, Result)),
    Uncapped = wfcli_stat_math:contact_packet(Packet, none,
        Context#{critical := Critical#{chance_limit := none}}),
    ?assertEqual(2.0, maps:get(critical_multiplier, Uncapped)).

weakpoint_is_added_to_existing_multiplier_test() ->
    Packet = packet(maps:get(<<"settled">>, maps:get(<<"packets">>, fixture()))),
    Proxy = #{enabled => true, treat_head_as_normal => false, damage_multiplier => 2,
              hit_type_multiplier => 1, critical_chance_add => 0, critical_multiplier => 1,
              status_multiplier => 1, channel_add => #{}, channel_multiply => #{}},
    Context = #{head => false, suppressed => false, head_factor => 1.5, deferred_scalar => true,
                weakpoint => #{target => none, weapon => #{entries => [{add, 0.5}]}}},
    ?assertEqual(72.5, maps:get(scalar, wfcli_stat_math:contact_packet(Packet, Proxy, Context))),
    ?assertEqual(29.0, maps:get(scalar, wfcli_stat_math:contact_packet(Packet, none, Context))),
    ?assertEqual(58.0, maps:get(scalar, wfcli_stat_math:contact_packet(
        Packet, Proxy, Context#{weakpoint := none}))).

no_proxy_has_no_head_bonus_test() ->
    Packet = packet(maps:get(<<"pending">>, maps:get(<<"packets">>, fixture()))),
    #{pending_updates := Pending} = Packet,
    Context = #{head => true, suppressed => false, head_factor => 100, deferred_scalar => true},
    Result = wfcli_stat_math:contact_packet(Packet#{quantization => unquantized}, none, Context),
    ?assertEqual(unquantized, maps:get(quantization, Result)),
    ?assertNot(maps:is_key(pending_updates, Result)),
    [?assertEqual(wfcli_stat_math:scalar_updates(maps:get(K, Packet), Updates), maps:get(K, Result))
     || {K, Updates} <- maps:to_list(Pending)],
    ?assertEqual(maps:get(channels, Packet), maps:get(channels, Result)).

suppression_preserves_channel_edits_test() ->
    Fixture = fixture(),
    [Case] = [C || C <- maps:get(<<"contact_cases">>, Fixture),
                  maps:get(<<"name">>, C) =:= <<"radial_head:pending:mixed:False">>],
    Result = contact(Case, Fixture),
    Packet = packet(maps:get(<<"pending">>, maps:get(<<"packets">>, Fixture))),
    ?assertNotEqual(maps:get(channels, Packet), maps:get(channels, Result)),
    ?assertEqual(maps:get(status_chance, Packet), maps:get(status_chance, Result)),
    ?assertEqual(wfcli_stat_math:scalar_updates(maps:get(scalar, Packet),
        maps:get(scalar, maps:get(pending_updates, Packet))), maps:get(scalar, Result)).

fixture() ->
    {ok, Json} = file:read_file("apps/wfcli/test/fixtures/native_contact_packet.json"),
    jsone:decode(Json).

contact(Case, Fixture) ->
    Packet = packet(maps:get(maps:get(<<"packet">>, Case), maps:get(<<"packets">>, Fixture))),
    P = maps:get(maps:get(<<"proxy">>, Case), maps:get(<<"proxies">>, Fixture)),
    Proxy = #{enabled => maps:get(<<"enabled">>, Case),
              treat_head_as_normal => maps:get(<<"treat_head_as_normal">>, Case),
              damage_multiplier => maps:get(<<"damage_multiplier">>, P),
              hit_type_multiplier => maps:get(<<"hit_type_multiplier">>, P),
              critical_chance_add => maps:get(<<"critical_chance_add">>, P),
              critical_multiplier => maps:get(<<"critical_multiplier">>, P),
              status_multiplier => maps:get(<<"status_multiplier">>, P),
              channel_add => channels(maps:get(<<"channel_add">>, P)),
              channel_multiply => channels(maps:get(<<"channel_multiply">>, P))},
    C = maps:get(<<"context">>, Case),
    Context = #{head => maps:get(<<"head">>, C), suppressed => maps:get(<<"suppressed">>, C),
                head_factor => maps:get(<<"head_factor">>, C),
                deferred_scalar => maps:get(<<"deferred_scalar">>, C)},
    wfcli_stat_math:contact_packet(Packet, Proxy, Context).

packet(P) ->
    {module, wfcli_stat_math} = code:ensure_loaded(wfcli_stat_math),
    Keys = [scalar, critical_chance, critical_multiplier, status_chance],
    Values = maps:from_list([{K, float32(maps:get(atom_to_binary(K), P))} || K <- Keys]),
    Packet = Values#{channels => [{binary_to_existing_atom(C), float32(V)}
                                  || [C, V] <- maps:get(<<"channels">>, P)]},
    case maps:find(<<"pending_updates">>, P) of
        error -> Packet;
        {ok, Pending} -> Packet#{pending_updates => maps:from_list([
            {binary_to_existing_atom(K), [{binary_to_existing_atom(Op), V} || [Op, V] <- Updates]}
            || {K, Updates} <- maps:to_list(Pending)])}
    end.

channels(Map) -> maps:from_list([{binary_to_existing_atom(C), V} || {C, V} <- maps:to_list(Map)]).

weakpoint_contact(Case, Fixture) ->
    Packet = pending_packet(maps:get(<<"pending">>, Case), Fixture),
    Geometry = maps:get(maps:get(<<"context">>, Case), maps:get(<<"contexts">>, Fixture)),
    P = maps:get(<<"proxy">>, Fixture),
    Proxy = maps:merge(#{enabled => maps:get(<<"enabled">>, Geometry),
                        treat_head_as_normal => maps:get(<<"treat_head_as_normal">>, Geometry),
                        channel_add => #{}, channel_multiply => #{}},
        maps:from_list([{K, maps:get(atom_to_binary(K), P)} || K <- [damage_multiplier,
            hit_type_multiplier, critical_chance_add, critical_multiplier, status_multiplier]])),
    W = maps:get(maps:get(<<"profile">>, Case), maps:get(<<"profiles">>, Fixture)),
    Context = #{head => maps:get(<<"head">>, Geometry),
                suppressed => maps:get(<<"suppressed">>, Geometry),
                head_factor => maps:get(<<"head_factor">>, Fixture),
                deferred_scalar => maps:get(<<"deferred_scalar">>, Case),
                weakpoint => #{target => bucket(maps:get(<<"target">>, W)),
                               weapon => bucket(maps:get(<<"weapon">>, W))}},
    wfcli_stat_math:contact_packet(Packet, Proxy, Context).

critical_contact(Case, Fixture) ->
    Packet = pending_packet(maps:get(<<"pending">>, Case), Fixture),
    P = maps:get(<<"proxy">>, Fixture),
    Proxy = maps:merge(#{enabled => true, treat_head_as_normal => false,
                        channel_add => #{}, channel_multiply => #{}},
        maps:from_list([{K, maps:get(atom_to_binary(K), P)} || K <- [damage_multiplier,
            hit_type_multiplier, critical_chance_add, critical_multiplier, status_multiplier]])),
    Profile = maps:get(maps:get(<<"profile">>, Case), maps:get(<<"profiles">>, Fixture)),
    LowChance = case maps:get(<<"threshold">>, Case) of
        null -> none;
        Threshold -> #{threshold => Threshold, multiplier => bucket(maps:get(<<"low_multiplier">>, Fixture))}
    end,
    Critical = #{chance => [bucket(B) || B <- maps:get(<<"chance">>, Profile)],
                 multiplier => [bucket(B) || B <- maps:get(<<"multiplier">>, Profile)],
                 chance_limit => optional_number(maps:get(<<"cap">>, Case)), low_chance => LowChance},
    Context = #{head => maps:get(<<"contact">>, Case) =/= <<"body">>,
                suppressed => maps:get(<<"contact">>, Case) =:= <<"radial_head">>,
                head_factor => maps:get(<<"head_factor">>, Fixture),
                deferred_scalar => maps:get(<<"deferred_scalar">>, Case), critical => Critical},
    wfcli_stat_math:contact_packet(Packet, Proxy, Context).

pending_packet(Pending, Fixture) ->
    Base = packet(maps:get(<<"packet">>, Fixture)),
    case Pending of
        true ->
            Updates = entries(maps:get(<<"updates">>, Fixture)),
            Base#{pending_updates => maps:from_list(
                [{K, Updates} || K <- [scalar, critical_chance, critical_multiplier]])};
        false -> Base
    end.

optional_number(null) -> none;
optional_number(Number) -> Number.

bucket(null) -> none;
bucket(Map) ->
    Bucket = #{entries => entries(maps:get(<<"entries">>, Map))},
    case maps:find(<<"scale">>, Map) of
        {ok, Scale} -> Bucket#{scale => Scale};
        error -> Bucket
    end.

entries(Entries) -> [{binary_to_existing_atom(Op), V} || [Op, V] <- Entries].

float32(Value) ->
    <<Result:32/float>> = <<Value:32/float>>,
    Result.
