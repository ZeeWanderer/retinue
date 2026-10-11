-module(wfcli_diagnostics_query_eunit).

-include_lib("eunit/include/eunit.hrl").

filters_and_sorts_resolution_issues_test() ->
    Issues = [issue(<<"asset">>, <<"/Lotus/B">>, 2),
              issue(<<"asset">>, <<"/Lotus/A">>, 7),
              issue(<<"friendly_name">>, <<"/Lotus/C">>, 9)],
    {ok, Ast} = wfcli_query_parse:parse("kind=asset sort=-count"),
    {ok, #{results := #{slice := Rows, total := 2}}} =
        wfcli_diagnostics_query:execute(Ast, #{}, Issues),
    ?assertEqual([<<"/Lotus/A">>, <<"/Lotus/B">>],
                 [maps:get(<<"identity">>, maps:get(data, Row)) || Row <- Rows]).

rejects_invalid_sort_operator_test() ->
    {ok, Ast} = wfcli_query_parse:parse("sort>count"),
    ?assertMatch({error, {query_errors, [_]}},
                 wfcli_diagnostics_query:execute(Ast, #{}, [])).

incident_time_comparisons_and_offset_sorting_test() ->
    Rows = [incident(<<"late">>, <<"2026-09-30T10:00:00Z">>),
            incident(<<"early">>, <<"2026-09-30T12:00:00+03:00">>),
            incident(<<"missing">>, null)],
    {ok, Ast} = wfcli_query_parse:parse("timestamp>=2026-09-30T09:00:00Z sort=timestamp"),
    {ok, #{results := #{slice := Entries, kind := incidents}}} =
        wfcli_diagnostics_query:execute(incidents, Ast, #{}, Rows),
    ?assertEqual([<<"early">>, <<"late">>], [maps:get(name, E) || E <- Entries]),
    {ok, Invalid} = wfcli_query_parse:parse("timestamp>=2026-02-30T00:00:00Z"),
    ?assertMatch({error, {query_errors, [_]}},
        wfcli_diagnostics_query:execute(incidents, Invalid, #{}, Rows)),
    {ok, Contains} = wfcli_query_parse:parse("timestamp~2026-09-30T10:00:00Z"),
    ?assertMatch({error, {query_errors, [_]}},
        wfcli_diagnostics_query:execute(incidents, Contains, #{}, Rows)).

relative_time_is_resolved_when_compiled_test() ->
    {ok, Ast} = wfcli_query_parse:parse("timestamp>=now-1h timestamp<=now"),
    Before = erlang:system_time(millisecond),
    {ok, Query} = wfcli_entity_query:compile(Ast, wfcli_diagnostics_query, incidents),
    After = erlang:system_time(millisecond),
    Build = fun(Time) -> wfcli_entity:build(incidents, <<"id">>, <<"test">>,
                           #{<<"timestamp">> => Time}, #{}, #{}) end,
    ?assert(wfcli_entity_query:match(Build(Before - 1000), Query, wfcli_diagnostics_query, incidents)),
    ?assertNot(wfcli_entity_query:match(Build(Before - 3601000), Query, wfcli_diagnostics_query, incidents)),
    ?assertNot(wfcli_entity_query:match(Build(After + 1000), Query, wfcli_diagnostics_query, incidents)).

capture_query_combines_status_and_time_test() ->
    Rows = [#{<<"id">> => <<"capture">>, <<"name">> => <<"Capture">>,
              <<"state">> => <<"armed">>, <<"current">> => true,
              <<"timestamp">> => 1000, <<"expires_at">> => 5000}],
    {ok, Ast} = wfcli_query_parse:parse("current=true state=armed expires_at>1970-01-01T00:00:04Z"),
    ?assertMatch({ok, #{results := #{total := 1, kind := captures}}},
        wfcli_diagnostics_query:execute(captures, Ast, #{}, Rows)).

incident(Id, Time) ->
    #{<<"id">> => Id, <<"name">> => Id, <<"timestamp">> => Time}.

issue(Kind, Identity, Count) ->
    #{<<"kind">> => Kind, <<"identity">> => Identity,
      <<"fallback">> => Identity, <<"scope">> => <<"build_equipment">>,
      <<"reason">> => <<"missing">>, <<"count">> => Count,
      <<"first_seen">> => 1, <<"last_seen">> => Count}.
