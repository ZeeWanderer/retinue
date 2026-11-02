-module(wf_mod_catalog_eunit).

-include_lib("eunit/include/eunit.hrl").

card_text_uses_the_requested_rank_test() ->
    Details = #{compat => <<"Shotgun">>, level_stats =>
        [#{<<"stats">> => [<<"+10% Damage">>]},
         #{<<"stats">> => [<<"+20% Damage">>]},
         #{<<"stats">> => [<<"+30% Damage">>, <<"On Kill: +5% Fire Rate">>]}]},
    lists:foreach(fun({Rank, Effects}) ->
        ?assertEqual(#{<<"compat_name">> => <<"Shotgun">>, <<"effects">> => Effects},
                     wf_mod_catalog:card_text(Rank, Details))
    end, [{0, [<<"+10% Damage">>]}, {1, [<<"+20% Damage">>]},
          {2, [<<"+30% Damage">>, <<"On Kill: +5% Fire Rate">>]}]),
    lists:foreach(fun(Rank) ->
        ?assertEqual(#{<<"compat_name">> => <<"Shotgun">>},
                     wf_mod_catalog:card_text(Rank, Details))
    end, [undefined, null, -1, 3]).

card_text_tolerates_missing_or_incomplete_metadata_test() ->
    lists:foreach(fun(Details) ->
        ?assertEqual(#{}, wf_mod_catalog:card_text(0, Details))
    end, [undefined, #{}, #{compat => null}, #{level_stats => null},
          #{level_stats => [null]}, #{level_stats => [#{}]},
          #{level_stats => [#{<<"stats">> => null}]}]).
