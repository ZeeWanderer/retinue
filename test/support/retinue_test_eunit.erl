-module(retinue_test_eunit).
-include_lib("eunit/include/eunit.hrl").

fixture_paths_do_not_depend_on_test_working_directory_test() ->
    Path = retinue_test:fixture("worldstate_sample.json"),
    ?assertEqual(absolute, filename:pathtype(Path)),
    ?assert(filelib:is_regular(Path)),
    ?assert(filelib:is_dir(retinue_test:fixture("exports"))).

missing_fixture_fails_before_a_source_can_fetch_it_test() ->
    Path = filename:join(retinue_test:fixtures(), "not-a-fixture.json"),
    ?assertError({missing_fixture, Path}, retinue_test:fixture("not-a-fixture.json")).
