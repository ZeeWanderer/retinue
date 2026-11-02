%%%-------------------------------------------------------------------
%% EUnit coverage for authenticated Warframe.market HTTP requests.
%%%-------------------------------------------------------------------
-module(wf_market_account_api_eunit).

-include_lib("eunit/include/eunit.hrl").

market_account_api_test_() ->
    {setup, fun setup/0, fun cleanup/1, fun(_State) -> fun exercise/0 end}.

setup() ->
    application:set_env(retinue_daemon, market_request_interval_ms, 0),
    ok.

cleanup(_State) ->
    application:unset_env(retinue_daemon, market_account_http_fun),
    application:unset_env(retinue_daemon, market_request_interval_ms).

exercise() ->
    Test = self(),
    application:set_env(
      retinue_daemon, market_account_http_fun,
      fun(Method, Url, Headers, Body) ->
          Test ! {market_http, Method, Url, Headers, Body},
          response(Method, Url)
      end),

    ?assertEqual({ok, <<"login-token">>},
                 wf_market_account_api:login(<<"tenno@example.test">>, <<"secret">>)),
    {post, LoginUrl, LoginHeaders, LoginBody} = next_request(),
    ?assert(lists:suffix("/v1/auth/signin", LoginUrl)),
    ?assertEqual("JWT", proplists:get_value("authorization", LoginHeaders)),
    ?assertEqual(<<"tenno@example.test">>,
                 maps:get(<<"email">>, jsone:decode(LoginBody))),

    {ok, Profile, <<"rotated-token">>} =
        wf_market_account_api:profile(<<"login-token">>),
    ?assertEqual(<<"Tenno">>, maps:get(<<"ingameName">>, Profile)),
    {get, ProfileUrl, ProfileHeaders, <<>>} = next_request(),
    ?assert(lists:suffix("/v2/me", ProfileUrl)),
    ?assertEqual("Bearer login-token",
                 proplists:get_value("authorization", ProfileHeaders)),

    {ok, [Order], undefined} = wf_market_account_api:orders(<<"rotated-token">>),
    ?assertEqual(<<"order-1">>, maps:get(<<"id">>, Order)),
    {get, OrdersUrl, _OrdersHeaders, <<>>} = next_request(),
    ?assert(lists:suffix("/v2/orders/my", OrdersUrl)),

    Create = #{<<"itemId">> => <<"item-1">>, <<"type">> => <<"sell">>,
               <<"platinum">> => 10, <<"quantity">> => 1, <<"visible">> => true},
    {ok, _, undefined} = wf_market_account_api:create_order(<<"token">>, Create),
    {post, CreateUrl, _CreateHeaders, CreateBody} = next_request(),
    ?assert(lists:suffix("/v2/order", CreateUrl)),
    ?assertEqual(Create, jsone:decode(CreateBody)),

    Patch = #{<<"platinum">> => 12, <<"visible">> => false},
    {ok, _, undefined} =
        wf_market_account_api:update_order(<<"token">>, <<"order/1">>, Patch),
    {patch, PatchUrl, _PatchHeaders, PatchBody} = next_request(),
    ?assert(lists:suffix("/v2/order/order%2F1", PatchUrl)),
    ?assertEqual(Patch, jsone:decode(PatchBody)),

    {ok, _, undefined} = wf_market_account_api:close_order(<<"token">>, <<"order-1">>, 2),
    {post, CloseUrl, _CloseHeaders, CloseBody} = next_request(),
    ?assert(lists:suffix("/v2/order/order-1/close", CloseUrl)),
    ?assertEqual(2, maps:get(<<"quantity">>, jsone:decode(CloseBody))),

    {ok, _, undefined} = wf_market_account_api:delete_order(<<"token">>, <<"order-1">>),
    {delete, DeleteUrl, _DeleteHeaders, <<>>} = next_request(),
    ?assert(lists:suffix("/v2/order/order-1", DeleteUrl)),

    {ok, _, undefined} =
        wf_market_account_api:set_group_visibility(<<"token">>, false, <<"sell">>),
    {patch, GroupUrl, _GroupHeaders, GroupBody} = next_request(),
    ?assert(lists:suffix("/v2/orders/group/all", GroupUrl)),
    ?assertEqual(#{<<"type">> => <<"sell">>, <<"visible">> => false},
                 jsone:decode(GroupBody)),

    application:set_env(
      retinue_daemon, market_account_http_fun,
      fun(_Method, _Url, _Headers, _Body) ->
          {ok, 401, [], jsone:encode(#{<<"error">> => <<"unauthorized">>})}
      end),
    ?assertEqual({error, market_account_unauthorized},
                 wf_market_account_api:orders(<<"expired">>)).

response(post, Url) ->
    case lists:suffix("/v1/auth/signin", Url) of
        true -> {ok, 200, [{"Authorization", "JWT login-token"}], <<"{}">>};
        false -> envelope(#{<<"id">> => <<"created">>})
    end;
response(get, Url) ->
    case lists:suffix("/v2/me", Url) of
        true ->
            {ok, 200, [{"Authorization", "JWT rotated-token"}],
             jsone:encode(#{<<"error">> => null,
                            <<"data">> => #{<<"verification">> => true,
                                            <<"ingameName">> => <<"Tenno">>}})};
        false -> envelope([#{<<"id">> => <<"order-1">>}])
    end;
response(_Method, _Url) -> envelope(#{<<"id">> => <<"updated">>}).

envelope(Data) ->
    {ok, 200, [], jsone:encode(#{<<"error">> => null, <<"data">> => Data})}.

next_request() ->
    receive
        {market_http, Method, Url, Headers, Body} -> {Method, Url, Headers, Body}
    after 1000 ->
        error(market_http_not_called)
    end.
