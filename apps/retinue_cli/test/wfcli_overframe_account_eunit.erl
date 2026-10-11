%%%-------------------------------------------------------------------
%% EUnit coverage for persisted Overframe browser sessions.
%%%-------------------------------------------------------------------
-module(wfcli_overframe_account_eunit).

-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

overframe_account_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(State) -> fun() -> exercise(State) end end}.

setup() ->
    Root = filename:join(
             "/tmp", "wfcli-overframe-account-" ++
                 integer_to_list(erlang:unique_integer([positive]))),
    SessionFile = filename:join(Root, "session.json"),
    application:set_env(retinue_daemon, overframe_account_file, SessionFile),
    application:set_env(retinue_daemon, overframe_http_fun, valid_http_fun()),
    #{root => Root, session_file => SessionFile}.

cleanup(#{root := Root}) ->
    application:unset_env(retinue_daemon, overframe_account_file),
    application:unset_env(retinue_daemon, overframe_http_fun),
    _ = file:del_dir_r(Root),
    ok.

exercise(#{session_file := SessionFile}) ->
    {ok, Empty} = wfcli_overframe_account:snapshot(),
    ?assertEqual(false, maps:get(<<"authenticated">>, Empty)),
    ?assertEqual(false, maps:get(<<"stale">>, Empty)),

    Cookies = [cookie(<<"sessionid">>, <<"session-token">>),
               cookie(<<"csrftoken">>, <<"csrf-token">>)],
    {ok, Account} = wfcli_overframe_account:store_session(Cookies),
    ?assertEqual(true, maps:get(<<"authenticated">>, Account)),
    ?assertEqual(<<"TestTenno">>,
                 maps:get(<<"username">>, maps:get(<<"profile">>, Account))),
    {ok, FileInfo} = file:read_file_info(SessionFile),
    ?assertEqual(8#600, FileInfo#file_info.mode band 8#777),

    {ok, Reloaded} = wfcli_overframe_account:snapshot(),
    ?assertEqual(true, maps:get(<<"authenticated">>, Reloaded)),
    {ok, SessionHeaders} = wfcli_overframe_account:session_headers(),
    ?assertEqual("csrf-token", proplists:get_value("x-csrftoken", SessionHeaders)),

    application:set_env(retinue_daemon, overframe_http_fun, stale_http_fun()),
    {ok, Stale} = wfcli_overframe_account:snapshot(),
    ?assertEqual(false, maps:get(<<"authenticated">>, Stale)),
    ?assertEqual(true, maps:get(<<"stale">>, Stale)),

    ?assertMatch({error, _}, wfcli_overframe_account:store_session(
                              [(cookie(<<"sessionid">>, <<"x">>))#{
                                   <<"domain">> => 42}])),
    ?assertMatch({error, _}, wfcli_overframe_account:store_session(
                              [cookie(<<"csrftoken">>, <<"x">>)])),

    {ok, LoggedOut} = wfcli_overframe_account:logout(),
    ?assertEqual(false, maps:get(<<"authenticated">>, LoggedOut)),
    ?assertEqual(false, filelib:is_file(SessionFile)).

cookie(Name, Value) ->
    #{<<"name">> => Name, <<"value">> => Value,
      <<"domain">> => <<".overframe.gg">>, <<"expires">> => -1}.

valid_http_fun() ->
    fun(_Url, Headers) ->
        Header = list_to_binary(proplists:get_value("cookie", Headers)),
        true = binary:match(Header, <<"sessionid=session-token">>) =/= nomatch,
        true = binary:match(Header, <<"csrftoken=csrf-token">>) =/= nomatch,
        {ok, 200, jsone:encode(
                    #{<<"user">> => #{<<"username">> => <<"TestTenno">>}})}
    end.

stale_http_fun() ->
    fun(_Url, _Headers) ->
        {ok, 200, jsone:encode(#{<<"user">> => null})}
    end.
