%%%-------------------------------------------------------------------
%% EUnit coverage for daemon-owned Market login and order state.
%%%-------------------------------------------------------------------
-module(wfcli_market_account_service_eunit).

-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

market_account_service_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(State) -> fun() -> exercise(State) end end}.

setup() ->
    Root = filename:join(
             "/tmp", "wfcli-market-account-" ++
                 integer_to_list(erlang:unique_integer([positive]))),
    TokenFile = filename:join(Root, "market-token"),
    State = ets:new(market_account_state, [set, public]),
    ets:insert(State, {orders, [order(<<"order-1">>, 10, 2)]}),
    application:set_env(retinue_daemon, market_account_file, TokenFile),
    application:set_env(retinue_daemon, market_request_interval_ms, 0),
    application:set_env(retinue_daemon, market_account_http_fun, http_fun(State)),
    {ok, _Service} = wfcli_market_account_service:start_link(),
    #{root => Root, token_file => TokenFile, state => State}.

cleanup(#{root := Root, state := State}) ->
    stop(wfcli_market_account_service),
    application:unset_env(retinue_daemon, market_account_file),
    application:unset_env(retinue_daemon, market_request_interval_ms),
    application:unset_env(retinue_daemon, market_account_http_fun),
    ets:delete(State),
    _ = file:del_dir_r(Root),
    ok.

exercise(#{token_file := TokenFile, state := State}) ->
    {ok, Empty} = request(#{action => snapshot}),
    ?assertEqual(false, maps:get(<<"authenticated">>, Empty)),

    {ok, LoggedIn} = request(#{action => login, email => <<"tenno@example.test">>,
                               password => <<"secret">>}),
    ?assertEqual(true, maps:get(<<"authenticated">>, LoggedIn)),
    ?assertEqual(<<"Tenno">>,
                 maps:get(<<"ingameName">>, maps:get(<<"profile">>, LoggedIn))),
    ?assertEqual(1, length(maps:get(<<"orders">>, LoggedIn))),
    {ok, <<"rotated-token">>} = file:read_file(TokenFile),
    {ok, FileInfo} = file:read_file_info(TokenFile),
    ?assertEqual(8#600, FileInfo#file_info.mode band 8#777),

    NewOrder = #{<<"itemId">> => <<"item-2">>, <<"type">> => <<"buy">>,
                 <<"platinum">> => 7, <<"quantity">> => 1, <<"visible">> => true},
    {ok, Created} = request(#{action => create_order, order => NewOrder}),
    ?assertEqual(2, length(maps:get(<<"orders">>, Created))),

    {ok, Updated} = request(#{action => update_order, id => <<"order-1">>,
                              patch => #{<<"platinum">> => 15}}),
    [Patched] = [Order || Order <- maps:get(<<"orders">>, Updated),
                          maps:get(<<"id">>, Order) =:= <<"order-1">>],
    ?assertEqual(15, maps:get(<<"platinum">>, Patched)),

    {ok, Closed} = request(#{action => close_order, id => <<"order-1">>, quantity => 1}),
    [Reduced] = [Order || Order <- maps:get(<<"orders">>, Closed),
                          maps:get(<<"id">>, Order) =:= <<"order-1">>],
    ?assertEqual(1, maps:get(<<"quantity">>, Reduced)),

    {ok, Hidden} = request(#{action => set_visibility, visible => false}),
    ?assert(lists:all(fun(Order) -> maps:get(<<"visible">>, Order) =:= false end,
                      maps:get(<<"orders">>, Hidden))),

    {ok, Deleted} = request(#{action => delete_order, id => <<"order-1">>}),
    ?assertEqual(1, length(maps:get(<<"orders">>, Deleted))),
    ?assertEqual(1, length(ets:lookup_element(State, orders, 2))),

    {ok, LoggedOut} = request(#{action => logout}),
    ?assertEqual(false, maps:get(<<"authenticated">>, LoggedOut)),
    ?assertEqual(false, filelib:is_file(TokenFile)),
    ?assertMatch(#{authenticated := false, orders := 0},
                 wfcli_market_account_service:status()).

http_fun(State) ->
    fun(Method, Url, _Headers, Body) ->
        case {Method, path(Url)} of
            {post, "/v1/auth/signin"} ->
                {ok, 200, [{"Authorization", "JWT login-token"}], <<"{}">>};
            {get, "/v2/me"} ->
                response(#{<<"verification">> => true,
                           <<"ingameName">> => <<"Tenno">>},
                         [{"Authorization", "JWT rotated-token"}]);
            {get, "/v2/orders/my"} -> response(ets:lookup_element(State, orders, 2));
            {post, "/v2/order"} -> create(State, jsone:decode(Body));
            {patch, "/v2/order/order-1"} -> update(State, <<"order-1">>, jsone:decode(Body));
            {post, "/v2/order/order-1/close"} ->
                close(State, <<"order-1">>, maps:get(<<"quantity">>, jsone:decode(Body)));
            {delete, "/v2/order/order-1"} -> delete(State, <<"order-1">>);
            {patch, "/v2/orders/group/all"} -> visibility(State, jsone:decode(Body));
            Other -> {error, {unexpected_market_request, Other}}
        end
    end.

create(State, Order0) ->
    Order = Order0#{<<"id">> => <<"order-2">>},
    update_orders(State, fun(Orders) -> Orders ++ [Order] end),
    response(Order).

update(State, Id, Patch) ->
    update_orders(State, fun(Orders) ->
        [case maps:get(<<"id">>, Order) of Id -> maps:merge(Order, Patch); _ -> Order end
         || Order <- Orders]
    end),
    response(#{}).

close(State, Id, Quantity) ->
    update_orders(State, fun(Orders) ->
        [Order#{<<"quantity">> => maps:get(<<"quantity">>, Order) - Quantity}
         || Order <- Orders, maps:get(<<"id">>, Order) =:= Id] ++
        [Order || Order <- Orders, maps:get(<<"id">>, Order) =/= Id]
    end),
    response(#{}).

delete(State, Id) ->
    update_orders(State, fun(Orders) ->
        [Order || Order <- Orders, maps:get(<<"id">>, Order) =/= Id]
    end),
    response(#{}).

visibility(State, Patch) ->
    Visible = maps:get(<<"visible">>, Patch),
    Type = maps:get(<<"type">>, Patch, undefined),
    update_orders(State, fun(Orders) ->
        [case Type =:= undefined orelse maps:get(<<"type">>, Order) =:= Type of
             true -> Order#{<<"visible">> => Visible};
             false -> Order
         end || Order <- Orders]
    end),
    response(#{}).

update_orders(State, Fun) ->
    ets:insert(State, {orders, Fun(ets:lookup_element(State, orders, 2))}).

order(Id, Platinum, Quantity) ->
    #{<<"id">> => Id, <<"itemId">> => <<"item-1">>, <<"type">> => <<"sell">>,
      <<"platinum">> => Platinum, <<"quantity">> => Quantity, <<"visible">> => true}.

response(Data) -> response(Data, []).
response(Data, Headers) ->
    {ok, 200, Headers,
     jsone:encode(#{<<"error">> => null, <<"data">> => Data})}.

path(Url) ->
    Uri = uri_string:parse(Url),
    maps:get(path, Uri).

request(Request) ->
    {ok, Ref} = wfcli_market_account_service:submit(self(), Request),
    receive
        {wfcli_market_account, Ref, Reply} -> Reply
    after 3000 ->
        error({market_account_timeout, Request})
    end.

stop(Name) ->
    case whereis(Name) of
        undefined -> ok;
        _Pid -> gen_server:stop(Name)
    end.
