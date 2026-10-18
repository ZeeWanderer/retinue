-module(retinue_poe2_service_eunit).
-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").
-import(retinue_poe2_fixture, [sample/1, with_snapshot/2, with_player/2, waiting/1, failed/1]).

session_test_() ->
    {foreach, fun setup/0, fun cleanup/1,
     [fun(Context) -> fun() -> Test(Context) end end || Test <-
         [fun projection_and_retry/1, fun changes/1, fun heartbeat/1, fun persistence/1,
          fun subscriptions/1, fun failed_persistence/1, fun unknown_is_not_zero/1,
          fun death_requires_continuity/1, fun history_bound/1, fun cache_validation/1,
          fun health_requires_current_producer/1]]}.

health_requires_current_producer(_) ->
    Producer = spawn(fun() -> receive stop -> ok end end),
    Monitor = monitor(process, Producer),
    Request = #{<<"op">> => <<"publish">>, <<"dataset">> => <<"session">>, <<"data">> => sample(1)},
    ?assertMatch({ok, _}, retinue_poe2_service:request(Request, #{connection => Producer, version => <<"test">>})),
    ?assertMatch(#{<<"receipt">> := #{<<"connected">> := true}}, retinue_poe2_service:health()),
    Producer ! stop,
    receive {'DOWN', Monitor, process, Producer, normal} -> ok after 1000 -> error(timeout) end,
    ?assertMatch(#{<<"receipt">> := #{<<"connected">> := false}}, retinue_poe2_service:health()),
    restart(),
    ?assertMatch(#{<<"receipt">> := Receipt} when map_size(Receipt) =:= 0, retinue_poe2_service:health()).

setup() ->
    Root = "/tmp/retinue-poe2-" ++ os:getpid() ++ "-" ++
           integer_to_list(erlang:unique_integer([positive])),
    Path = filename:join(Root, "session.json"),
    application:set_env(retinue_daemon, poe2_session_file, Path),
    {ok, _} = retinue_poe2_service:start_link(),
    Root.

cleanup(Root) ->
    gen_server:stop(retinue_poe2_service),
    application:unset_env(retinue_daemon, poe2_session_file),
    file:del_dir_r(Root).

publish(Data) -> retinue_poe2_service:request(
    #{<<"op">> => <<"publish">>, <<"dataset">> => <<"session">>, <<"data">> => Data}).

projection_and_retry(_) ->
    Sample = sample(1),
    ?assertMatch({ok, #{<<"revision">> := 1}}, publish(Sample)),
    Status = retinue_poe2_service:status(),
    ?assertEqual(maps:get(<<"snapshot">>, Sample), maps:get(<<"current">>, Status)),
    ?assertEqual(2, maps:get(<<"schema">>, Status)),
    ?assertEqual([<<"state">>, <<"area">>, <<"character">>, <<"xp">>, <<"level">>,
                  <<"gold">>, <<"panel">>], kinds(retinue_poe2_service:events())),
    lists:foreach(fun(Event) ->
        ?assertMatch(#{<<"stream">> := <<"fixture">>, <<"sequence">> := 1,
                       <<"observed_at">> := 1000,
                       <<"process">> := #{<<"pid">> := 42, <<"started">> := 1000}}, Event)
    end, retinue_poe2_service:events()),
    ?assertEqual({ok, Status}, retinue_poe2_service:request(
        #{<<"op">> => <<"get">>, <<"dataset">> => <<"session">>})),
    ?assertMatch({ok, #{<<"unchanged">> := true, <<"revision">> := 1}}, publish(Sample)),
    ?assertEqual(Status, retinue_poe2_service:status()),
    ?assertEqual({error, session_sequence_conflict},
                 publish(with_player(Sample, #{<<"gold">> => 43}))),
    ?assertEqual({error, session_sequence_conflict}, publish(Sample#{<<"observed_at">> => 1001})),
    ?assertEqual({error, stale_session_sample}, publish(sample(0))),
    ?assertEqual(Status, retinue_poe2_service:status()),
    ?assertMatch({ok, #{<<"unchanged">> := true}},
                 publish((sample(0))#{<<"stream">> => <<"restarted">>})),
    ?assertEqual([], at_sequence(0)).

changes(_) ->
    ?assertMatch({ok, _}, publish(sample(1))),
    Sample = changed(2),
    ?assertMatch({ok, #{<<"revision">> := 2}}, publish(Sample)),
    ?assertEqual([<<"state">>, <<"area">>, <<"xp">>, <<"level">>, <<"gold">>,
                  <<"panel">>, <<"death">>], kinds(at_sequence(2))),
    [Death] = [E || #{<<"kind">> := <<"death">>} = E <- at_sequence(2)],
    ?assertMatch(#{<<"from">> := 100, <<"to">> := 0, <<"character">> := <<"ExampleCharacter">>}, Death),
    ?assertMatch({ok, _}, publish(with_player(changed(3), #{<<"name">> => <<"OtherCharacter">>}))),
    ?assertEqual([<<"character">>, <<"xp">>, <<"level">>, <<"gold">>], kinds(at_sequence(3))).

changed(Sequence) ->
    with_snapshot(with_player(sample(Sequence), #{<<"experience">> => 400, <<"level">> => 3,
                                                   <<"gold">> => 43, <<"life">> => 0}),
                  #{<<"scene">> => <<"escape_menu">>, <<"states">> => [<<"InGameState">>, <<"EscapeState">>],
                    <<"area">> => #{<<"name">> => <<"Clearfell Encampment">>, <<"in_town">> => true,
                                    <<"change_key">> => 8},
                    <<"panels">> => #{<<"inventory">> => true, <<"stash">> => null}}).

heartbeat(Root) ->
    {ok, Ref, _} = retinue_poe2_service:subscribe(self()),
    ?assertMatch({ok, _}, publish(sample(1))),
    received(Ref, 1),
    Before = retinue_poe2_service:status(),
    Path = filename:join(Root, "session.json"),
    {ok, Bytes} = file:read_file(Path),
    ok = file:make_dir(Path ++ ".tmp"),
    ?assertMatch({ok, #{<<"revision">> := 1, <<"unchanged">> := true}}, publish(sample(2))),
    After = retinue_poe2_service:status(),
    ?assertEqual(Before#{<<"sequence">> => 2, <<"observed_at">> => 2000}, After),
    ?assertEqual({ok, Bytes}, file:read_file(Path)),
    no_event(Ref),
    ?assertMatch({ok, #{<<"unchanged">> := true}}, publish(sample(2))),
    no_event(Ref),
    {ok, Ref2, After} = retinue_poe2_service:subscribe(self()),
    ok = retinue_poe2_service:unsubscribe(Ref2).

persistence(Root) ->
    ?assertMatch({ok, _}, publish(sample(1))),
    ?assertMatch({ok, _}, publish(changed(2))),
    Status = retinue_poe2_service:status(),
    {ok, Info} = file:read_file_info(filename:join(Root, "session.json")),
    ?assertEqual(8#600, Info#file_info.mode band 8#777),
    restart(),
    ?assertEqual(Status, retinue_poe2_service:status()),
    ?assertMatch({ok, #{<<"unchanged">> := true}}, publish(changed(2))),
    ?assertMatch({ok, #{<<"unchanged">> := true}}, publish(changed(3))),
    restart(),
    ?assertEqual(Status, retinue_poe2_service:status()).

subscriptions(_) ->
    {ok, Ref, #{<<"revision">> := 0}} = retinue_poe2_service:subscribe(self()),
    ?assertMatch({ok, _}, publish(waiting(1))),
    received(Ref, 1),
    ?assertMatch({ok, _}, publish(failed(2))),
    received(Ref, 2),
    ?assertMatch(#{<<"status">> := <<"error">>, <<"current">> := null}, retinue_poe2_service:status()),
    ?assertMatch({ok, _}, publish(sample(3))),
    received(Ref, 3),
    ok = retinue_poe2_service:unsubscribe(Ref),
    ?assertMatch({ok, _}, publish(changed(4))),
    no_event(Ref).

failed_persistence(Root) ->
    {ok, Ref, _} = retinue_poe2_service:subscribe(self()),
    ?assertMatch({ok, _}, publish(sample(1))),
    received(Ref, 1),
    Before = retinue_poe2_service:status(),
    Temp = filename:join(Root, "session.json.tmp"),
    ok = file:make_dir(Temp),
    ?assertMatch({error, {session_persistence_failed, _}}, publish(changed(2))),
    ?assertEqual(Before, retinue_poe2_service:status()),
    no_event(Ref),
    ok = file:del_dir(Temp),
    ?assertMatch({ok, #{<<"revision">> := 2}}, publish(changed(2))),
    received(Ref, 2).

unknown_is_not_zero(_) ->
    ?assertMatch({ok, _}, publish(sample(1))),
    Partial = with_snapshot(with_player(sample(2), #{<<"gold">> => null, <<"life">> => null}),
        #{<<"area">> => null, <<"panels">> => #{<<"inventory">> => null},
          <<"issues">> => [#{<<"field">> => <<"life">>, <<"reason">> => <<"unreadable component">>}]}),
    ?assertMatch({ok, _}, publish(Partial)),
    ?assertEqual([], at_sequence(2)),
    ?assertEqual(maps:get(<<"snapshot">>, Partial), maps:get(<<"current">>, retinue_poe2_service:status())),
    ?assertMatch({ok, _}, publish(with_player(sample(3), #{<<"life">> => 0}))),
    ?assertEqual([], deaths()),
    ?assertMatch({ok, _}, publish(failed(4))),
    ?assertMatch(#{<<"current">> := null, <<"status">> := <<"error">>}, retinue_poe2_service:status()),
    ?assertEqual([], at_sequence(4)),
    ?assertMatch({ok, _}, publish(waiting(5))),
    ?assertMatch(#{<<"current">> := null, <<"process">> := null, <<"error">> := null},
                 retinue_poe2_service:status()).

death_requires_continuity(_) ->
    Cases = [
        fun(S) -> with_player(S, #{<<"name">> => <<"Other">>}) end,
        fun(S) -> with_player(S, #{<<"class">> => <<"Monk">>}) end |
        [fun(S) -> S#{<<"process">> => (maps:get(<<"process">>, S))#{Key => Value}} end
         || {Key, Value} <- [{<<"pid">>, 43}, {<<"started">>, 1001},
                             {<<"image_base">>, 16#150000000},
                             {<<"executable">>, <<"/other/PathOfExile_x64.exe">>}]]],
    Next = lists:foldl(fun(Change, N) ->
        ?assertMatch({ok, _}, publish(sample(N))),
        ?assertMatch({ok, _}, publish(Change(with_player(sample(N + 1), #{<<"life">> => 0})))),
        ?assertEqual([], deaths()),
        N + 2
    end, 1, Cases),
    lists:foldl(fun(Gap, N) ->
        ?assertMatch({ok, _}, publish(sample(N))),
        ?assertMatch({ok, _}, publish(Gap(N + 1))),
        ?assertMatch({ok, _}, publish(with_player(sample(N + 2), #{<<"life">> => 0}))),
        ?assertEqual([], deaths()),
        N + 3
    end, Next, [fun retinue_poe2_fixture:waiting/1, fun retinue_poe2_fixture:failed/1,
                 fun(N) -> with_snapshot(sample(N), #{<<"player">> => null}) end,
                 fun(N) -> with_player(sample(N), #{<<"life">> => null}) end]),
    ?assertEqual([], deaths()).

history_bound(_) ->
    lists:foreach(fun(N) ->
        ?assertMatch({ok, _}, publish(with_player(sample(N), #{<<"experience">> => N})))
    end, lists:seq(1, 140)),
    Events = retinue_poe2_service:events(),
    ?assertEqual(128, length(Events)),
    ?assertEqual(lists:seq(13, 140), [maps:get(<<"sequence">>, E) || E <- Events]),
    restart(),
    ?assertEqual(Events, retinue_poe2_service:events()).

cache_validation(Root) ->
    ?assertMatch({ok, _}, publish(sample(1))),
    Good = retinue_poe2_service:status(),
    [Event | _] = maps:get(<<"events">>, Good),
    Invalid = [#{<<"game">> => <<"poe2">>, <<"source">> => <<"/Client.txt">>,
                 <<"revision">> => 3, <<"current">> => #{}, <<"events">> => []},
               Good#{<<"schema">> => 1}, Good#{<<"revision">> => -1},
               Good#{<<"events">> => lists:duplicate(129, Event)},
               Good#{<<"events">> => [Event#{<<"kind">> => <<"untrusted">>}]},
               Good#{<<"current">> => #{}}, maps:remove(<<"observed_at">>, Good)],
    lists:foreach(fun(Cache) ->
        gen_server:stop(retinue_poe2_service),
        ok = file:write_file(filename:join(Root, "session.json"), json:encode(Cache)),
        {ok, _} = retinue_poe2_service:start_link(),
        ?assertEqual(retinue_poe2_session:empty(), retinue_poe2_service:status())
    end, Invalid),
    gen_server:stop(retinue_poe2_service),
    ok = file:write_file(filename:join(Root, "session.json"), binary:copy(<<"x">>, 4 * 1024 * 1024 + 1)),
    {ok, _} = retinue_poe2_service:start_link(),
    ?assertEqual(retinue_poe2_session:empty(), retinue_poe2_service:status()).

hot_update_drops_recoverable_log_state_test() ->
    Ref = make_ref(),
    State = #{snapshot => #{<<"source">> => <<"/Client.txt">>, <<"current">> => #{}},
              path => "/tmp/session.json", subscribers => #{Ref => self()}},
    {ok, Updated} = retinue_poe2_service:code_change(old, State, hot_update),
    ?assertEqual(State#{snapshot => retinue_poe2_session:empty()}, Updated),
    ?assertEqual({ok, Updated}, retinue_poe2_service:code_change(current, Updated, hot_update)).

sample_validation_test_() ->
    Sample = sample(1),
    Cases = [
        {[<<"stream">>], <<>>}, {[<<"stream">>], binary:copy(<<"x">>, 129)},
        {[<<"sequence">>], -1}, {[<<"sequence">>], 16#10000000000000000}, {[<<"sequence">>], 1.0},
        {[<<"observed_at">>], -1}, {[<<"observed_at">>], 16#10000000000000000},
        {[<<"status">>], <<"idle">>}, {[<<"status">>], <<"waiting">>},
        {[<<"status">>], <<"error">>}, {[<<"error">>], <<"unexpected">>},
        {[<<"process">>], null}, {[<<"snapshot">>], null},
        {[<<"process">>, <<"pid">>], -1}, {[<<"process">>, <<"pid">>], 16#100000000},
        {[<<"process">>, <<"started">>], -1}, {[<<"process">>, <<"image_base">>], 16#10000000000000000},
        {[<<"process">>, <<"executable">>], binary:copy(<<"x">>, 4097)},
        {[<<"snapshot">>, <<"scene">>], <<"orbiter">>},
        {[<<"snapshot">>, <<"states">>], lists:duplicate(17, <<"State">>)},
        {[<<"snapshot">>, <<"states">>], [binary:copy(<<"s">>, 65)]},
        {[<<"snapshot">>, <<"states">>], [<<255>>]},
        {[<<"snapshot">>, <<"player">>, <<"name">>], binary:copy(<<"x">>, 257)},
        {[<<"snapshot">>, <<"player">>, <<"class">>], binary:copy(<<"x">>, 129)},
        {[<<"snapshot">>, <<"player">>, <<"level">>], 0}, {[<<"snapshot">>, <<"player">>, <<"level">>], 101},
        {[<<"snapshot">>, <<"player">>, <<"experience">>], -1},
        {[<<"snapshot">>, <<"player">>, <<"experience">>], 16#100000000},
        {[<<"snapshot">>, <<"player">>, <<"experience">>], null},
        {[<<"snapshot">>, <<"player">>, <<"gold">>], -16#80000001},
        {[<<"snapshot">>, <<"player">>, <<"gold">>], 16#80000000},
        {[<<"snapshot">>, <<"player">>, <<"life">>], -1},
        {[<<"snapshot">>, <<"player">>, <<"life">>], 16#80000000},
        {[<<"snapshot">>, <<"area">>, <<"name">>], binary:copy(<<"x">>, 513)},
        {[<<"snapshot">>, <<"area">>, <<"in_town">>], null},
        {[<<"snapshot">>, <<"area">>, <<"change_key">>], 16#100000000},
        {[<<"snapshot">>, <<"panels">>], #{<<"unknown">> => true}},
        {[<<"snapshot">>, <<"panels">>], #{<<"inventory">> => 0}},
        {[<<"snapshot">>, <<"issues">>], lists:duplicate(17, #{<<"field">> => <<"x">>, <<"reason">> => <<"x">>})},
        {[<<"snapshot">>, <<"issues">>], [#{<<"field">> => binary:copy(<<"x">>, 65), <<"reason">> => <<"x">>}]},
        {[<<"snapshot">>, <<"issues">>], [#{<<"field">> => <<"x">>, <<"reason">> => binary:copy(<<"x">>, 2049)}]}],
    [?_assertEqual({error, invalid_session_sample}, retinue_poe2_session:sample(Data)) ||
        Data <- [#{}, [], maps:remove(<<"error">>, Sample), maps:remove(<<"snapshot">>, Sample),
                 (failed(1))#{<<"error">> => null}, (failed(1))#{<<"error">> => binary:copy(<<"x">>, 2049)} |
                 [put(Sample, Path, Value) || {Path, Value} <- Cases]]].

validation_bounds_and_sanitization_test() ->
    S = with_player(sample(1), #{<<"class">> => null, <<"level">> => 100,
                                 <<"experience">> => 16#ffffffff, <<"gold">> => -16#80000000,
                                 <<"life">> => 16#7fffffff}),
    ?assertMatch({ok, _}, retinue_poe2_session:sample(S#{<<"sequence">> => 16#ffffffffffffffff})),
    ?assertMatch({ok, _}, retinue_poe2_session:sample(waiting(1))),
    ?assertMatch({ok, _}, retinue_poe2_session:sample((failed(1))#{<<"process">> => null})),
    lists:foreach(fun(Scene) ->
        ?assertMatch({ok, _}, retinue_poe2_session:sample(with_snapshot(sample(1),
            #{<<"scene">> => Scene, <<"states">> => [], <<"player">> => null,
              <<"area">> => null, <<"panels">> => #{}})))
    end, [<<"unknown">>, <<"login">>, <<"select_character">>, <<"loading">>, <<"in_game">>, <<"escape_menu">>]),
    Clean = with_snapshot(sample(1), #{<<"issues">> => [#{<<"field">> => <<"stash">>, <<"reason">> => <<"unavailable">>}]}),
    Dirty = with_player(with_snapshot(Clean, #{<<"raw">> => <<"private">>,
        <<"issues">> => [#{<<"field">> => <<"stash">>, <<"reason">> => <<"unavailable">>,
                           <<"raw">> => <<"private">>}]}), #{<<"raw">> => <<"private">>}),
    ?assertEqual({ok, Clean}, retinue_poe2_session:sample(Dirty#{
        <<"source">> => <<"/Client.txt">>, <<"raw">> => <<"private">>,
        <<"process">> => (maps:get(<<"process">>, Dirty))#{<<"raw">> => <<"private">>}})).

put(_, [], Value) -> Value;
put(Map, [Key | Rest], Value) -> Map#{Key => put(maps:get(Key, Map), Rest, Value)}.
kinds(Events) -> [maps:get(<<"kind">>, Event) || Event <- Events].
at_sequence(N) -> [E || #{<<"sequence">> := Sequence} = E <- retinue_poe2_service:events(), Sequence =:= N].
deaths() -> [E || #{<<"kind">> := <<"death">>} = E <- retinue_poe2_service:events()].
restart() ->
    gen_server:stop(retinue_poe2_service),
    {ok, _} = retinue_poe2_service:start_link().
received(Ref, Revision) ->
    receive {retinue_poe2_session, Ref, #{<<"revision">> := Revision}} -> ok
    after 1000 -> error(no_session_event) end.
no_event(Ref) ->
    receive {retinue_poe2_session, Ref, _} -> error(unexpected_session_event) after 20 -> ok end.
