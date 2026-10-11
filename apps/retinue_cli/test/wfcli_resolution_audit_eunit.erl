-module(wfcli_resolution_audit_eunit).

-include_lib("eunit/include/eunit.hrl").

finds_nested_projection_gaps_test() ->
    Missing = #{<<"id">> => <<"/Lotus/Test/MissingMod">>,
                <<"name">> => <<"MissingMod">>, <<"name_source">> => <<"path">>,
                <<"role">> => <<"mod">>, <<"asset">> => null},
    Resolved = #{<<"id">> => <<"/Lotus/Test/ResolvedMod">>,
                 <<"name">> => <<"Resolved Mod">>, <<"name_source">> => <<"wfcd">>,
                 <<"asset">> => #{<<"source">> => <<"wfcd">>,
                                    <<"image_name">> => <<"resolved.png">>}},
    Issues = wfcli_resolution_audit:scan(
               #{<<"configs">> => [#{<<"upgrades">> => [Missing, Resolved]}]}),
    ?assertEqual([<<"asset">>, <<"friendly_name">>],
                 lists:sort([maps:get(<<"kind">>, Issue) || Issue <- Issues])),
    ?assert(lists:all(
              fun(Issue) ->
                  maps:get(<<"identity">>, Issue) =:= <<"/Lotus/Test/MissingMod">>
              end, Issues)).

ignores_non_entities_and_absent_optional_asset_fields_test() ->
    ?assertEqual([], wfcli_resolution_audit:scan(
                       #{<<"summary">> => #{<<"name">> => <<"Summary">>},
                         <<"item">> => #{<<"id">> => <<"known">>,
                                         <<"name">> => <<"Known">>,
                                         <<"name_source">> => <<"wfcd">>}})).
