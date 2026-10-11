-module(wfcli_player_inventory_eunit).

-include_lib("eunit/include/eunit.hrl").

-export([full/2, native/3]).

full(Sequence, Count) ->
    #{<<"schema">> => 2, <<"sync">> => #{<<"$oid">> => <<"abc">>},
      <<"collector">> => <<"native_http_buffer">>, <<"process_pid">> => 42,
      <<"collected_at">> => 1000, <<"observation">> => stamp(Sequence, Sequence, 100, 100),
      <<"raw">> => #{<<"LastInventorySync">> => #{<<"$oid">> => <<"abc">>},
                    <<"MiscItems">> => [#{<<"ItemType">> => <<"resource">>, <<"ItemCount">> => Count,
                                         <<"future">> => true}],
                    <<"Recipes">> => [#{<<"ItemType">> => <<"recipe">>, <<"ItemCount">> => 1}],
                    <<"PendingRecipes">> => [#{<<"ItemId">> => #{<<"$oid">> => <<"job">>},
                                              <<"ItemType">> => <<"recipe">>, <<"future">> => 1}],
                    <<"Suits">> => [#{<<"XP">> => 123}], <<"unknown">> => 42}}.

native(Sequence, Baseline, Count) ->
    #{<<"schema">> => 1, <<"sync">> => <<"abc">>, <<"collector">> => <<"native_inventory">>,
      <<"process_pid">> => 42, <<"collected_at">> => 2000,
      <<"observation">> => stamp(Sequence, Baseline, 101, 102),
      <<"fields">> => #{<<"MiscItems">> => [#{<<"ItemType">> => <<"resource">>, <<"ItemCount">> => Count}],
                        <<"Recipes">> => [#{<<"ItemType">> => <<"recipe">>, <<"ItemCount">> => 1}],
                        <<"PendingRecipes">> => []}}.

stamp(Sequence, Baseline, Start, Finish) ->
    #{<<"boot_id">> => wfcli_player_inventory:boot_id(), <<"stream">> => 100,
      <<"game_started">> => 99, <<"generation">> => 1,
      <<"sequence">> => Sequence, <<"baseline">> => Baseline,
      <<"started_ns">> => Start, <<"finished_ns">> => Finish}.

observe(Source, Value, Sources) ->
    {ok, Next} = wfcli_player_inventory:apply(Source, Value, Sources, wfcli_player_inventory:boot_id()),
    Next.

alter_stamp(Value, Changes) ->
    Value#{<<"observation">> => maps:merge(maps:get(<<"observation">>, Value), Changes)}.

count(Sources) ->
    [Row | _] = maps:get(<<"MiscItems">>, maps:get(<<"raw">>, maps:get(<<"inventory">>, Sources))),
    maps:get(<<"ItemCount">>, Row).

absolute_replacement_preserves_unknown_fields_test() ->
    Full = full(1, 12),
    Base = observe(<<"inventory_http">>, Full, #{}),
    Claimed = observe(<<"inventory_native">>, native(2, 1, 17), Base),
    ?assertEqual(17, count(Claimed)),
    ?assertEqual(Claimed, observe(<<"inventory_native">>, native(2, 1, 17), Claimed)),
    Again = observe(<<"inventory_native">>, native(3, 1, 32), Claimed),
    ?assertEqual(32, count(Again)),
    Raw = maps:get(<<"raw">>, maps:get(<<"inventory">>, Again)),
    ?assertEqual(true, maps:get(<<"future">>, hd(maps:get(<<"MiscItems">>, Raw)))),
    ?assertEqual([#{<<"XP">> => 123}], maps:get(<<"Suits">>, Raw)),
    ?assertEqual(42, maps:get(<<"unknown">>, Raw)),
    ?assertEqual([], maps:get(<<"PendingRecipes">>, Raw)),
    ?assertEqual(Full, maps:get(<<"inventory_http">>, Again)).

late_scopes_cannot_overwrite_baseline_test() ->
    Base = observe(<<"inventory_http">>, full(3, 22), #{}),
    Good = native(4, 3, 17),
    Wrong = [native(2, 1, 17), alter_stamp(Good, #{<<"started_ns">> => 99}),
             alter_stamp(Good, #{<<"stream">> => 99}),
             alter_stamp(Good, #{<<"generation">> => 2}),
             alter_stamp(Good, #{<<"game_started">> => 100}),
             Good#{<<"sync">> => <<"different-login">>}, Good#{<<"process_pid">> => 43}],
    lists:foreach(fun(Value) -> ?assertEqual(Base, observe(<<"inventory_native">>, Value, Base)) end, Wrong),
    ?assertEqual(#{}, observe(<<"inventory_native">>, Good, #{})),
    ?assertEqual(17, count(observe(<<"inventory_native">>, Good, Base))).

replay_and_new_session_ordering_test() ->
    Base = observe(<<"inventory_http">>, full(1, 12), #{}),
    Native = observe(<<"inventory_native">>, native(2, 1, 17), Base),
    ?assertEqual(Native, observe(<<"inventory_http">>, full(1, 12), Native)),
    NewBase = observe(<<"inventory_http">>, full(3, 22), Native),
    ?assertNot(maps:is_key(<<"inventory_native">>, NewBase)),
    ?assertEqual(NewBase, observe(<<"inventory_native">>, native(2, 1, 17), NewBase)),
    NewSession = alter_stamp(full(1, 44), #{<<"stream">> => 101, <<"game_started">> => 105}),
    Latest = observe(<<"inventory_http">>, NewSession, NewBase),
    ?assertEqual(44, count(Latest)),
    ?assertEqual(Latest, observe(<<"inventory_http">>, full(1000, 12), Latest)),
    OldGame = alter_stamp(full(1, 12), #{<<"stream">> => 999}),
    ?assertEqual(Latest, observe(<<"inventory_http">>, OldGame, Latest)),
    OtherBoot = alter_stamp(full(1001, 12), #{<<"boot_id">> => <<"00000000-0000-0000-0000-000000000000">>}),
    ?assertMatch({error, _}, wfcli_player_inventory:apply(<<"inventory_http">>, OtherBoot, Latest,
                                                       wfcli_player_inventory:boot_id())).

row_order_and_known_field_removal_test() ->
    Full = full(1, 12),
    Raw = maps:get(<<"raw">>, Full),
    Rows = [#{<<"ItemType">> => <<"b">>, <<"ItemCount">> => 1},
            #{<<"ItemType">> => <<"a">>, <<"ItemCount">> => 2, <<"unknown">> => true}],
    [Job] = maps:get(<<"PendingRecipes">>, Raw),
    Base = observe(<<"inventory_http">>, Full#{<<"raw">> => Raw#{
        <<"MiscItems">> => Rows, <<"PendingRecipes">> => [Job#{<<"TargetFingerprint">> => <<"old">>}]}}, #{}),
    Native = native(2, 1, 12),
    Fields = (maps:get(<<"fields">>, Native))#{<<"MiscItems">> => lists:reverse(Rows),
                                             <<"PendingRecipes">> => [maps:remove(<<"future">>, Job)]},
    Updated = observe(<<"inventory_native">>, Native#{<<"fields">> => Fields}, Base),
    Effective = maps:get(<<"raw">>, maps:get(<<"inventory">>, Updated)),
    ?assertEqual(Rows, maps:get(<<"MiscItems">>, Effective)),
    ?assertEqual([Job], maps:get(<<"PendingRecipes">>, Effective)).

scope_validation_test() ->
    Base = observe(<<"inventory_http">>, full(1, 12), #{}),
    Native = native(2, 1, 12),
    Fields = maps:get(<<"fields">>, Native),
    Bad = [maps:remove(<<"Recipes">>, Fields), Fields#{<<"Suits">> => []},
           Fields#{<<"MiscItems">> => null},
           Fields#{<<"MiscItems">> => [#{<<"ItemType">> => <<"x">>, <<"ItemCount">> => -1}]},
           Fields#{<<"PendingRecipes">> => [#{<<"ItemType">> => <<"recipe">>}]},
           Fields#{<<"Recipes">> => lists:duplicate(2, #{<<"ItemType">> => <<"recipe">>, <<"ItemCount">> => 1})}],
    lists:foreach(fun(Value) ->
        ?assertMatch({error, _}, wfcli_player_inventory:apply(<<"inventory_native">>,
                              Native#{<<"fields">> => Value}, Base, wfcli_player_inventory:boot_id()))
    end, Bad),
    Empty = Native#{<<"fields">> => #{<<"MiscItems">> => [], <<"Recipes">> => [], <<"PendingRecipes">> => []}},
    Removed = observe(<<"inventory_native">>, Empty, Base),
    ?assertEqual([], maps:get(<<"MiscItems">>, maps:get(<<"raw">>, maps:get(<<"inventory">>, Removed)))).
