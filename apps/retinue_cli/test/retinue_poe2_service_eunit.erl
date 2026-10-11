-module(retinue_poe2_service_eunit).
-include_lib("eunit/include/eunit.hrl").

session_test_() ->
    {foreach, fun setup/0, fun cleanup/1,
     [fun(Context) -> fun() -> projection_and_replay(Context) end end,
      fun(Context) -> fun() -> persistence(Context) end end,
      fun(Context) -> fun() -> subscriptions(Context) end end,
      fun(Context) -> fun() -> validation(Context) end end,
      fun(Context) -> fun() -> empty_cursor_is_not_fresh_evidence(Context) end end,
      fun(Context) -> fun() -> session_reset(Context) end end]}.

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

event(Offset, Fields) ->
    maps:merge(#{<<"offset">> => Offset, <<"timestamp_local">> => <<"2026/10/11 12:00:00">>,
                 <<"client_pid">> => 312, <<"tick_ms">> => Offset}, Fields).
area(Offset) -> event(Offset, #{<<"kind">> => <<"area_generated">>,
    <<"area_id">> => <<"G1_1">>, <<"area_level">> => 1, <<"seed">> => 7}).
death(Offset) -> event(Offset, #{<<"kind">> => <<"death">>, <<"character">> => <<"Example">>}).
batch(Cursor, Events) -> #{<<"source">> => <<"/game/logs/Client.txt">>,
    <<"generation">> => <<"1:42:0">>, <<"cursor">> => Cursor, <<"events">> => Events}.
publish(Data) -> retinue_poe2_service:request(
    #{<<"op">> => <<"publish">>, <<"dataset">> => <<"session">>, <<"data">> => Data}).

projection_and_replay(_) ->
    Events = [area(100), event(200, #{<<"kind">> => <<"area_name">>, <<"area_name">> => <<"The Riverbank">>}),
              death(300)],
    ?assertMatch({ok, #{<<"revision">> := 1}}, publish(batch(300, Events))),
    ?assertMatch({ok, #{<<"unchanged">> := true}}, publish(batch(300, Events))),
    #{<<"current">> := Current} = retinue_poe2_service:status(),
    ?assertEqual(<<"The Riverbank">>, maps:get(<<"area_name">>, Current)),
    ?assertNot(maps:is_key(<<"character">>, Current)),
    ?assertEqual(#{<<"Example">> => #{<<"observed_deaths">> => 1}}, maps:get(<<"actors">>, Current)),
    ?assertMatch({ok, _}, publish(batch(400, [death(300), death(400)]))),
    ?assertEqual(4, length(retinue_poe2_service:events())).

persistence(_) ->
    ?assertMatch({ok, _}, publish(batch(100, [area(100)]))),
    Snapshot = retinue_poe2_service:status(),
    gen_server:stop(retinue_poe2_service),
    {ok, _} = retinue_poe2_service:start_link(),
    ?assertEqual(Snapshot, retinue_poe2_service:status()),
    ?assertMatch({ok, #{<<"unchanged">> := true}}, publish(batch(100, [area(100)]))).

subscriptions(_) ->
    {ok, Ref, #{<<"revision">> := 0}} = retinue_poe2_service:subscribe(self()),
    ?assertMatch({ok, _}, publish(batch(100, [area(100)]))),
    receive {retinue_poe2_session, Ref, #{<<"revision">> := 1}} -> ok
    after 1000 -> error(no_session_event) end,
    ok = retinue_poe2_service:unsubscribe(Ref),
    ?assertMatch({ok, _}, publish(batch(200, [death(200)]))),
    receive {retinue_poe2_session, Ref, _} -> error(unsubscribed_event) after 20 -> ok end.

validation(_) ->
    Invalid = [batch(50, [area(100)]), batch(200, [area(200), area(100)]),
               batch(100, [event(100, #{<<"kind">> => <<"chat">>})]),
               batch(100, lists:duplicate(129, area(100))), #{}],
    lists:foreach(fun(Batch) -> ?assertEqual({error, invalid_session_batch}, publish(Batch)) end, Invalid),
    ?assertMatch({ok, _}, publish(batch(100, [(area(100))#{<<"raw_chat">> => <<"private">>}]))),
    [Stored] = retinue_poe2_service:events(),
    ?assertNot(maps:is_key(<<"raw_chat">>, Stored)).

session_reset(_) ->
    ?assertMatch({ok, _}, publish(batch(100, [area(100)]))),
    New = (death(200))#{<<"client_pid">> => 400},
    ?assertMatch({ok, _}, publish(batch(200, [New]))),
    #{<<"current">> := Current} = retinue_poe2_service:status(),
    ?assertNot(maps:is_key(<<"area_id">>, Current)),
    ?assertMatch({ok, _}, publish((batch(10, [area(10)]))#{<<"generation">> => <<"new">>})),
    ?assertEqual(1, length(retinue_poe2_service:events())).

empty_cursor_is_not_fresh_evidence(_) ->
    ?assertMatch({ok, _}, publish(batch(100, []))),
    ?assertEqual(null, maps:get(<<"updated_at">>, retinue_poe2_service:status())),
    ?assertMatch({ok, _}, publish(batch(200, [area(200)]))),
    Before = retinue_poe2_service:status(),
    timer:sleep(2),
    ?assertMatch({ok, _}, publish(batch(300, []))),
    After = retinue_poe2_service:status(),
    ?assertEqual(maps:get(<<"updated_at">>, Before), maps:get(<<"updated_at">>, After)),
    ?assertEqual(maps:get(<<"current">>, Before), maps:get(<<"current">>, After)).
