-module(retinue_test).
-export([fixtures/0, fixture/1]).

fixtures() ->
    filename:join(filename:dirname(filename:dirname(?FILE)), "fixtures").

fixture(Name) ->
    Path = filename:join(fixtures(), Name),
    case filelib:is_regular(Path) orelse filelib:is_dir(Path) of
        true -> Path;
        false -> error({missing_fixture, Path})
    end.
